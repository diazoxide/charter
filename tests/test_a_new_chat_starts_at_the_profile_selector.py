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

**Where a row's state comes from Tasks 3 and 4, this drives the seam**
(`selector.pending`): that function is what will ask `profiletrust.approval_needed` and
`wiring.detect` once those land, and every case here about "not approved yet" or "not wired"
states it through the seam rather than through either module, so the row rules are pinned
before the answers exist.
"""

from __future__ import annotations

import os
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import config, profiles, util
from charter.frame import chats, launcher, leave, overlay, palette, selector, state
from tests import _gitguard
from tests._isolation import PersonaIso, declare_profiles, make_plane

#: A profile whose row can run: on `PATH`, approved and wired. The fixture's `which` says
#: yes to everything, and `pending` answers `None` until Tasks 3 and 4 fill it in.
WORK = "claude-work"


class _APlaneWithProfiles(PersonaIso):
    """A plane declaring `claude-work` and `codex-pinned`, in a git repository that ignores
    the local file — `tests._isolation.declare_profiles`' fixture, which runs a real
    `git status` because `profiles.ignore_check` does.

    `shutil.which` answers a path for every program, so a built-in is listed and a declared
    profile is not refused for being absent from this machine's `PATH`. A case about either
    says so itself.
    """

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        self.local = declare_profiles(self)
        self.which = self.enterContext(mock.patch.object(
            selector.shutil, "which", side_effect=lambda cmd, **kw: f"/usr/bin/{cmd}"))

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

    def test_control_bytes_in_a_command_are_escaped_in_its_row(self):
        """Ruling 35: `charter.local.toml` is a file a chat can write, and a `\\r` or an ESC
        in a command could otherwise redraw the row to show a harmless command."""
        self.local.write_text('[harness.sneaky]\nkind = "claude"\n'
                              'command = ["cl\\raude\\u001b[2Kharmless"]\n')
        row = self._row(self._rows(), "sneaky")
        line = "".join(selector.Selector(catalogue=(row,)).render(200, 6))
        for text in (row.note, line):
            self.assertNotIn("\r", text)
            self.assertNotIn("\x1b[2K", text)
        self.assertIn("\\u000d", row.note)

    def test_a_long_profile_name_is_clipped_in_its_title(self):
        """`profiles.NAME_RE` bounds a name's shape and says nothing about its length, so
        the title is the one value here that arrives printable and unbounded."""
        self.local.write_text('[harness.%s]\nkind = "claude"\ncommand = ["claude"]\n'
                              % ("w" * 200))
        row = self._row(self._rows(), "w" * 200)
        self.assertTrue(row.title.endswith("..."))
        self.assertLess(len(row.title), 200)


class TheSeamTasksThreeAndFourFill(_APlaneWithProfiles, unittest.TestCase):
    """One function answers "is this profile approved, and is it wired" — `selector.pending`.

    Task 3's approval and Task 4's wiring both land in it, so the ROW rules are pinned here
    and the two answers are filled in one place. Until then it answers `None`, which is what
    makes this branch's selector list every installed profile as runnable.
    """

    def test_a_state_the_seam_refuses_makes_the_row_refused_with_its_fix(self):
        with mock.patch.object(selector, "pending", return_value=selector.Pending(
                refused=True, note="not wired — charter harness install claude-work",
                ask=False)):
            row = self._row(self._rows(), WORK)
        self.assertTrue(row.refused)
        self.assertIn("charter harness install claude-work", row.note)

    def test_a_profile_that_asks_is_not_refused_and_says_it_is_not_approved_yet(self):
        """Task 3: a new or changed profile is not refused — Enter shows its command and
        asks in place — so its row must stay runnable or Enter would only say why."""
        with mock.patch.object(selector, "pending", return_value=selector.Pending(
                refused=False, note="not approved yet (new) — Enter shows its command",
                ask=True)):
            row = self._row(self._rows(), WORK)
        self.assertFalse(row.refused)
        self.assertIn("not approved yet (new)", row.note)

    def test_the_seam_is_asked_for_every_listed_profile_and_no_refused_one(self):
        """Ruling 1: a probe never runs for a profile whose launch record does not match,
        and a profile the file itself refused has no command to probe at all."""
        self.local.write_text('[harness.broken]\nkind = "claude"\ncommand = "x"\n'
                              '[harness.%s]\nkind = "claude"\ncommand = ["claude"]\n' % WORK)
        asked: list[str] = []
        with mock.patch.object(selector, "pending",
                               side_effect=lambda p, **kw: asked.append(p.name)):
            self._rows()
        self.assertIn(WORK, asked)
        self.assertNotIn("broken", asked)

    def test_nothing_on_this_branch_spawns_a_probe(self):
        """Ruling 11 and B2: drawing the rows runs no profile command. `util.run` is what
        every probe will go through, so a `rows` that spawned one is a raise here — read
        first, because the ignore check is the one subprocess profile code is allowed."""
        have = self._read()
        with mock.patch.object(util, "run", side_effect=AssertionError("spawned")):
            self.assertTrue(selector.rows(have, cwd=Path(config.ROOT)))
            self.assertIsNone(selector.pending(have.profiles[WORK], cwd=Path(config.ROOT)))


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


class TheConfirmAsksInPlace(_APickedSelector, unittest.TestCase):
    """Task 3's ask, drawn in the selector's own pane: Enter on a new or changed profile
    shows its command and asks `run this?` there rather than anywhere else."""

    def setUp(self) -> None:
        super().setUp()
        self.approved: list[str] = []
        self.enterContext(mock.patch.object(
            selector, "approve", side_effect=lambda p: self.approved.append(p.name) or ""))
        self.enterContext(mock.patch.object(selector, "pending", side_effect=self._pending))

    def _pending(self, p, **kw):
        if p.name in self.approved or p.source == profiles.BUILTIN:
            return None
        return selector.Pending(False, "not approved yet (new) — Enter shows its command",
                                ask=True)

    def test_yes_records_the_approval_and_is_the_choice(self):
        self._queue(self._row_named(WORK), selector.CONFIRM_ROW)
        self.assertEqual(self._pick(), selector.Choice(WORK))
        self.assertEqual(self.approved, [WORK])
        self.assertIn("run this?", self.surfaces[1].heading)
        self.assertIn("CLAUDE_CONFIG_DIR=", self.surfaces[1].heading)

    def test_no_at_the_confirm_goes_back_to_the_list_and_records_nothing(self):
        self._queue(self._row_named(WORK), None, None)
        self.assertIsNone(self._pick())
        self.assertEqual(self.approved, [])
        self.assertIsInstance(self.surfaces[2], selector.Selector)

    def test_y_chooses_and_every_other_key_goes_back(self):
        confirm = selector.Confirm()
        self.assertEqual(confirm.handle(overlay.Event(overlay.KEY, "y"), 8), overlay.CHOOSE)
        self.assertEqual(confirm.handle(overlay.Event(overlay.KEY, "n"), 8), overlay.CANCEL)
        self.assertEqual(confirm.handle(overlay.Event(overlay.KEY, "escape"), 8),
                         overlay.CANCEL)
        self.assertIsNone(confirm.handle(overlay.Event(overlay.SCROLL, "up"), 8))

    def test_a_confirm_that_is_chosen_answers_a_row(self):
        """`overlay.Surface.run` answers `self.selected` for a CHOOSE, and `None` for a
        surface with no rows — which reads as a cancel. So the confirm has a row."""
        self.assertTrue(selector.Confirm().rows)
        self.assertIsNotNone(selector.Confirm().selected)

    def test_an_approval_that_could_not_be_written_returns_to_the_selector(self):
        """The fourth review's nit: a launch record that fails to write refuses rather than
        re-asking, so `approve` answers the write error and the pick does not proceed."""
        with mock.patch.object(selector, "approve", return_value="could not write it"):
            self._queue(self._row_named(WORK), selector.CONFIRM_ROW, None)
            self.assertIsNone(self._pick())
        self.assertIn("could not write it", self.surfaces[2].footer)


class NothingTheOperatorMustReadIsCutWithoutAWord(_APlaneWithProfiles, unittest.TestCase):
    """Ruling 35's clipping clause, on the three surfaces that carry profile-derived text.

    An ellipsis marks a cut and not its size, and on all three the size is the question: a
    refusal's reason appears in the footer and NOWHERE else on this surface, and an approval
    prompt holds the very command a `y` is about to run.
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

    def test_an_approval_prompt_past_one_pane_says_how_much_it_hid(self):
        """F4: a `y` given to a silently cut prompt approves a command the operator did not
        see the end of."""
        head = selector.Confirm(
            heading=selector.CONFIRM.format(shown=self.LONG)).render(70, 8)[0]
        self.assertIn("not shown", head)
        self.assertIn("run this?", head)

    def test_an_approval_prompt_never_says_how_many_rows_it_has(self):
        """It asks one question. `· 1 to choose from` after a command describes the widget
        rather than the thing being approved."""
        head = selector.Confirm(heading="run this? claude --foo").render(120, 8)[0]
        self.assertNotIn("to choose from", head)

    def test_a_list_still_says_how_many_rows_it_has(self):
        """The control: the count is right for a surface somebody is choosing FROM, and
        `Confirm` opting out must not take it from the selector."""
        self.assertIn("to choose from",
                      selector.Selector(catalogue=self._rows()).render(120, 8)[0])


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

    def test_the_confirms_footer_names_esc_too(self):
        self.assertIn(selector.ESC_HINT, selector.CONFIRM_FOOTER)

    def test_the_esc_hint_survives_a_reason_too_wide_for_the_pane(self):
        """Which is why the hint goes FIRST when there is a reason: whatever is last is what
        a narrow pane takes, and this is the half that must not be taken."""
        line = selector.Selector(
            catalogue=self._rows(),
            footer=selector._footer((), selector.Refused(WORK, "y" * 400))).render(70, 8)[-1]
        self.assertIn(selector.ESC_HINT, line)
        self.assertIn("not shown", line)

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

    def test_a_capital_y_is_a_yes_as_well(self):
        """`Confirm` reads the key an operator with caps lock on pressed. Anything but a
        yes goes back to the list, so a `Y` read as *anything else* is an approval an
        operator gave and charter did not take."""
        for key in ("y", "Y"):
            with self.subTest(key=key):
                self.assertEqual(
                    selector.Confirm().handle(overlay.Event(overlay.KEY, key), 8),
                    overlay.CHOOSE)

    def test_the_waiting_marker_never_raises_over_a_chat_id_that_is_no_directory(self):
        """All three writers, and the same shape `state.record_closed` has: `frame_dir`
        REFUSES an id it cannot make a directory of rather than raising, so each of these
        has to answer for that — two of them on a path a panel and a quit run."""
        self.assertIsNone(state.record_waiting(""))
        self.assertFalse(state.is_waiting(""))
        self.assertIsNone(state.clear_waiting(""))

    def test_forgetting_a_waiting_pane_survives_a_filesystem_that_refuses(self):
        """`clear_waiting`'s catch. It runs at the pick, in the pane, one line before the
        `exec` — so an `OSError` raised out of it would take down the launch that was about
        to hand the pane to the harness, over a marker whose whole job is already done."""
        state.record_waiting("beta.9")
        with mock.patch.object(Path, "unlink", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.clear_waiting("beta.9"))
        self.assertTrue(state.is_waiting("beta.9"))


class ThePaneWaitsThenBecomesTheHarness(_APlaneWithProfiles, unittest.TestCase):
    """`charter frame-launch --select` — what a new chat's window runs before any harness."""

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
        self.execs: list[tuple] = []
        self.exec = self.enterContext(mock.patch.object(
            launcher.os, "execvpe", side_effect=lambda *a: self.execs.append(a)))
        # Task 2 refuses every DECLARED profile until Task 3's approval lands (review B1),
        # so without this stand-in every case here would be measuring that one sentence
        # instead of the selector. `tests/test_the_launcher_becomes_the_profile.py` stands
        # in for it the same way, and Task 3 replaces the body both of them patch.
        self.enterContext(mock.patch.object(launcher, "_approval_refusal",
                                            return_value=None))
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
        ever run in, and it closes its own window.
        """
        state.record_harness_pane("beta.1", "%7")
        state.record_server("beta.1", "some-socket")
        killed: list = []
        with mock.patch.object(launcher.tmuxctl, "run",
                               side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(len(killed), 1, killed)
        self.assertEqual(killed[0][-3:], ["kill-window", "-t", "%7"])
        self.assertIn("some-socket", killed[0],
                      "the chat's own server, never charter's default")

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
        would be this launch closing a pane it could not prove was its own."""
        killed: list = []
        with mock.patch.object(launcher, "framed_chat", return_value=None), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self.assertEqual(self._run(), selector.CANCELLED_EXIT)
        self.assertEqual(killed, [])

    def test_a_chat_with_no_pane_recorded_falls_back_to_the_one_tmux_named(self):
        """The migration case, and the same pair `cmd_new_chat` reads: the record is what
        the LAUNCH wrote down, and `$TMUX_PANE` is what tmux put in this process."""
        state.record_harness_pane("beta.1", "")
        killed: list = []
        with mock.patch.dict(os.environ, {"TMUX_PANE": "%3"}), \
                mock.patch.object(launcher.tmuxctl, "run",
                                  side_effect=lambda a, argv, **kw: killed.append(argv)):
            self._run()
        self.assertEqual(killed[0][-3:], ["kill-window", "-t", "%3"])

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
        line = overlay.Surface(rows=(overlay.Row("a", "a"),),
                               footer="a\rb\nc").render(120, 6)[-1]
        self.assertNotIn("\r", line)
        self.assertNotIn("\n", line)


if __name__ == "__main__":
    unittest.main()
