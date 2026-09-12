"""A new chat's pane opens at the profile selector, and no harness starts until one is picked.

The selector is the F2 palette's picker surface — type to filter, Enter to choose, Esc to
cancel — drawn in the chat's OWN pane before any harness has run there (ADR 0018 as the spec
amends it). It always shows, even where one profile is available: skipping it would bring
back the harness nobody picked on a one-harness machine, and one profile costs one Enter.

**Enter on a refused row does not close it** (ruling 6). The palette closes on a refused
Enter and puts the reason on the attention row; closing here would close the chat. So the
reason goes in the footer, the row stays, and only Esc closes the window — which is also
what a pick the fresh check at launch refuses does (ruling 30).

**A waiting pane is not a chat.** It has a tab, so it can be left and come back to; it is
not in the quit manifest and is never reopened; its kind and its profile are recorded at the
pick and not before, which is what makes Esc "nothing has happened yet" rather than #518's
"a launch that half happened".

**A row's approval and wiring come from Tasks 3 and 4 themselves.** "Not approved yet" is
`profiletrust.approval_needed` reading a real launch record, and "not wired" is `wiring`
asking the harness through the same `util.run` seam Task 4's own module drives
(`tests/test_a_profile_is_wired_or_refuses.spawns`). A case whose subject is something else
says so the way Task 4's modules do: every declared profile approved, and wiring stated —
`wiring.detect` answering WIRED for the rows, `tests._isolation.wired_as_today` for a launch,
which is otherwise refused "could not tell" before it reaches anything.

**The approval is asked in the pane, by the launch, in Task 3's words** — the selector draws
no prompt of its own. So the cases about a new or changed profile drive
`launcher.cmd_frame_launch --select` with a terminal on both ends, the way Task 3's own
module drives `launcher.start`.
"""

from __future__ import annotations

import inspect
import itertools
import json
import os
import shutil
import subprocess
import sys
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

import re
import threading

from charter import (commands_frame, config, contain, doctor, profiles, profiletrust, tui,
                     util, wiring)
from charter.frame import (chats, launcher, leave, overlay, palette, selector, state,
                           tmuxctl)
from tests import _gitguard, _tmuxreap, _tmuxsocket, _ttyguard
from tests._isolation import (APipe, ATerminal, PersonaIso, Typed, approve_every_profile,
                              approve_profile, declare_profiles, make_plane,
                              no_background_refresh, no_update_check_in, wired_as_today)
from tests.test_a_profile_is_wired_or_refuses import entry, listing, spawns

#: A profile whose row can run: on `PATH`, approved and wired. The fixture's `which` says
#: yes to everything, every declared profile is approved, and `wiring.detect` answers WIRED
#: unless a case says otherwise.
WORK = "claude-work"


class _APlaneWithProfiles(PersonaIso):
    """A plane declaring `claude-work` and `codex-pinned`, in a git repository that ignores
    the local file — `tests._isolation.declare_profiles`' fixture, which runs a real
    `git status` because `profiles.ignore_check` does.

    `shutil.which` answers a path for every program, so a built-in is listed and a declared
    profile is not refused for being absent from this machine's `PATH`. A case about either
    says so itself.

    Every declared profile is APPROVED — a real launch record, `approve_every_profile` — and
    `wiring.detect` answers WIRED, recording whom it was asked about in :attr:`detected`. A
    case about either says so itself: `_unapprove` for the first, and :attr:`REAL_WIRING`
    for a class that drives `wiring` through `util.run`.
    """

    #: Leave `wiring.detect` alone, for the cases whose subject is what it answers.
    REAL_WIRING = False

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.local = declare_profiles(self)
        self.which = self.enterContext(mock.patch.object(
            selector.shutil, "which", side_effect=lambda cmd, **kw: f"/usr/bin/{cmd}"))
        approve_every_profile(self)
        self.detected: list[str] = []
        if not self.REAL_WIRING:
            self.enterContext(mock.patch.object(wiring, "detect", side_effect=self._wired))

    def _wired(self, p, *, cwd):
        self.detected.append(p.name)
        return wiring.Wiring(wiring.WIRED, "stood in for the probe", "")

    def _unapprove(self) -> None:
        """No launch record for anything: every declared profile is new again."""
        (config.STATE_DIR / profiletrust.RECORD).unlink()

    def _read(self) -> profiles.ProfileSet:
        return selector.read(config.ROOT)

    def _rows(self, *, start: str | None = None,
              after: "selector.Refused | None" = None) -> tuple[overlay.Row, ...]:
        return selector.rows(self._read(), cwd=Path(config.ROOT), start=start, after=after)

    def _row(self, rows, name: str) -> overlay.Row:
        found = [r for r in rows if r.id == selector.ROW_PREFIX + name]
        self.assertEqual(len(found), 1, f"{name} is not listed once in {[r.id for r in rows]}")
        return found[0]

    def _names(self, rows) -> list[str]:
        return [r.id.removeprefix(selector.ROW_PREFIX) for r in rows]


class TheRows(_APlaneWithProfiles, unittest.TestCase):
    """Declared profiles always; a built-in only when its program is installed; a profile
    that cannot start stays listed with its reason on the row (#512, and the spec)."""

    def test_a_declared_profile_is_listed_even_when_its_command_is_absent(self):
        self.which.side_effect = lambda cmd, **kw: None
        rows = self._rows()
        row = self._row(rows, WORK)
        self.assertTrue(row.refused)
        self.assertIn("not on PATH", row.note)
        self.assertIn("claude", row.note)

    def test_a_built_in_is_listed_only_when_its_program_is_installed(self):
        self.assertIn("codex", self._names(self._rows()))
        self.which.side_effect = lambda cmd, **kw: None if cmd == "codex" else "/usr/bin/x"
        self.assertNotIn("codex", self._names(self._rows()))

    def test_a_refused_profile_is_listed_with_its_reason(self):
        self.local.write_text('[harness.broken]\nkind = "claude"\ncommand = "claude -x"\n')
        row = self._row(self._rows(), "broken")
        self.assertTrue(row.refused)
        self.assertIn("never a shell string", row.note)

    def test_a_committable_file_refuses_every_declared_row_and_no_built_in(self):
        """F1, through `profiles.with_ignore_check` — the same call `charter harness list`
        makes, so the selector and the listing say one thing about one file."""
        (config.ROOT / ".gitignore").write_text("")
        rows = self._rows()
        self.assertTrue(self._row(rows, WORK).refused)
        self.assertIn("charter reinit", self._row(rows, WORK).note)
        self.assertFalse(self._row(rows, "claude").refused)

    def test_the_row_state_of_a_runnable_profile_names_its_kind_and_command(self):
        row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused)
        self.assertIn("claude", row.note)
        self.assertIn("CLAUDE_CONFIG_DIR=", row.note)

    def test_the_start_row_is_marked_and_nothing_else_is(self):
        rows = self._rows(start=WORK)
        self.assertEqual([self._names(rows)[i] for i, r in enumerate(rows) if r.mark],
                         [WORK])

    def test_a_default_naming_a_missing_profile_marks_nothing(self):
        """Ruling 18: it marks no row — and Enter must still do something, which is the
        case below."""
        self.assertEqual([r for r in self._rows(start="gone") if r.mark], [])

    def _with_a_refused_first_row(self) -> tuple[overlay.Row, ...]:
        """A list whose FIRST row cannot run, so `palette.aim`'s answer is not index 0.

        A declared replacement of the first built-in, because only a declared profile can be
        listed-and-refused: a built-in whose program is absent is not a row at all.
        """
        first = next(iter(profiles.builtins()))
        self.local.write_text(f'[harness.{first}]\nkind = "{first}"\n'
                              'command = ["nowhere-at-all"]\n'
                              f'[harness.{WORK}]\nkind = "claude"\ncommand = ["claude"]\n')
        self.which.side_effect = lambda cmd, **kw: (None if cmd == "nowhere-at-all"
                                                    else f"/usr/bin/{cmd}")
        rows = self._rows()
        self.assertTrue(rows[0].refused, self._names(rows))
        return rows

    def test_a_default_naming_a_missing_profile_puts_the_cursor_where_palette_aim_does(self):
        """Ruling 18 and review 10. The first row that can run, which is `palette.aim`'s own
        rule, asked of the rows rather than restated."""
        rows = self._with_a_refused_first_row()
        self.assertEqual(selector.opens_on(rows, "gone"), "")
        surface = selector.Selector(catalogue=rows, on=selector.opens_on(rows, "gone"))
        self.assertIs(surface.selected, rows[palette.aim(rows)])
        self.assertFalse(surface.selected.refused)

    def test_the_cursor_opens_on_the_start_row_when_that_row_can_run(self):
        rows = self._rows(start=WORK)
        self.assertEqual(selector.opens_on(rows, WORK), WORK)
        surface = selector.Selector(catalogue=rows, on=selector.opens_on(rows, WORK))
        self.assertEqual(surface.selected.id, selector.ROW_PREFIX + WORK)

    def test_the_cursor_leaves_a_start_row_that_cannot_run_to_palette_aim(self):
        """Ruling 18's other half: a `default` that IS declared and cannot run is the same
        case for the operator, because Enter on it can only say why."""
        rows = self._with_a_refused_first_row()
        first = self._names(rows)[0]
        self.assertEqual(selector.opens_on(rows, first), "")
        surface = selector.Selector(catalogue=rows, on=selector.opens_on(rows, first))
        self.assertFalse(surface.selected.refused)
        self.assertIs(surface.selected, rows[palette.aim(rows)])

    def test_typing_re_aims_by_the_palettes_own_rule(self):
        """The opening position is the only one the selector chooses (`palette.aim`'s own
        note): after a keystroke the operator is looking at a different list."""
        rows = self._rows(start=WORK)
        surface = selector.Selector(catalogue=rows, on=WORK)
        surface.handle(overlay.Event(overlay.KEY, "c"), 24)
        self.assertIs(surface.selected, surface.rows[palette.aim(surface.rows)])

    def test_no_start_row_aims_at_no_row_at_all(self):
        """`Selector.on` is empty for a chat that named no profile, and empty must mean
        *nothing to look for* rather than *look for the row with no name*. Two lines keep
        that from opposite sides — this one, and `rows` refusing to build a row for a
        refusal with no name — so the row id this asks about is one nothing can produce
        today and the guard is what makes that safe to rely on.
        """
        listed = (overlay.Row(id=selector.ROW_PREFIX + WORK, title=WORK, note=""),
                  overlay.Row(id=selector.ROW_PREFIX, title="", note=""))
        surface = selector.Selector(catalogue=listed, on="")
        self.assertEqual(surface.selected.id, selector.ROW_PREFIX + WORK)
        self.assertIs(surface.selected, surface.rows[palette.aim(surface.rows)])

    def test_control_bytes_in_a_command_are_escaped_in_its_row(self):
        """Ruling 35: `charter.local.toml` is a file a chat can write, and a `\\r` or an ESC
        in a command could otherwise redraw the row to show a harmless command."""
        self.local.write_text('[harness.sneaky]\nkind = "claude"\n'
                              'command = ["cl\\raude\\u001b[2Kharmless"]\n')
        approve_profile(self, "sneaky")
        row = self._row(self._rows(), "sneaky")
        line = "".join(selector.Selector(catalogue=(row,)).render(200, 6))
        for text in (row.note, line):
            self.assertNotIn("\r", text)
            self.assertNotIn("\x1b[2K", text)
        self.assertIn("\\u000d", row.note)

    def test_a_long_profile_name_is_whole_in_its_row_and_counted_where_it_is_cut(self):
        """`profiles.NAME_RE` bounds a name's shape and says nothing about its length, so
        the title is the one value here that arrives printable and unbounded — and ruling
        45: a row the operator chooses from says how much it hid. So the ROW holds the name
        whole, and the pane that cannot fit it says by how much, counted from the whole name
        rather than from what a fixed `...` left of it."""
        name = "w" * 200
        self.local.write_text('[harness.%s]\nkind = "claude"\ncommand = ["claude"]\n'
                              % name)
        approve_profile(self, name)
        row = self._row(self._rows(), name)
        self.assertEqual(row.title, name)
        line = selector.Selector(catalogue=(row,)).render(80, 6)[1]
        kept = len(re.search(r"(w+)…", line).group(1))
        self.assertIn(f"… +{200 - kept} not shown", line)
        self.assertNotIn("...", line)

    def test_a_name_with_an_escape_in_a_hand_built_profile_is_shown_escaped(self):
        """`profiles.NAME_RE` keeps a control byte out of every name a FILE declares, so the
        containment of a row's title is reachable only through a `ProfileSet` built by hand —
        which is what `rows` takes, and what the next reader of it may hand over. The row
        title is contained where it is made, not left to the surface (ruling 35)."""
        name = "cl\x1b[2Kaude"
        p = profiles.Profile(name=name, kind="claude", harness="claude-code",
                             command=("claude",), env=(), source=profiles.BUILTIN)
        have = profiles.ProfileSet(profiles={name: p}, refused=(), default=None,
                                   default_from=None, default_refused=None)
        (row,) = selector.rows(have, cwd=Path(config.ROOT))
        self.assertNotIn("\x1b", row.title)
        self.assertEqual(row.title, contain.readable(name, contain.NO_CLIP))

    def test_a_long_command_and_environment_reach_the_row_whole(self):
        """Ruling 45 at the row's source: `profiles.display` clips each piece at
        `contain.DISPLAY_LIMIT` with a fixed `...` unless told not to, and a row that then
        counted its cut would be counting what that left. The row holds the value whole;
        the surface says how much of it the pane took."""
        home, arg = "h" * 300, "a" * 300
        self.local.write_text('[harness.long]\nkind = "claude"\n'
                              f'command = ["claude", "{arg}"]\n'
                              f'env = {{ CLAUDE_CONFIG_DIR = "{home}" }}\n')
        approve_profile(self, "long")
        note = self._row(self._rows(), "long").note
        self.assertIn(arg, note)
        self.assertIn(f"CLAUDE_CONFIG_DIR={home}", note)
        self.assertNotIn("...", note)

    def test_a_long_command_a_machine_lacks_is_named_whole(self):
        program = "p" * 300
        self.local.write_text(f'[harness.long]\nkind = "claude"\ncommand = ["{program}"]\n')
        self.which.side_effect = lambda cmd, **kw: None
        note = self._row(self._rows(), "long").note
        self.assertEqual(note, selector.NOT_ON_PATH.format(cmd=program))

    def test_a_refusal_at_the_launch_marks_the_row_it_names_and_no_other(self):
        """*after* is ONE profile's refusal — the one the launch just checked again. A list
        that spread it over every row would tell an operator that the profile they did not
        pick cannot start either, and Enter on that row would answer with somebody else's
        sentence instead of starting a harness that was fine all along."""
        rows = self._rows(after=selector.Refused(WORK, "the reason it gave"))
        picked = self._row(rows, WORK)
        self.assertTrue(picked.refused)
        self.assertIn("the reason it gave", picked.note)
        other = self._row(rows, "codex-pinned")
        self.assertFalse(other.refused)
        self.assertNotIn("the reason it gave", other.note)

    def test_the_command_a_machine_lacks_is_named_escaped(self):
        """Ruling 35 again, on the other sentence a row can carry: `not on PATH` quotes the
        profile's own `command`, and `charter.local.toml` is a file a chat can write."""
        self.local.write_text('[harness.sneaky]\nkind = "claude"\n'
                              'command = ["cl\\u001b[2Kear"]\n')
        self.which.side_effect = lambda cmd, **kw: None
        note = self._row(self._rows(), "sneaky").note
        self.assertIn("not on PATH", note)
        self.assertNotIn("\x1b", note)
        self.assertIn(contain.readable("cl\x1b[2Kear"), note)

    def test_a_refusal_naming_the_whole_file_is_not_a_row(self):
        """`profiles.Refused` spells a whole-file refusal with an EMPTY name — unreadable
        TOML, or a `harness` key that is not a table — and there is no profile behind one to
        start and no name to put in its title. A row for it is a blank line Enter cannot
        act on, in a list whose every other row is a thing to run. The sentence is not
        lost: `charter harness list` and `charter doctor` both print `r.name or r.source`,
        so it reads as `charter.local.toml: <why>` where a reader whose file will not parse
        is already going.
        """
        self.local.write_text('harness = "not a table"\n')
        self.assertIn("", [r.name for r in self._read().refused])
        rows = self._rows()
        self.assertTrue(rows, "the built-ins are still listed")
        self.assertNotIn(selector.ROW_PREFIX, [r.id for r in rows])
        self.assertTrue(all(r.title for r in rows), self._names(rows))


class ARowSaysWhetherItIsApproved(_APlaneWithProfiles, unittest.TestCase):
    """Task 3 on the rows: a new or changed profile is NOT refused — Enter starts the launch,
    and the launch asks — and it is never probed, because a probe runs the profile's own
    command and only a yes stands for the operator's approval of it (ruling 1)."""

    def test_a_new_profile_is_not_refused_and_says_it_is_not_approved_yet(self):
        self._unapprove()
        row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused)
        self.assertEqual(row.note, selector.NOT_APPROVED.format(state=profiletrust.NEW))
        self.assertIn("not approved yet (new) — Enter shows its command", row.note)

    def test_a_changed_profile_says_changed(self):
        self.local.write_text(self.local.read_text().replace(
            'command = ["claude"]', 'command = ["claude", "--pinned"]'))
        row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused)
        self.assertIn("not approved yet (changed)", row.note)

    def test_an_unapproved_profile_is_never_probed(self):
        self._unapprove()
        self._rows()
        self.assertNotIn(WORK, self.detected)
        self.assertIn("claude", self.detected, "a built-in never asks, so it is probed")

    def test_a_built_in_never_says_it_is_not_approved(self):
        self._unapprove()
        self.assertNotIn("not approved", self._row(self._rows(), "claude").note)


class ARowSaysWhetherItIsWired(_APlaneWithProfiles, unittest.TestCase):
    """Task 4 on the rows, asked of the harness through `util.run` exactly as `wiring`'s own
    module asks it (`spawns`). A profile that is not wired is refused with Task 4's own
    sentence — the one the launch says a moment later — and the answer is remembered for
    the next paint and never for a launch (ruling 21)."""

    REAL_WIRING = True

    def setUp(self) -> None:
        super().setUp()
        # `claude` and nothing else, so the rows that probe are the built-in and the one
        # declared profile that runs it: `codex-pinned` runs `npx`, which is not "here".
        self.which.side_effect = lambda cmd, **kw: "/usr/bin/claude" if cmd == "claude" else None

    def test_an_unwired_profile_is_refused_with_task_fours_sentence_and_its_fix(self):
        with spawns([], listing()):
            row = self._row(self._rows(), WORK)
        self.assertTrue(row.refused)
        self.assertIn(f"profile '{WORK}' is not wired", row.note)
        self.assertIn(f"charter harness install {WORK}", row.note)
        self.assertIn("so a chat on it would run without charter's guard", row.note)

    def test_a_profile_charter_could_not_ask_is_refused_with_its_own_sentence(self):
        """Ruling 12: an unknown is not a pass, and "could not look" is not "looked and the
        guard is absent" — two sentences, and the row says the one that is true."""
        with spawns([], "not json at all"):
            row = self._row(self._rows(), WORK)
        self.assertTrue(row.refused)
        self.assertIn("charter could not ask", row.note)
        self.assertNotIn("is not wired", row.note)

    def test_a_wired_profile_shows_its_kind_and_command(self):
        with spawns([], listing(entry(scope="user"))):
            row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused, row.note)
        self.assertIn("claude · CLAUDE_CONFIG_DIR=", row.note)

    def test_each_profile_is_asked_under_its_own_environment(self):
        calls: list = []
        with spawns(calls, listing(entry(scope="user"))):
            self._rows()
        seen = sorted(c.env.get("CLAUDE_CONFIG_DIR", "") for c in calls)
        self.assertEqual(seen, ["", str(self.home / ".cw")])

    def test_a_miss_is_remembered_and_the_next_paint_spawns_nothing(self):
        """The spec's *the selector reads a stamped cache to draw its rows*: a warm open is
        a file read, and it is the same answer the probe gave."""
        with spawns([], listing(entry(scope="user"))):
            self._rows()
        p = profiles.current().profiles[WORK]
        self.assertEqual(wiring.cached(p, cwd=Path(config.ROOT)).state, wiring.WIRED)
        again: list = []
        with spawns(again, raises=AssertionError("probed again")):
            row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused)
        self.assertEqual(again, [])

    def test_a_remembered_answer_is_the_row_and_is_not_asked_again(self):
        """A hit is DRAWN, and that is all it may do: the entry says unwired, the row says
        so in the words Task 4 would, and the profile's own command is not run for it."""
        p = profiles.current().profiles[WORK]
        wiring.remember(p, cwd=Path(config.ROOT),
                        w=wiring.Wiring(wiring.UNWIRED, "remembered as unwired", "the fix"))
        calls: list = []
        with spawns(calls, listing(entry(scope="user"))):
            row = self._row(self._rows(), WORK)
        self.assertTrue(row.refused)
        self.assertIn("remembered as unwired", row.note)
        self.assertNotIn(str(self.home / ".cw"),
                         [c.env.get("CLAUDE_CONFIG_DIR", "") for c in calls])

    def test_a_row_whose_probe_could_not_tell_is_asked_again_next_draw(self):
        """"Could not ask" is not remembered (the coordinator's ruling): one probe that hit
        its timeout would otherwise refuse the row for the cache's whole day, and Enter on a
        refused row never reaches the launch's fresh probe."""
        answers = [wiring.Wiring(wiring.UNKNOWN_STATE, "timed out", "the fix"),
                   wiring.Wiring(wiring.WIRED, "asked again", "")]
        asked: list[str] = []

        def once_unknown(p, *, cwd):
            asked.append(p.name)
            return answers[0] if asked.count(p.name) == 1 else answers[1]

        with mock.patch.object(wiring, "detect", side_effect=once_unknown):
            first = self._row(self._rows(), WORK)
            second = self._row(self._rows(), WORK)
        self.assertTrue(first.refused)
        self.assertIn("could not ask", first.note)
        self.assertFalse(second.refused, second.note)
        self.assertEqual(asked.count(WORK), 2)

    def test_every_miss_is_probed_at_once(self):
        """The plan's stop rule for a cold open is priced with the probes CONCURRENT — one
        after another, a plane of four Claude profiles pays a second before the first row."""
        both = threading.Barrier(2, timeout=10)

        def meets_the_other(p, *, cwd):
            both.wait()
            return wiring.Wiring(wiring.WIRED, "met", "")

        with mock.patch.object(wiring, "detect", side_effect=meets_the_other):
            rows = self._rows()
        self.assertFalse(self._row(rows, WORK).refused)
        self.assertFalse(self._row(rows, "claude").refused)

    def test_a_command_not_on_path_and_the_row_the_launch_refused_are_never_probed(self):
        """Each is already saying why, and a probe runs the profile's own command."""
        asked: list[str] = []
        with mock.patch.object(wiring, "detect", side_effect=lambda p, *, cwd: asked.append(
                p.name) or wiring.Wiring(wiring.WIRED, "", "")):
            self._rows(after=selector.Refused(WORK, "the launch said no"))
        self.assertEqual(asked, ["claude"])


class _APickedSelector(_APlaneWithProfiles):
    """`palette.own_the_tty` answering a queue of rows, and every surface it was handed."""

    def setUp(self) -> None:
        super().setUp()
        self.surfaces: list = []
        self.answers: list = []
        self.enterContext(mock.patch.object(
            selector.palette, "own_the_tty", side_effect=self._answer))

    def _answer(self, surface, **kw):
        self.surfaces.append(surface)
        return self.answers.pop(0) if self.answers else None

    def _queue(self, *answers) -> None:
        self.answers = list(answers)

    def _row_named(self, name: str) -> overlay.Row:
        return self._row(self._rows(), name)

    def _pick(self, **kw):
        return selector.pick(cwd=Path(config.ROOT), root=config.ROOT, **kw)


class ThePick(_APickedSelector, unittest.TestCase):
    def test_enter_on_a_runnable_row_is_the_choice(self):
        self._queue(self._row_named(WORK))
        self.assertEqual(self._pick(), selector.Choice(WORK))

    def test_escape_starts_nothing(self):
        self._queue()
        self.assertIsNone(self._pick())

    def test_enter_on_a_refused_row_keeps_the_selector_open_and_shows_why(self):
        self.which.side_effect = lambda cmd, **kw: None if cmd == "claude" else "/usr/bin/x"
        self._queue(self._row_named(WORK), None)
        self.assertIsNone(self._pick())
        self.assertEqual(len(self.surfaces), 2)
        self.assertIn("not on PATH", self.surfaces[1].footer)

    def test_a_refused_row_that_is_the_only_row_says_nothing_can_start(self):
        """The plan's `NOTHING_TO_PICK`, and it is shown from the first paint rather than
        only after an Enter: a list where nothing can run says so while it is being read."""
        self.which.side_effect = lambda cmd, **kw: None
        self._queue()
        self.assertIsNone(self._pick())
        self.assertIn("no profile can start here", self.surfaces[0].footer)

    def test_the_selector_shows_even_with_one_profile(self):
        """The spec's own rule: one profile costs one Enter, and nothing is picked for the
        operator (review 10)."""
        self.local.write_text("")
        self.which.side_effect = lambda cmd, **kw: "/usr/bin/x" if cmd == "claude" else None
        self._queue(self._row_named("claude"))
        self.assertEqual(self._pick(), selector.Choice("claude"))
        self.assertEqual(len(self.surfaces), 1)
        self.assertEqual(len(self.surfaces[0].catalogue), 1)

    def test_the_footer_does_not_promise_a_harness_to_go_back_to(self):
        """`F12` is the overlay's escape hatch back to the harness, and in this pane there
        is no harness yet to go back to."""
        self._queue()
        self._pick()
        line = self.surfaces[0].render(120, 8)[-1]
        self.assertNotIn(overlay.HATCH_KEY, line)
        self.assertIn("esc close this chat", line)

    def test_a_pick_refused_at_launch_comes_back_with_that_row_updated(self):
        """Ruling 30's other half, as `pick` sees it: the launch hands back what it refused
        and the row that was picked carries it."""
        self._queue(None)
        self._pick(after=selector.Refused(WORK, "profile 'claude-work' is refused — why"))
        row = self._row(self.surfaces[0].catalogue, WORK)
        self.assertTrue(row.refused)
        self.assertIn("is refused — why", row.note)
        self.assertIn("is refused — why", self.surfaces[0].footer)

    def test_the_cursor_comes_back_to_the_row_the_launch_refused(self):
        """"The cursor where it was": a list that re-aimed to the first runnable row would
        move the operator off the row whose reason they are being shown — and that row is
        refused, so `palette.aim` would never land on it."""
        self._queue(None)
        self._pick(after=selector.Refused(WORK, "why not"))
        self.assertEqual(self.surfaces[0].selected.id, selector.ROW_PREFIX + WORK)
        self.assertTrue(self.surfaces[0].selected.refused)


class NothingTheOperatorMustReadIsCutWithoutAWord(_APlaneWithProfiles, unittest.TestCase):
    """Ruling 45, on the two places this surface carries profile-derived text: its rows and
    its footer.

    An ellipsis marks a cut and not its size, and on both the size is the question: a
    refusal's reason appears in the footer and NOWHERE else on this surface, and a row
    holds the very command Enter is about to start. The approval prompt is not here because
    it is not on this surface: Task 3's prompt is printed whole in the pane
    (`ANewProfileIsAskedInThePane`).
    """

    #: A command far wider than any pane, and printable so nothing else clips it first.
    LONG = "x" * 400

    def test_a_footer_too_wide_for_the_pane_says_how_much_it_hid(self):
        line = selector.Selector(catalogue=self._rows(),
                                 footer="  " + self.LONG).render(70, 8)[-1]
        self.assertIn("not shown", line)
        self.assertLessEqual(len(line.replace("\x1b[2m", "").replace("\x1b[0m", "")), 70)

    def test_a_footer_that_fits_says_nothing_about_hiding(self):
        line = selector.Selector(catalogue=self._rows(), footer="  short").render(70, 8)[-1]
        self.assertNotIn("not shown", line)
        self.assertIn("short", line)

    def test_the_number_it_says_is_the_characters_it_took(self):
        """Characters and not cells, because that is what an operator would have to go and
        read somewhere else — `charter harness list` prints the same sentence unclipped."""
        text = "  " + self.LONG
        line = overlay._clipped(text, 70)
        kept = line.split("…")[0]
        self.assertIn(f"+{len(text) - len(kept)} not shown", line)

    def test_a_sentence_exactly_as_wide_as_the_pane_is_left_whole(self):
        """The boundary, not just the direction: a line that FITS is not a line that was
        cut, so it says nothing about hiding. One cell narrower is the other side of it."""
        text = "x" * 70
        self.assertEqual(overlay._clipped(text, 70), text)
        self.assertIn("not shown", overlay._clipped(text, 69))

    def test_a_line_that_fits_in_cells_is_whole_however_many_characters_it_holds(self):
        """The fit is measured in CELLS, and a combining mark is none: forty accented letters
        spelled as letter plus U+0301 are eighty characters and forty cells. Exactly as wide
        as the pane, the line is whole — no count, nothing cut — and one cell narrower it is
        cut. Asked of the walk below instead, a line with more characters than cells would be
        cut to half of itself with `… +40 not shown` when it fitted all along, which is the
        early return and its boundary both, found by CI's sweep on `3184066`."""
        text = "e\u0301" * 40
        self.assertEqual((len(text), tui.width(text)), (80, 40))
        self.assertEqual(overlay._clipped(text, 40), text)
        self.assertIn("not shown", overlay._clipped(text, 39))

    def test_the_cut_line_keeps_every_cell_the_pane_has(self):
        """The search walks down from the end and stops at the FIRST length that fits, so
        the answer is the longest line this pane can hold — one cell short would be one more
        character of the operator's own sentence hidden, and the count would say so."""
        line = overlay._clipped(self.LONG, 70)
        self.assertEqual(tui.width(line), 70)
        self.assertIn(f"+{len(self.LONG) - 54} not shown", line)

    def test_a_pane_too_narrow_for_the_count_still_draws_a_line_that_fits(self):
        """A pane can be narrower than the `· n to choose from` suffix alone. Whatever the
        heading's arithmetic answers, the line that reaches the terminal is the pane's width
        and no wider."""
        head = selector.Selector(catalogue=self._rows()).render(8, 8)[0]
        self.assertLessEqual(tui.width(head), 8)

    def test_a_list_still_says_how_many_rows_it_has(self):
        """The control: the count is right for a surface somebody is choosing FROM."""
        self.assertIn("to choose from",
                      selector.Selector(catalogue=self._rows()).render(120, 8)[0])

    def _row_line(self, note: str, width: int, *, counts: bool = True) -> str:
        """The first row's line, on the selector — or, *counts* false, on a plain surface."""
        row = overlay.Row(id=selector.ROW_PREFIX + WORK, title=WORK, note=note)
        surface = (selector.Selector(catalogue=(row,)) if counts
                   else overlay.Surface(rows=(row,)))
        return surface.render(width, 6)[1]

    def test_a_row_too_wide_for_the_pane_says_how_much_of_its_note_it_hid(self):
        """A command out of a file a chat can write, cut to the pane: the count says it was
        cut and by how much, so a reader can tell a clipped command from a whole one."""
        line = self._row_line(self.LONG, 80)
        kept = len(re.search(r"(x+)…", line).group(1))
        self.assertIn(f"… +{len(self.LONG) - kept} not shown", line)
        self.assertLessEqual(tui.width(line), 80)

    def test_a_row_that_fits_says_nothing_about_hiding(self):
        self.assertNotIn("not shown", self._row_line("claude · claude", 80))

    def test_the_count_is_of_the_whole_note_and_not_of_a_fixed_cut(self):
        """`contain.one_line`'s own budget ends in a fixed `...` at 160 characters, and a
        count taken after it would count what was left — a number that looks exact and is
        not. So a 400-character note in a pane wide enough for all of it is drawn whole."""
        line = self._row_line(self.LONG, 500)
        self.assertIn(self.LONG, line)
        self.assertNotIn("…", line)

    def test_a_surface_that_does_not_count_draws_its_rows_as_it_always_did(self):
        """The palette, the tab menu and the pickers carry charter's own words, and their
        rows are not this ruling's: a long note there keeps `contain`'s fixed budget and
        says nothing about hiding."""
        line = self._row_line(self.LONG, 500, counts=False)
        self.assertIn("…", line)
        self.assertNotIn("not shown", line)
        self.assertNotIn(self.LONG, line)

    def test_a_note_a_megabyte_long_costs_the_paint_what_the_pane_holds(self):
        """A chat can write a command of any length, and this row is painted on every
        keystroke. The cut searches down from what a pane this wide could show — never from
        the end of the note — so measuring it is bounded by the pane, not by the file."""
        real, measured = tui.width, []

        def width(text):
            measured.append(len(text))
            if len(measured) > 400:
                raise AssertionError("the cut walked the note rather than the pane")
            return real(text)

        with mock.patch.object(overlay.tui, "width", side_effect=width):
            line = self._row_line("y" * 1_000_000, 80)
        self.assertIn("not shown", line)

    def test_a_footer_reason_longer_than_contains_budget_is_counted_from_all_of_it(self):
        text = "  " + "z" * 300
        line = selector.Selector(catalogue=self._rows(), footer=text).render(250, 8)[-1]
        kept = len(re.search(r"(z+)…", line).group(1))
        self.assertIn(f"… +{300 - kept} not shown", line)

    def test_doctor_and_the_selector_say_it_with_one_helper(self):
        """Ruling 45 names both surfaces and one promise, so there is one spelling of it."""
        self.assertIs(doctor._counted, contain.counted)
        self.assertEqual(overlay._clipped("q" * 100, 40),
                         contain.counted("q" * 100, len(overlay._clipped("q" * 100, 40)
                                                         .split("…")[0])))


class EveryFooterSaysHowToLeave(_APickedSelector, unittest.TestCase):
    """F5 and F6. Esc is the whole of the way out of this pane — and in the one state where
    it is the ONLY thing that works, a footer that dropped the hint to make room for a
    sentence would be describing a dead end without naming the door."""

    def test_the_ordinary_footer_names_esc(self):
        self.assertIn(selector.ESC_HINT, selector.FOOTER)

    def test_a_footer_carrying_a_refusal_still_names_esc(self):
        self._queue(None)
        self._pick(after=selector.Refused(WORK, "why not"))
        self.assertIn(selector.ESC_HINT, self.surfaces[0].footer)
        self.assertIn("why not", self.surfaces[0].footer)

    def test_a_footer_saying_nothing_can_start_still_names_esc(self):
        self.which.side_effect = lambda cmd, **kw: None
        self._queue()
        self._pick()
        self.assertIn(selector.ESC_HINT, self.surfaces[0].footer)
        self.assertIn("no profile can start here", self.surfaces[0].footer)

    def test_the_esc_hint_survives_a_reason_too_wide_for_the_pane(self):
        """Which is why the hint goes FIRST when there is a reason: whatever is last is what
        a narrow pane takes, and this is the half that must not be taken."""
        line = selector.Selector(
            catalogue=self._rows(),
            footer=selector._footer((), selector.Refused(WORK, "y" * 400))).render(70, 8)[-1]
        self.assertIn(selector.ESC_HINT, line)
        self.assertIn("not shown", line)

    def test_the_footer_keeps_esc_down_to_the_narrowest_supported_pane(self):
        """How narrow a pane the selector supports, stated as a number and pinned: the whole
        hint `esc close this chat` survives at **40 columns** and wider, and the key `esc`
        at **24** and wider, for any reason shorter than 100,000 characters (a five-digit
        count). Below that the count and the hint compete for the same cells, and a count
        that cannot fit cannot be shown (`overlay._clipped`'s docstring) — measured: at 23
        a 50,000-character reason leaves `  es… +50024 not shown`."""
        foot = selector._footer((), selector.Refused(WORK, "y" * 99_000))
        for width in range(40, 121):
            with self.subTest(width=width):
                self.assertIn(selector.ESC_HINT, overlay._clipped(foot, width))
        for width in range(24, 40):
            with self.subTest(width=width):
                self.assertIn("  esc", overlay._clipped(foot, width))

    def test_a_list_with_something_to_start_says_only_the_keys(self):
        """The third state of this footer, and the one that says the other two are about
        something. `NOTHING_TO_PICK` belongs to a list where NOTHING can run: over a list
        holding one refused row among runnable ones — which is most lists — it is the
        summary contradicting the rows, and it would be there from the first paint.
        """
        self.which.side_effect = lambda cmd, **kw: (None if cmd == "npx"
                                                    else f"/usr/bin/{cmd}")
        rows = self._rows()
        self.assertTrue(any(r.refused for r in rows), self._names(rows))
        self.assertTrue(any(not r.refused for r in rows), self._names(rows))
        self.assertEqual(selector._footer(rows, None), selector.FOOTER)
        self.assertNotIn("no profile can start here", selector.FOOTER)

    def test_a_refused_row_in_an_all_refused_list_shows_its_own_reason(self):
        """F6, and it is ruling 30 read exactly: Enter on a refused row shows THAT row's
        reason. Checked the other way round, the one state where the operator most needs the
        row's own sentence is the one that answers with the summary."""
        self.which.side_effect = lambda cmd, **kw: None
        rows = self._rows()
        self.assertTrue(all(r.refused for r in rows), self._names(rows))
        foot = selector._footer(rows, selector.Refused(WORK, "not on PATH: claude"))
        self.assertIn("not on PATH: claude", foot)
        self.assertNotIn("no profile can start here", foot)


class TheSweepsOwnFindings(_APlaneWithProfiles, unittest.TestCase):
    """The lines CI's deletion sweep reported as survivors on this branch's first run, each
    with the case that goes red when it changes.

    Nine of them, and they are here together rather than scattered because they are one
    finding about one change: a surface, its argv, its two orderings and the three state
    writers underneath it were all pinned by what they DID and not by what they SAID.
    """

    def test_the_launcher_refuses_a_launch_that_names_nothing_in_its_own_words(self):
        """`launcher.NOTHING_NAMED`. The sentence is the whole of what this path produces —
        it is reachable only by hand, so a number would tell nobody anything."""
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.cmd_frame_launch(
                SimpleNamespace(select=False, start="", profile="", attended=False, rest=[]))
        self.assertEqual(rc, 2)
        self.assertIn("--select", said[0])
        self.assertIn("charter <profile>", said[0])

    def test_the_start_row_rides_the_launchers_argv_only_when_there_is_one(self):
        """`argv_select`'s conditional. Always-`()` would make every selector open on
        `palette.aim`'s row whatever the press expressed, which is a chat opening on
        somebody else's account with nothing said."""
        self.assertEqual(launcher.argv_select("claude-work")[-4:],
                         ["--select", "--attended", "--start", "claude-work"])
        self.assertEqual(launcher.argv_select(None)[-2:], ["--select", "--attended"])
        self.assertNotIn("--start", launcher.argv_select(None))

    def test_the_surface_says_what_it_is_for(self):
        """`selector.LABEL` reaches the heading: the pane is a question, and a heading that
        did not say which question is a modal surface with no subject."""
        surface = selector.Selector(catalogue=self._rows())
        self.assertEqual(surface.heading, selector.LABEL)
        self.assertIn("which profile", surface.render(120, 8)[0])

    def test_the_rows_are_built_ins_in_registry_order_then_declared_by_name(self):
        """`place`'s index. Collapsed to a constant, every built-in sorts equal and the list
        comes back alphabetical — so the row an operator reaches for moves the day charter
        registers a harness, and `charter harness list` and this surface stop agreeing."""
        order = list(profiles.builtins())
        names = self._names(self._rows())
        self.assertEqual(names[:len(order)], order)
        self.assertEqual(names[len(order):], sorted(names[len(order):]))

    def test_the_waiting_marker_never_raises_over_a_chat_id_that_is_no_directory(self):
        """All three writers, and the same shape `state.record_closed` has: `frame_dir`
        REFUSES an id it cannot make a directory of rather than raising, so each of these
        has to answer for that — two of them on a path a panel and a quit run."""
        self.assertIsNone(state.record_waiting(""))
        self.assertFalse(state.is_waiting(""))
        self.assertIsNone(state.clear_waiting(""))

    def test_the_marker_is_a_file_called_waiting_in_the_chats_own_directory(self):
        """The name is on disk, which makes it an interface rather than a private spelling:
        a frame started by one charter is read by whichever charter the operator upgrades
        to, and `state.reap`, a quit and a reopen all meet this chat through its directory.
        `closed` is pinned the same way, one marker over."""
        state.record_waiting("beta.7")
        self.assertTrue((state.frame_dir("beta.7") / "waiting").exists())
        state.clear_waiting("beta.7")
        self.assertFalse((state.frame_dir("beta.7") / "waiting").exists())

    def test_marking_a_waiting_pane_survives_a_filesystem_that_refuses(self):
        """`record_waiting`'s catch. It runs in `_launch`, before tmux is asked for
        anything, so an `OSError` out of it would refuse to open the chat over a marker
        whose failure costs exactly one uninvited tab — the trade its docstring states."""
        with mock.patch.object(state.config, "write_for", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.record_waiting("beta.8"))
        self.assertFalse(state.is_waiting("beta.8"))

    def test_a_row_id_is_not_something_the_query_can_match(self):
        """What the `profile:` in a row id is FOR, said where it can be seen. `palette
        .matches` reads an id only when `component.usable_id` accepts it, and a colon is
        what that refuses — so ids stay out of the filter and typing the prefix's own word
        lists nothing. Ids spelled as bare names would be matched, and every row would
        answer to the letters of a name the operator was not typing.
        """
        rows = self._rows()
        self.assertTrue(all(r.id.startswith(selector.ROW_PREFIX) for r in rows))
        self.assertEqual(palette.narrow(rows, "profile"), ())
        self.assertTrue(palette.narrow(rows, WORK))

    def test_forgetting_a_waiting_pane_survives_a_filesystem_that_refuses(self):
        """`clear_waiting`'s catch. It runs at the pick, in the pane, one line before the
        `exec` — so an `OSError` raised out of it would take down the launch that was about
        to hand the pane to the harness, over a marker whose whole job is already done."""
        state.record_waiting("beta.9")
        with mock.patch.object(Path, "unlink", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.clear_waiting("beta.9"))
        self.assertTrue(state.is_waiting("beta.9"))


class _ASelectorPane(_APlaneWithProfiles):
    """`charter frame-launch --select` in-process: the frame proof, the claim, the exec and
    `selector.pick` stood in, and wiring stated for the launch (`wired_as_today`) — without
    it every pick here would be refused "could not tell" before it reached the exec."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_ROOT": str(config.ROOT), "HOME": str(self.home),
                         "PATH": os.environ.get("PATH", ""),
                         "GIT_CEILING_DIRECTORIES": os.environ.get(
                             "GIT_CEILING_DIRECTORIES", ""),
                         **_gitguard.environment()}, clear=True))
        self.log: list[str] = []
        self.enterContext(mock.patch.object(
            launcher, "framed_chat", side_effect=lambda: self.log.append("proof") or "beta.1"))
        self.enterContext(mock.patch.object(
            launcher.pane, "claim", side_effect=lambda: self.log.append("claim")))
        self.enterContext(mock.patch.object(launcher.pane, "release"))
        self.enterContext(mock.patch.object(launcher, "_wait_for_the_operator"))
        # No case in this class may reach a real tmux server: a cancel asks tmux which pane
        # this process is, and the default server is charter's own — the operator's (see
        # `test_a_pane_this_process_is_not_the_first_process_of_is_never_closed`).
        self.enterContext(mock.patch.object(launcher.tmuxctl, "live_pane_by_pid",
                                            return_value=None))
        self.execs: list[tuple] = []
        self.exec = self.enterContext(mock.patch.object(
            launcher.os, "execvpe", side_effect=lambda *a: self.execs.append(a)))
        wired_as_today(self)
        self.picks: list = []
        self.asked: list[dict] = []
        self.enterContext(mock.patch.object(selector, "pick", side_effect=self._pick))
        state.record_waiting("beta.1")

    @property
    def afters(self) -> list:
        return [kw.get("after") for kw in self.asked]

    def _pick(self, **kw):
        self.log.append("draw")
        self.asked.append(kw)
        return self.picks.pop(0) if self.picks else None

    def _run(self, *picks, start: str = "") -> int:
        self.picks = list(picks)
        return launcher.cmd_frame_launch(
            SimpleNamespace(select=True, start=start, profile="", attended=True, rest=[]))


class ThePaneWaitsThenBecomesTheHarness(_ASelectorPane, unittest.TestCase):
    """`charter frame-launch --select` — what a new chat's window runs before any harness."""

    def test_a_pick_records_kind_and_profile_then_execs(self):
        self.assertEqual(self._run(selector.Choice(WORK)), 0)
        self.assertEqual(state.identity("beta.1")["CHARTER_HARNESS"], "claude-code")
        self.assertEqual(state.profile("beta.1"), WORK)
        self.assertFalse(state.is_waiting("beta.1"))
        self.assertEqual(self.execs[0][0], "claude")

    def test_escape_closes_this_chats_own_window_rather_than_waiting_for_a_hook(self):
        """**Charter knows the operator cancelled**, so it does not ask tmux to infer it.

        Measured on a Linux CI runner: a chat pane that exits at the selector comes back
        dead with BOTH `#{pane_dead_status}` and `#{pane_dead_signal}` empty, and the window
        is still listed a minute later — with `remain-on-exit on` and both hooks present.
        The `pane-died` hook is untouched and is still the answer for a harness that dies;
        Esc is charter's own keystroke on charter's own surface in a pane no harness has
        ever run in, and it closes its own window — the one tmux says THIS process is the
        first process of, and no other.
        """
        state.record_server("beta.1", "some-socket")
        own = launcher.tmuxctl.LivePane("%7", "beta", "beta.1", "")
        killed: list = []
        with mock.patch.object(launcher.tmuxctl, "live_pane_by_pid",
                               return_value=own) as asked, \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        asked.assert_called_once_with("some-socket", os.getpid())
        self.assertEqual(len(killed), 1, killed)
        self.assertEqual(killed[0][-3:], ["kill-window", "-t", "%7"])
        self.assertIn("some-socket", killed[0], "the chat's own recorded server")

    def test_a_pick_closes_no_window(self):
        """The control. Only a cancel closes anything — a pick hands the pane to the
        harness, which is the one thing that must still be running in it."""
        killed: list = []
        with mock.patch.object(launcher.tmuxctl, "run",
                               side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(selector.Choice(WORK)), 0)
        self.assertEqual(killed, [])

    def test_a_cancel_with_no_frame_to_close_closes_nothing(self):
        """A `charter frame-launch --select` run by hand proves no chat (rulings 29 and 33),
        so there is no window of charter's to take — and taking one by `$TMUX_PANE` alone
        would be this launch closing a pane it could not prove was its own.

        `$TMUX_PANE` is SET here, which is what makes that sentence a measurement: a hand
        run inside somebody's own tmux has one, and it names a pane of theirs.

        **tmux PROVES a pane here**, which is what makes this case about the missing frame
        and nothing else: a pane whose `#{pane_pid}` is this process would be closed by
        every other line of `_close_the_cancelled_chat`, so only the missing chat stops it.
        """
        killed: list = []
        own = launcher.tmuxctl.LivePane("%9", "beta", "beta.1", "")
        with mock.patch.object(launcher, "framed_chat", return_value=None), \
                mock.patch.dict(os.environ, {"TMUX_PANE": "%9"}), \
                mock.patch.object(launcher.tmuxctl, "live_pane_by_pid", return_value=own), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(killed, [])

    def test_a_launch_with_no_frame_proof_still_runs_what_was_picked(self):
        """The pick's own writes, on the path where there is nothing to write them to.

        `framed_chat` answering `None` is a `frame-launch` run by hand, and every one of the
        three records the pick makes is then addressed to a chat that has no directory.
        They answer for that rather than raising — which is what keeps a hand run from
        dying one line above the `exec` it exists to reach.
        """
        with mock.patch.object(launcher, "framed_chat", return_value=None):
            self.assertEqual(self._run(selector.Choice(WORK)), 0)
        self.assertEqual(self.execs[0][0], "claude")
        self.assertIsNone(state.profile("beta.1"))
        self.assertTrue(state.is_waiting("beta.1"), "beta.1 was never this launch's chat")

    def test_a_pane_this_process_is_not_the_first_process_of_is_never_closed(self):
        """**The incident this rule exists for (2026-09-12).** A test run of this class from
        inside a live chat — with the empty-target guard deleted — sent `kill-window` to
        charter's own server and closed the operator's session. A recorded pane and an
        inherited `$TMUX_PANE` both name SOME pane; neither proves it is this process's, and
        a chat's own shell inherits `$TMUX_PANE` from the harness pane it runs in. Only tmux
        answering `#{pane_pid} == os.getpid()` proves that, and without it nothing closes."""
        state.record_harness_pane("beta.1", "%7")
        killed: list = []
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%3195"}), \
                mock.patch.object(launcher.tmuxctl, "live_pane_by_pid", return_value=None), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(killed, [])

    def test_the_inherited_tmux_pane_is_never_the_target(self):
        """Proof found, and the target is the pane tmux proved — not the record, not the
        variable, even when both are present and disagree with it."""
        state.record_harness_pane("beta.1", "%7")
        own = launcher.tmuxctl.LivePane("%9", "beta", "beta.1", "")
        killed: list = []
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%3"}), \
                mock.patch.object(launcher.tmuxctl, "live_pane_by_pid", return_value=own), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self._run()
        self.assertEqual([argv[-3:] for argv in killed], [["kill-window", "-t", "%9"]])

    def test_no_kill_is_ever_sent_with_an_empty_target(self):
        """**Measured on tmux 3.7c, isolated socket, `$TMUX`/`$TMUX_PANE` unset:**
        `kill-window -t ''` exits 0 and kills that server's ACTIVE window. An empty target is
        not a no-op tmux refuses; it is the operator's current window."""
        own = launcher.tmuxctl.LivePane("", "beta", "beta.1", "")
        killed: list = []
        with mock.patch.object(launcher.tmuxctl, "live_pane_by_pid", return_value=own), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(killed, [])

    def test_escape_exits_130_and_leaves_it_waiting(self):
        self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(selector.CANCELLED_EXIT, 130)
        self.assertTrue(state.is_waiting("beta.1"))
        self.assertEqual(self.execs, [])
        self.assertEqual(state.profile("beta.1"), None)

    def test_a_pick_refused_at_launch_returns_to_the_selector(self):
        """Ruling 30, and the fresh check is why it can happen at all: the row was drawn
        from what was true a moment ago and the launch asks again."""
        (config.ROOT / ".gitignore").write_text("")
        self.assertEqual(self._run(selector.Choice(WORK)), selector.CANCELLED_EXIT)
        self.assertEqual(self.execs, [])
        self.assertEqual(len(self.afters), 2)
        self.assertEqual(self.afters[1].profile, WORK)
        self.assertIn("charter reinit", self.afters[1].why)
        self.assertTrue(state.is_waiting("beta.1"))

    def test_only_escape_closes_the_selector_window(self):
        (config.ROOT / ".gitignore").write_text("")
        self.assertEqual(self._run(selector.Choice(WORK), selector.Choice(WORK)),
                         selector.CANCELLED_EXIT)
        self.assertEqual(len(self.afters), 3)

    def test_a_name_the_plane_no_longer_declares_returns_to_the_selector(self):
        self.assertEqual(self._run(selector.Choice("gone")), selector.CANCELLED_EXIT)
        self.assertIn("no profile named", self.afters[1].why)

    def test_an_exec_that_fails_after_the_pick_reverts_and_exits(self):
        """N7's nit: a pane running nothing must not also claim to be a chat."""
        self.exec.side_effect = OSError(5, "Input/output error")
        self.assertEqual(self._run(selector.Choice(WORK)), launcher.REFUSED_EXIT)
        self.assertTrue(state.is_waiting("beta.1"))
        self.assertIsNone(state.profile("beta.1"))
        self.assertEqual(state.identity("beta.1").get("CHARTER_HARNESS"), "")

    def test_the_frame_proof_runs_before_the_pane_is_claimed_or_drawn(self):
        """Ruling 35: the proof holds only while the pane has printed nothing."""
        self._run()
        self.assertEqual(self.log[:3], ["proof", "claim", "draw"])

    def test_the_start_row_reaches_the_selector(self):
        self._run(start=WORK)
        self.assertEqual(self.asked[0]["start"], WORK)
        self.assertEqual(self.asked[0]["root"], Path(config.ROOT))

    def test_no_start_is_no_row_rather_than_an_empty_name(self):
        self._run()
        self.assertIsNone(self.asked[0]["start"])

    def test_a_refused_pick_is_the_next_selectors_start_row(self):
        (config.ROOT / ".gitignore").write_text("")
        self._run(selector.Choice(WORK))
        self.assertEqual(self.asked[1]["start"], WORK)

    def test_a_launch_with_neither_a_profile_nor_select_is_refused(self):
        said: list[str] = []
        with mock.patch.object(launcher.util, "err", side_effect=said.append):
            rc = launcher.cmd_frame_launch(
                SimpleNamespace(select=False, start="", profile="", attended=False, rest=[]))
        self.assertEqual(rc, 2)
        self.assertEqual(len(said), 1, said)


class ANewProfileIsAskedInThePane(_ASelectorPane, unittest.TestCase):
    """Task 3's question, put by the launch in the pane the selector was drawn in.

    Enter on a new or changed profile is a pick like any other; `launcher.attempt` asks, on
    the terminal the surface has just handed back, with `profiletrust.ask_in_terminal`'s own
    prompt — the whole command and environment, never clipped (ruling 45) — and every
    answer is a kind `_select_in_pane` already handles. A terminal on both ends, as a real
    pane has: `Typed` for the keyboard, `ATerminal` for the screen.
    """

    #: A command wider than any pane, printable so nothing else escapes it.
    LONG = "--" + "x" * 400

    def setUp(self) -> None:
        super().setUp()
        self.local.write_text(self.local.read_text().replace(
            'command = ["claude"]', f'command = ["claude", "{self.LONG}"]'))
        self._unapprove()
        self.screen = ATerminal()
        self.enterContext(mock.patch("sys.stdout", self.screen))
        self.said: list[str] = []
        self.enterContext(mock.patch.object(launcher.util, "err", side_effect=self.said.append))

    def _typing(self, *lines: str) -> Typed:
        typed = Typed(*lines)
        self.enterContext(mock.patch("sys.stdin", typed))
        return typed

    def _approved(self) -> bool:
        return not profiletrust.approval_needed(profiles.current().profiles[WORK])

    def test_yes_shows_the_whole_command_records_it_and_starts_it(self):
        typed = self._typing("y\n")
        self.assertEqual(self._run(selector.Choice(WORK)), 0)
        shown = self.screen.getvalue()
        self.assertIn(profiletrust.QUESTION.strip(), shown)
        self.assertIn(self.LONG, shown, "the approval prompt never clips")
        self.assertIn("CLAUDE_CONFIG_DIR=~/.cw", shown)
        self.assertNotIn("not shown", shown)
        self.assertEqual(typed.reads, 1)
        self.assertTrue(self._approved())
        self.assertEqual(self.execs[0][1], ["claude", self.LONG])
        self.assertFalse(state.is_waiting("beta.1"))

    def test_no_goes_back_to_the_list_with_nothing_refused_and_the_cursor_on_that_row(self):
        """A decline is the operator's own answer, already answered with `charter: nothing
        started.` where they gave it — so the row is not marked refused (Enter on it must
        ask again, not recite a reason) and nothing is said a second time."""
        self._typing("n\n")
        self.assertEqual(self._run(selector.Choice(WORK)), selector.CANCELLED_EXIT)
        self.assertEqual(self.execs, [])
        self.assertIsNone(self.afters[1])
        self.assertEqual(self.asked[1]["start"], WORK)
        self.assertFalse(self._approved())
        self.assertIn(profiletrust.NOTHING_STARTED, self.screen.getvalue())
        self.assertEqual(self.said, [])
        self.assertTrue(state.is_waiting("beta.1"))

    def test_a_yes_charter_could_not_write_down_refuses_on_that_row_and_never_asks_again(self):
        """Task 3's N2b, on the selector: re-running the chain after an unrecordable yes
        would find no record and put the identical question again. `KIND_RECORD` comes back
        as that row's refusal, the command never runs, and Enter on the row only says why."""
        typed = self._typing("y\n")
        with mock.patch.object(profiletrust.config, "replace_for",
                               side_effect=OSError(28, "No space left on device")):
            self.assertEqual(self._run(selector.Choice(WORK)), selector.CANCELLED_EXIT)
        self.assertEqual(typed.reads, 1, "the same question was asked twice")
        self.assertEqual(self.execs, [])
        self.assertEqual(self.afters[1].profile, WORK)
        self.assertIn("could not record that", self.afters[1].why)
        self.assertIn("No space left on device", self.afters[1].why)
        # And the list it comes back to holds that row REFUSED, so Enter on it recites the
        # reason instead of starting the same launch and asking the same question.
        row = self._row(self._rows(after=self.afters[1]), WORK)
        self.assertTrue(row.refused)
        self.assertIn("No space left on device", row.note)

    def test_a_yes_runs_the_whole_chain_again_and_the_wiring_probe_comes_after_it(self):
        """Ruling 27, wiring last, and ruling 1 underneath it: the profile's own command is
        not run to probe it until the operator has said yes to that command."""
        order: list[str] = []

        class _Answers(Typed):
            def readline(self) -> str:
                order.append("answered")
                return super().readline()

        typed = _Answers("y\n")
        self.enterContext(mock.patch("sys.stdin", typed))
        with mock.patch.object(wiring, "refusal",
                               side_effect=lambda p, *, cwd: order.append("probed")
                               or "profile 'claude-work' is not wired — the reason"):
            self.assertEqual(self._run(selector.Choice(WORK)), selector.CANCELLED_EXIT)
        self.assertEqual(order, ["answered", "probed"])
        self.assertEqual(self.execs, [])
        self.assertIn("is not wired", self.afters[1].why)

    def test_a_record_that_moved_while_the_question_was_up_is_refused_not_asked_again(self):
        real = profiletrust.record_launched

        def _records_it_then_loses_it(p):
            why = real(p)
            (config.STATE_DIR / profiletrust.RECORD).write_text("{}")
            return why

        typed = self._typing("y\n")
        with mock.patch.object(profiletrust, "record_launched",
                               side_effect=_records_it_then_loses_it):
            self._run(selector.Choice(WORK))
        self.assertEqual(typed.reads, 1, "the same question was asked twice")
        self.assertEqual(self.execs, [])
        self.assertIn("moved while that question was on screen", self.afters[1].why)

    def test_a_pane_with_no_terminal_to_ask_in_comes_back_with_that_reason(self):
        self.enterContext(mock.patch("sys.stdin", APipe()))
        self._run(selector.Choice(WORK))
        self.assertEqual(self.execs, [])
        self.assertIn("no terminal here to ask in", self.afters[1].why)


class AWaitingPaneIsNotAChat(_APlaneWithProfiles, unittest.TestCase):
    """It has a tab, so it can be left and come back to; it is not in the quit manifest and
    is never reopened, because nothing has been started in it to bring back."""

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir("beta.2", create=True)
        state.record_workspace("beta.2", "beta")
        state.record_waiting("beta.2")

    def test_it_has_a_tab(self):
        self.assertIn("beta.2", chats._by_workspace().get("beta", []))

    def test_a_quit_does_not_record_it(self):
        self.assertNotIn("beta.2", [d.chat for d in leave.plan(live=None, focus="").chats])

    def test_a_chat_that_picked_is_recorded_again(self):
        state.clear_waiting("beta.2")
        self.assertIn("beta.2", [d.chat for d in leave.plan(live=None, focus="").chats])

    def test_waiting_is_forgotten_at_the_pick_and_remembered_by_the_undo(self):
        self.assertTrue(state.is_waiting("beta.2"))
        state.clear_waiting("beta.2")
        self.assertFalse(state.is_waiting("beta.2"))
        state.record_waiting("beta.2")
        self.assertTrue(state.is_waiting("beta.2"))

    def test_the_undo_puts_back_the_kind_the_launch_had_recorded(self):
        """`_picked` reads the identity record's own `CHARTER_HARNESS` before it overwrites
        it, so an `execvpe` that raises leaves the chat saying exactly what it said before
        the pick. Reading any other name would put back a blank — which is what a pane that
        is still at the selector says, and this one stopped being that at the pick.
        """
        state.record_identity("beta.2", {"CHARTER_SESSION_ID": "beta.2",
                                         "CHARTER_HARNESS": "claude-code"})
        p = self._read().profiles["codex-pinned"]
        undo = launcher._picked("beta.2", p)
        self.assertEqual(state.identity("beta.2")["CHARTER_HARNESS"], p.harness)
        self.assertNotEqual(p.harness, "claude-code")
        undo()
        self.assertEqual(state.identity("beta.2")["CHARTER_HARNESS"], "claude-code")
        self.assertTrue(state.is_waiting("beta.2"))

    def test_the_picked_kind_is_written_into_the_chats_identity(self):
        """`chats.harness_of`, `leave.plan` and the panels all read the kind off `identity`,
        so the pick writes it there rather than adding a second place to look."""
        state.record_identity("beta.2", {"CHARTER_SESSION_ID": "beta.2",
                                         "CHARTER_HARNESS": ""})
        state.record_picked_kind("beta.2", "codex")
        self.assertEqual(chats.harness_of("beta.2"), "codex")
        self.assertEqual(state.identity("beta.2")["CHARTER_SESSION_ID"], "beta.2")


class TheSurfaceDrawsTheFooterItWasGiven(unittest.TestCase):
    """`overlay.Surface.footer`, which is what lets the selector say a refusal where the
    operator is looking without borrowing the palette's own bottom line."""

    def test_a_surface_with_no_footer_says_what_it_always_said(self):
        line = overlay.Surface(rows=(overlay.Row("a", "a"),)).render(120, 6)[-1]
        self.assertIn("enter choose", line)
        self.assertIn(overlay.HATCH_KEY, line)

    def test_a_footer_replaces_that_line_whole(self):
        line = overlay.Surface(rows=(overlay.Row("a", "a"),),
                               footer="  mine").render(120, 6)[-1]
        self.assertIn("mine", line)
        self.assertNotIn("enter choose", line)

    def test_a_footer_is_contained_and_clipped_to_the_width(self):
        """An SGR escape as well as the two control characters, because a `\\r` alone cannot
        tell the containment apart from what happens without it: `tui.sanitize` drops a
        carriage return whether the footer was contained or not, and deliberately passes
        charter's own colour markup through untouched. SGR is therefore the one thing in
        this line that reaches the terminal if nothing contained it — and a footer carries
        a refusal, which quotes a profile's own command out of a file a chat can write.
        """
        line = overlay.Surface(rows=(overlay.Row("a", "a"),),
                               footer="a\rb\nc\x1b[31mred").render(120, 6)[-1]
        self.assertNotIn("\r", line)
        self.assertNotIn("\n", line)
        self.assertNotIn("\x1b[31m", line)
        self.assertIn(contain.one_line("\x1b[31m"), line)



def _completed(argv, rc=0, out="", err=""):
    return subprocess.CompletedProcess(argv, rc, stdout=out, stderr=err)


class _AServerWithOneWorkspaceRunning:
    """The four answers a selector launch reads off tmux, and a log of everything it asked.

    Deliberately small: what every case in `WhereItAppears` is about is which question was
    asked and what charter did with the answer, not what a real server would do next.
    """

    def __init__(self, *, clients=(), sessions=("beta",), chats=("beta.1",), dead=None):
        #: What `#{pane_dead}:#{pane_dead_status}` answers, or ``None`` for a pane that is
        #: still running. The one tmux question whose answer decides what a launch SAYS
        #: on its way out, which is why it is a knob rather than a constant.
        self.dead = dead
        self.clients = list(clients)
        self.sessions = list(sessions)
        self.chats = list(chats)
        self.calls: list[list[str]] = []
        #: Run inside the `new-window` answer, where `state.record_waiting` has happened
        #: and tmux has not: the one moment "before tmux" can be read.
        self.at_new_window = lambda: None

    @staticmethod
    def _lines(rows) -> str:
        # One per line and NOTHING at all for none — a bare newline reads as one client
        # with a blank name, which is the opposite answer.
        return "".join(f"{r}\n" for r in rows)

    def __call__(self, cmd, **kw):
        self.calls.append(list(cmd))
        if "list-panes" in cmd:
            return _completed(cmd, 0, f"$3\t%0\t{config.STATE_DIR}\n")
        if "list-clients" in cmd:
            return _completed(cmd, 0, self._lines(self.clients))
        if "list-sessions" in cmd:
            return _completed(cmd, 0, self._lines(self.sessions))
        if "list-windows" in cmd:
            return _completed(cmd, 0, self._lines(self.chats))
        if "new-window" in cmd or "new-session" in cmd:
            self.at_new_window()
            # And the server now HAS what was just created. `_launch` reaps again on its
            # way out, against this same listing — so a fake that kept answering "nothing
            # is running" would have charter collect the chat directory it had just
            # written, and every case reading that chat's state would read an empty one.
            self.sessions.append("beta")
            self.chats.append("beta.1")
            return _completed(cmd, 0, "%9\n")
        if "display-message" in cmd:
            if "pane_dead" in cmd[-1] and self.dead is not None:
                return _completed(cmd, 0, self.dead)
            return _completed(cmd, 0, "132:43")
        return _completed(cmd, 0)

    def asked(self, verb: str) -> int:
        return sum(1 for c in self.calls if verb in c)

    def argv_of(self, verb: str) -> list[str]:
        return next(c for c in self.calls if verb in c)


class WhereItAppears(_APlaneWithProfiles, unittest.TestCase):
    """Which opens reach the selector, and which name their profile and skip it.

    Bare `charter`, `+`, a workspace tab and the palette's new chat all open at it. Every
    open nobody is at — reopen, a restored plane, a handoff — names its profile, because a
    selector is a question and there is nobody there to answer one.
    """

    #: The id a launch allocates here. `state.reap` takes the cold `beta.1` this fixture
    #: leaves behind before `new_chat_id` walks upward from 1, so the new chat gets that
    #: ordinal back — spelled out rather than inferred, because a case reading the wrong
    #: chat's state is a case that passes for the wrong reason.
    NEW = "beta.1"

    def setUp(self) -> None:
        super().setUp()
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        state.frame_dir("beta.1", create=True)
        state.record_workspace("beta.1", "beta")
        state.record_server("beta.1", commands_frame.SOCKET)
        state.record_harness_pane("beta.1", "%0")
        self.enterContext(mock.patch.object(commands_frame, "_spawn_gather"))
        self.enterContext(mock.patch.object(commands_frame, "_drawable_slots",
                                            return_value=[]))
        self.focused: list = []
        self.enterContext(mock.patch.object(
            commands_frame, "_focus_workspace",
            side_effect=lambda *a, **kw: self.focused.append((a, kw)) or 0))

    def _launch(self, fake, **ns) -> int:
        args = SimpleNamespace(**{"harness": "", "rest": [], "no_frame": False,
                                 "workspace": "beta", "pick": False, "select": True,
                                 "start": "", "profile": None, **ns})
        stripped = {k: v for k, v in os.environ.items()
                    if k not in ("TMUX", "TMUX_PANE")}
        with mock.patch.dict(os.environ, stripped, clear=True), \
                mock.patch("charter.commands_frame.subprocess.run", side_effect=fake), \
                mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False):
            return commands_frame._launch(args)

    def test_bare_charter_on_a_workspace_nobody_is_attached_to_attaches(self):
        """Ruling 17. `_workspace_to_focus` answers `None` for a live workspace with no
        client on it — rightly, for a launch that NAMES something: with nobody to drag, it
        should add its chat and run it. A bare `charter` names nothing, so opening a second
        selector beside chats that are already running would be charter adding a chat
        nobody asked for. `+` is how you add one."""
        fake = _AServerWithOneWorkspaceRunning(clients=[])
        self.assertEqual(self._launch(fake), 0)
        self.assertEqual(len(self.focused), 1)
        self.assertEqual(fake.asked("new-window"), 0)
        self.assertEqual(fake.asked("new-session"), 0)

    def test_a_named_profile_on_that_workspace_still_opens_a_chat(self):
        """The other half of ruling 17, and the control for the case above: `charter
        <profile>` keeps today's open-a-chat-where-nobody-is behaviour."""
        fake = _AServerWithOneWorkspaceRunning(clients=[])
        self._launch(fake, harness="claude", select=False, profile="claude")
        self.assertEqual(self.focused, [])
        self.assertEqual(fake.asked("new-window"), 1)

    def test_a_workspace_somebody_is_on_is_attached_to_either_way(self):
        fake = _AServerWithOneWorkspaceRunning(clients=["/dev/ttys001"])
        self._launch(fake)
        self.assertEqual(len(self.focused), 1)

    def test_a_workspace_with_nothing_running_opens_the_selector(self):
        """The negative control: with no live session there is nothing to attach to, so
        the launch opens a chat — at the selector."""
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        self._launch(fake)
        self.assertEqual(self.focused, [])
        self.assertEqual(fake.asked("new-session"), 1)

    def test_the_selector_launch_records_the_chat_as_waiting_before_tmux(self):
        """Before, and not after: a quit landing between the window and the pick must not
        record a chat that is a question on a screen."""
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        seen: list[bool] = []
        fake.at_new_window = lambda: seen.append(state.is_waiting(self.NEW))
        self._launch(fake)
        self.assertEqual(seen, [True])

    def test_a_launch_that_names_its_profile_records_no_waiting_pane(self):
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        seen: list[bool] = []
        fake.at_new_window = lambda: seen.append(state.is_waiting(self.NEW))
        self._launch(fake, harness="claude", select=False, profile="claude")
        self.assertEqual(seen, [False])

    def test_the_window_starts_on_the_launcher_with_no_inherited_kind(self):
        """Review 12. `_frame_env` keeps `$CHARTER_HARNESS` from this process where no
        harness was resolved, and the process that presses `+` is a panel of a chat that
        HAS one — so without the clear, the pressing chat's kind rides onto the new
        window's `-e` and into its identity record, and `chats.harness_of` then answers for
        a chat that has picked nothing."""
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}):
            self._launch(fake, start="claude-work")
        argv = fake.argv_of("new-session")
        self.assertEqual(argv[-4:], ["--select", "--attended", "--start", "claude-work"])
        self.assertIn("CHARTER_HARNESS=", argv)
        self.assertEqual(state.identity(self.NEW).get("CHARTER_HARNESS"), "")

    def test_no_start_puts_no_name_on_the_launchers_argv(self):
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        self._launch(fake)
        argv = fake.argv_of("new-session")
        self.assertEqual(argv[-2:], ["--select", "--attended"])
        self.assertNotIn("--start", argv)

    def test_the_planes_default_is_the_row_a_bare_launch_opens_on(self):
        self.local.write_text(self.local.read_text()
                              + '\n[harness]\ndefault = "claude-work"\n')
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[])
        self._launch(fake)
        self.assertEqual(fake.argv_of("new-session")[-2:], ["--start", "claude-work"])

    def test_select_with_no_frame_is_refused_rather_than_execd(self):
        """`--no-frame` reaches `bypass`, which `os.execvp`s the launcher — a whole modal
        surface over the operator's own terminal and then a harness with no frame around
        it. Two launches asked for at once."""
        said: list[str] = []
        fake = _AServerWithOneWorkspaceRunning()
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(fake, no_frame=True)
        self.assertEqual(rc, 2)
        self.assertEqual(fake.calls, [])
        self.assertEqual(len(said), 1, said)
        self.assertIn("--no-frame", said[0])

    def test_select_with_a_command_is_refused_rather_than_swallowing_it(self):
        """`charter frame -- <cmd>` is the escape hatch for a command charter has never
        met. A selector cannot run it and cannot ask about it, so asking for both is
        refused — a launcher that swallowed a command would be wrong in the one direction
        this module refuses everywhere else, silently."""
        said: list[str] = []
        fake = _AServerWithOneWorkspaceRunning()
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            rc = self._launch(fake, rest=["--", "htop"])
        self.assertEqual(rc, 2)
        self.assertEqual(fake.calls, [])
        self.assertEqual(len(said), 1, said)
        self.assertIn("htop", said[0])

    def test_the_command_it_refuses_is_repeated_back_escaped(self):
        """Ruling 35's reason through another door: this is the operator's own word, and
        it goes to a terminal."""
        said: list[str] = []
        fake = _AServerWithOneWorkspaceRunning()
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append):
            self._launch(fake, rest=["--", "ht\rop\x1b[2K"])
        self.assertNotIn("\r", said[0])
        self.assertNotIn("\x1b", said[0])
        self.assertIn("\\u000d", said[0])

    def _closing_pane(self, *, waiting: bool, code: int):
        """A launch whose pane is already dead with *code*, said the way tmux says it.

        The two sentences this is about are written after the pane has gone: charter asks
        `#{pane_dead_status}` eagerly, kills the window and then reports. So the fixture
        answers that question and lets `_launch` run to its end.
        """
        said: list[str] = []
        fake = _AServerWithOneWorkspaceRunning(sessions=[], chats=[], dead=f"1:{code}")
        with mock.patch.object(commands_frame.util, "err", side_effect=said.append), \
                mock.patch.object(commands_frame.util, "info", side_effect=said.append):
            self._launch(fake, harness="" if waiting else "claude",
                         select=waiting, profile=None if waiting else "claude")
        return " ".join(said)

    def test_a_cancelled_selector_says_nothing_about_a_death_or_a_recorded_plane(self):
        """The pane started nothing, so neither sentence can be true of it: one names a
        command that died and the other offers `charter reopen` for a chat the record does
        not hold (`leave.plan` passes over a waiting pane)."""
        said = self._closing_pane(waiting=True, code=selector.CANCELLED_EXIT)
        self.assertNotIn("before the frame was drawn", said)
        self.assertNotIn(commands_frame.SELECTOR_DISPLAY, said)
        self.assertNotIn("reopen", said)

    def test_a_pane_that_picked_and_then_died_still_says_so(self):
        """The control, and the reason the suppression reads the marker rather than the
        exit code: a harness that exits 130 because somebody pressed Ctrl-C at its own
        prompt cleared the marker when it was picked, so it reads as the death it is —
        the SAME number the case above suppresses, which is what makes this a control
        rather than a second case about a different input."""
        said = self._closing_pane(waiting=False, code=selector.CANCELLED_EXIT)
        self.assertIn("before the frame was drawn", said)
        self.assertIn("claude", said)

    def test_the_operators_own_tmux_records_the_same_two_things(self):
        """The other launch path, and it needs its own case because it writes its own
        records: a frame built as a window in the tmux the operator already had goes
        through `_launch_in_operator_tmux`, not through the private-server branch above.
        Both halves — the waiting marker before tmux, and no kind on the window — are what
        `launcher._picked` fills in at the pick, so a path that skipped either would leave
        a chat claiming a harness it has not started."""
        def answer(cmd, **kw):
            if not cmd or cmd[0] != "tmux":
                return _completed(cmd, 0)
            return _completed(cmd, 0, "%7\n")

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=answer), \
                mock.patch.object(commands_frame, "_wait_for_harness", return_value=130), \
                mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}):
            commands_frame._launch_in_operator_tmux(
                "op", "$1", ws="beta", argv=launcher.argv_select("claude"),
                display=[commands_frame.SELECTOR_DISPLAY], profile="", h=None,
                v=(3, 7), picked=False, selecting=True)
        self.assertTrue(state.is_waiting("beta.2"))
        self.assertEqual(state.identity("beta.2").get("CHARTER_HARNESS"), "")

    def test_a_reopen_never_opens_the_selector(self):
        """An open nobody is at names its profile: there is no one there to pick."""
        args = commands_frame._reopen_args(
            SimpleNamespace(workspace="beta", persona="", cwd="", resume="", brief=""),
            harness_name="claude", profile="claude-work", rest=[], reopening=None)
        self.assertFalse(getattr(args, "select", False))
        self.assertEqual(args.profile, "claude-work")

    def test_a_handoff_never_opens_the_selector(self):
        """#956's own rule read through profiles: the chat opened in the background takes
        the calling chat's profile, and a selector would stop it before it started —
        nobody is at a background open to answer a question."""
        state.record_profile("beta.1", "claude")
        (config.WORKSPACES_DIR / "gamma").mkdir(parents=True, exist_ok=True)
        seen: list = []
        # The launcher's own `PATH` check runs before a background open, and `claude` is
        # not on CI's — `charter.commands_frame.shutil` is the module, so this is the
        # answer that check gets.
        with mock.patch("charter.commands_frame.shutil.which", return_value="/nowhere/x"), \
                mock.patch.object(commands_frame, "cmd_launch",
                               side_effect=lambda a: seen.append(a) or 0), \
                mock.patch.object(commands_frame, "_plane_session",
                                  return_value=("$3", "beta.1")), \
                mock.patch.object(commands_frame, "_window_size", return_value=(120, 40)), \
                mock.patch.object(commands_frame, "_live_sessions", return_value=set()):
            commands_frame.open_in_background(
                "gamma", caller="beta.1",
                first_message="pick up the review notes in gamma")
        self.assertEqual(len(seen), 1, seen)
        self.assertFalse(getattr(seen[0], "select", False))
        self.assertEqual(seen[0].profile, "claude")

_HAS_TMUX = shutil.which("tmux") is not None

#: A fresh socket per CASE, never per class — a server told to exit is still accepting on
#: its socket for a few milliseconds, and the next case's `new-session` draws that race.
_SERVERS = itertools.count()

#: This checkout, for the `$PYTHONPATH` the pane's own launcher needs. `python -P -m
#: charter` keeps the child's cwd off `sys.path` (#390) and its cwd is a workspace
#: directory, so without this the pane cannot import the charter under test.
_REPO_ROOT = Path(__file__).resolve().parents[1]


def _eventually(predicate, timeout: float = 20.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return bool(predicate())


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TheSelectorOnARealServer(PersonaIso, unittest.TestCase):
    """The selector in a REAL chat pane, driven by real keystrokes through a real tmux.

    **Nothing here is patched inside the pane.** The keys go in with `send-keys`, which is
    what makes this the measurement the plan asks for rather than a second unit test: the
    pane really puts its terminal into raw mode (`palette.own_the_tty`), really paints the
    alternate screen, and really `exec`s the profile's command on Enter. That raw-mode call
    is the one thing ADR 0018's previous amendment deliberately avoided after a `tcsetattr`
    from a launcher pane was measured leaving the process killed by a signal on a Linux
    runner — so this is where that stops being an argument.

    The launch runs on a WORKER, because `_launch` owns this thread until the pane is gone.
    The recorder standing in for `claude` is first on the client's `PATH`: the launcher
    resolves the profile in the PANE, so patching `Harness.binary` in this process would
    reach nothing.
    """

    WS = "beta"

    def setUp(self) -> None:
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]}"
                          f"; this machine has {v}")
        make_plane(self)
        no_background_refresh(self)
        no_update_check_in(config.ROOT)
        _ttyguard.no_terminal()
        self.tmux = shutil.which("tmux")
        self.socket = _tmuxreap.name(f"the-selector-{next(_SERVERS)}")
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        self.addCleanup(self._reap_the_server)
        self.enterContext(mock.patch.object(commands_frame, "_spawn_gather"))
        self.enterContext(mock.patch.object(commands_frame, "_drawable_slots",
                                            return_value=[]))
        (config.WORKSPACES_DIR / self.WS).mkdir(parents=True, exist_ok=True)
        self.records = self.tmp / "records"
        self.records.mkdir()
        bindir = self.tmp / "bin"
        bindir.mkdir()
        (bindir / "claude").write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys, time\n"
            "out = os.path.join(os.environ['RECORD_DIR'], 'harness.json')\n"
            "with open(out + '.tmp', 'w') as f:\n"
            "    json.dump({'argv': sys.argv[1:], 'pid': os.getpid()}, f)\n"
            "os.replace(out + '.tmp', out)\n"
            "time.sleep(300)\n")
        (bindir / "claude").chmod(0o755)
        self.enterContext(mock.patch.dict(os.environ, {
            "PATH": f"{bindir}{os.pathsep}{os.environ.get('PATH', '')}",
            "RECORD_DIR": str(self.records),
            "CHARTER_ROOT": str(config.ROOT),
            "PYTHONPATH": os.pathsep.join(
                [str(_REPO_ROOT), os.environ.get("PYTHONPATH", "")]).rstrip(os.pathsep),
            **_gitguard.environment(),
        }, clear=True))

    def _reap_the_server(self) -> None:
        subprocess.run([self.tmux, "-L", self.socket, "kill-server"],
                       capture_output=True, timeout=20)
        try:
            os.unlink(_tmuxsocket.socket_path(self.socket))
        except OSError:
            pass

    def _tmux(self, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run([self.tmux, "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20)

    def _in_the_background(self):
        """Run the selector launch on a worker and hand back the thread and its answer.

        `_launch` holds its thread until the pane is gone (`_wait_for_harness`), and the
        keystroke that ends the selector has to come from somewhere else.
        """
        out: list = []

        def work() -> None:
            args = SimpleNamespace(harness="", rest=[], no_frame=False, workspace=self.WS,
                                   pick=False, select=True, start="claude", profile=None,
                                   attach=False, size=(120, 40))
            try:
                out.append(commands_frame._launch(args))
            except BaseException as e:                    # noqa: BLE001 — reported
                out.append(e)

        t = threading.Thread(target=work, daemon=True)
        t.start()
        return t, out

    def _pane_showing_the_selector(self) -> str:
        """The harness pane of `beta.1`, once the selector has painted in it."""
        self.assertTrue(_eventually(lambda: bool(state.harness_pane("beta.1"))),
                        "the launch never recorded a harness pane")
        pane = state.harness_pane("beta.1")
        self.assertTrue(
            _eventually(lambda: "which profile" in self._text(pane)),
            f"the selector never painted: {self._text(pane)!r}")
        return pane

    def _text(self, pane: str) -> str:
        return "".join(self._tmux("capture-pane", "-p", "-t", pane).stdout.splitlines())

    def _status(self, pane: str) -> str:
        """`#{pane_dead}:#{pane_dead_status}` for *pane*, or what tmux said instead.

        A pane tmux no longer lists at all is gone, which is the same answer as dead for
        every claim here — the window went with it.
        """
        out = self._tmux("display-message", "-p", "-t", pane,
                         "#{pane_dead}:#{pane_dead_status}:#{pane_dead_signal}")
        return out.stdout.strip() if out.returncode == 0 else f"(gone: {out.stderr.strip()})"

    def _dead(self, pane: str) -> bool:
        """Whether *pane*'s own process has ended — an empty `#{pane_dead_status}` included.

        Measured on the CI runner, 2026-09-12: a pane that exits out of raw mode on Linux
        comes back dead with an EMPTY status where the same pane on macOS carries the
        number (`commands_frame._UNKNOWN_DEATH_CODE`, and ruling 42). So the question is
        whether the process ended, never what it ended with — and `#{pane_dead_signal}` is
        read into the message beside it, because an empty status is tmux's own spelling for
        *killed by a signal* and the signal is the thing a reader would want next.
        """
        return self._status(pane).startswith(("1:", "(gone"))

    def test_a_cancelled_only_window_takes_the_session_with_it(self):
        """S2, and the frame's own rule for its last chat: Esc closes that chat having
        started nothing, and the workspace's session goes with its last window."""
        t, out = self._in_the_background()
        pane = self._pane_showing_the_selector()
        self._tmux("send-keys", "-t", pane, "Escape")
        t.join(timeout=40)
        self.assertFalse(t.is_alive(), "the launch never returned")
        # **The pane first, the session second**, so a failure says which link broke. A lone
        # ESC is the one key `overlay.decode` cannot resolve from its own bytes — it waits
        # for a tick with nothing behind it (`palette.TICK`) — so "the surface left" and
        # "the window went" are two claims, and reading them as one would blame tmux for a
        # keystroke that never arrived.
        self.assertTrue(
            _eventually(lambda: self._dead(pane), timeout=30.0),
            f"the selector never left on Escape: pane {self._status(pane)}, "
            f"screen {self._text(pane)!r}")
        # **The exit NUMBER is deliberately not asserted, and that is measured rather than
        # conceded.** A pane that exits out of raw mode on the Linux runner comes back with
        # an empty `#{pane_dead_status}` — `commands_frame._UNKNOWN_DEATH_CODE`'s own
        # measurement, and ruling 42's — so 130 reaches the chat's `exit` record here and
        # not there. What S2 is about survives that on both: the window went, the session
        # went with it, nothing was started, and the pane is still a waiting one. It is
        # also why `_launch` suppresses its early-death sentence on the MARKER rather than
        # on the code (`nothing_ever_ran_here`).
        #
        # `out == [0]` because a launch that will never attach returns as soon as the
        # window exists (`_wants_attach`) — long before anybody presses anything.
        self.assertEqual(out, [0])
        # **A minute, and the number is measured rather than chosen.** What is left here is
        # tmux's own work: charter's `pane-died[1] kill-window` is installed before the pane
        # can die (the message below reads it back), and the window going is that hook
        # firing. On CI the same commit was green on one Python and red at 30 s on another,
        # with the hooks present, `remain-on-exit on` and the pane dead — a runner busy with
        # six sweep shards, not a rule about the platform. Doubling the margin is the honest
        # fix for a claim about somebody else's queue.
        self.assertTrue(
            _eventually(lambda: self.WS not in self._tmux("list-sessions").stdout,
                        timeout=60.0),
            f"the cancelled chat's session outlived its only window: "
            f"windows {self._tmux('list-windows', '-a', '-F', '#{session_name}:#{window_id}').stdout!r}, "
            f"pane {self._status(pane)!r}, "
            f"remain-on-exit "
            f"{self._tmux('show-options', '-g', 'remain-on-exit').stdout.strip()!r}, "
            f"hooks {self._tmux('show-hooks', '-p', '-t', pane).stdout.strip()!r}")
        self.assertFalse((self.records / "harness.json").exists(),
                         "a cancelled selector started a harness")
        self.assertTrue(state.is_waiting("beta.1"),
                        "a cancelled pane stopped being a waiting pane, so a quit would "
                        "record a chat that never started")

    def test_a_pick_leaves_the_same_pane_running_the_harness(self):
        """L1 for the selector: the pane charter recorded is the pane the harness is in,
        because `exec` kept the launcher's pid — and the pick is a real Enter on a real
        raw-mode surface."""
        t, out = self._in_the_background()
        pane = self._pane_showing_the_selector()
        self._tmux("send-keys", "-t", pane, "Enter")
        record = self.records / "harness.json"
        self.assertTrue(_eventually(record.is_file),
                        f"the harness never started: {self._text(pane)!r}")
        harness = json.loads(record.read_text())
        pid = self._tmux("display-message", "-p", "-t", pane, "#{pane_pid}").stdout.strip()
        self.assertEqual(pid, str(harness["pid"]),
                         "the exec did not keep the launcher's pid for the harness")
        self.assertTrue(_eventually(lambda: not state.is_waiting("beta.1")),
                        "the pick left the pane recorded as still waiting")
        self.assertEqual(state.profile("beta.1"), "claude")


if __name__ == "__main__":
    unittest.main()
