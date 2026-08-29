"""Phase 5 Stage 5b, Task 4 and Task 6: switching between the chats of one workspace.

Two halves, and they are separated the way the code is. `frame/chats.py` decides —
which chats a workspace holds, which one you are in, and whether a name may be switched
to — and touches no tmux, so everything in :class:`TheRosterIsTheDirectory` and
:class:`TheCheckSaysWhichRefusalFired` runs against a plane and nothing else.
`commands_frame.cmd_chat` performs, and :class:`TheSwitchIsFourStepsInOneOrder` measures
the argv it sends with a fake server underneath it.

**The order the four steps go in is the only thing that makes the panels correct**, and
it is asserted as an order rather than as a set: `select-window` first, because a
background window keeps stale geometry on tmux 3.7c and on tmux 3.2 alike and tmux
resizes it AT the switch; then the teardown, then the split, so the new panels are born
in a window that has already been resized. `tests/test_frame_tmux_integration
.SwitchingBetweenChatsMovesTheClientAndThePanels` is where that is measured against a
real server with a real attached client, because it is the half a fake cannot make a
claim about.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import commands_frame, config, contain, instance
from charter.frame import chats, choose, slots, state, tmuxctl
from tests._isolation import PersonaIso
# The one list of hostile display strings this suite has, imported rather than retyped:
# a second copy is one that stops growing when somebody finds a ninth shape.
from tests.test_frame_pickers import HOSTILE


def _plant(fid: str, *, workspace: str, pane: str = "%1",
           harness: str = "Claude Code") -> None:
    """Make *fid* look like a chat charter launched into *workspace*.

    The production writers, never a hand-written file: `record_workspace` is what
    `frame_workspace` reads back and `record_identity` is what `state.identity` does, so
    a test that stopped agreeing with either of them fails here rather than passing
    against its own fixture.
    """
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, workspace)
    state.record_harness_pane(fid, pane)
    state.record_identity(fid, {"CHARTER_HARNESS": harness})


class TheRosterIsTheDirectory(PersonaIso, unittest.TestCase):
    """Spec §3.5: there is no per-workspace index of chats, so `.charter/frame/` is the
    list. What that costs and what it buys are both measured here."""

    def test_a_workspaces_chats_are_the_directories_that_name_it(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        _plant("web.1", workspace="web")
        self.assertEqual(chats.of_workspace("api"), ["api.1", "api.2"])
        self.assertEqual(chats.of_workspace("web"), ["web.1"])

    def test_membership_is_the_recorded_workspace_and_never_the_ids_prefix(self):
        """Spec §3.2's rename property, and the one that makes the id a NAME rather than
        a pointer. A workspace renamed under two live chats keeps both: their ids still
        spell the old name and their `workspace` file is what says where they belong."""
        _plant("old.1", workspace="old")
        _plant("old.2", workspace="old")
        for fid in ("old.1", "old.2"):
            state.record_workspace(fid, "renamed")
        self.assertEqual(chats.of_workspace("renamed"), ["old.1", "old.2"])
        self.assertEqual(chats.of_workspace("old"), [])

    def test_an_old_shape_frame_is_not_a_chat(self):
        """`{workspace}-{pid}` is Stage 5a's other id shape and it is still launchable.
        `state._launcher_pid` is the discriminator — asked once, in `chats.is_chat`, so
        a bar cannot offer to `select-window` at a frame that is a whole tmux session."""
        _plant("api-4242", workspace="api")
        _plant("api.1", workspace="api")
        self.assertFalse(chats.is_chat("api-4242"))
        self.assertTrue(chats.is_chat("api.1"))
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_a_directory_name_outside_the_id_alphabet_is_not_a_chat(self):
        """A name off `os.scandir` is whatever is on disk, and it goes on to a palette
        row, a `frame-chat` argv and a tmux `-t`. `chats.ID_RE` is that alphabet, asked
        where the name enters charter's vocabulary rather than at each of the three
        places it leaves it."""
        root = state._root()
        root.mkdir(parents=True, exist_ok=True)
        for hostile in ("api.1;kill-server", "api 1", "api\t1"):
            (root / hostile).mkdir()
            state.record_workspace(hostile, "api")
        _plant("api.1", workspace="api")
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_the_order_is_the_ordinal_and_not_the_directorys(self):
        """Ordinal order, so `api.1` stays leftmost on the bar where an operator learned
        to look for it. Ten sorts after two, which string order would not do."""
        for n in (10, 2, 1, 3):
            _plant(f"api.{n}", workspace="api")
        self.assertEqual(chats.of_workspace("api"),
                         ["api.1", "api.2", "api.3", "api.10"])

    def test_a_chat_whose_ordinal_cannot_be_read_sorts_last_rather_than_vanishing(self):
        """The migration and corruption case. A chat charter cannot read the ordinal of
        is still a chat whose window may be on screen, so it is ordered rather than
        dropped — dropping it would leave it out of the picker while it is running."""
        _plant("api.2", workspace="api")
        _plant("api.notanumber", workspace="api")
        self.assertEqual(chats.of_workspace("api"), ["api.2", "api.notanumber"])

    def test_an_ordinal_too_long_to_convert_sorts_last_rather_than_raising(self):
        """`int("9" * 5000)` raises `ValueError` on CPython — the int-str conversion
        limit, not an overflow — and unbounded that exception comes out of `sorted`, in a
        panel's render path.

        **Not through the directory**, which cannot hold a name that long (`NAME_MAX` is
        255 on every filesystem this runs on, so `mkdir` answers `ENAMETOOLONG` first).
        Through the fid `roster` folds in, which is `$CHARTER_SESSION_ID` — a value from
        the environment, not from a directory listing, and the one input to this sort with
        no length bound in front of it.
        """
        _plant("api.2", workspace="api")
        huge = "api." + "9" * 5000
        self.assertTrue(chats.is_chat(huge),
                        "if this stopped being a chat the sort would never see it and "
                        "this test would be measuring the wrong guard")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual([c.id for c in chats.roster(huge)], ["api.2", huge])

    def test_the_roster_marks_the_chat_asking_and_carries_each_ones_harness(self):
        _plant("api.1", workspace="api", harness="Claude Code")
        _plant("api.2", workspace="api", harness="Codex")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            roster = chats.roster("api.2")
        self.assertEqual([(c.id, c.harness, c.active) for c in roster],
                         [("api.1", "Claude Code", False),
                          ("api.2", "Codex", True)])

    def test_the_chat_asking_is_in_its_own_roster_even_with_no_record(self):
        """A frame whose `workspace` file could not be read is still the chat you are
        typing in. A bar that omitted the active chat would draw a list the operator is
        not in."""
        _plant("api.1", workspace="api")
        state.frame_dir("api.9", create=True)     # no `workspace` written at all
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            roster = chats.roster("api.9")
        self.assertEqual([c.id for c in roster], ["api.1", "api.9"])
        self.assertTrue(roster[-1].active)

    def test_others_is_the_roster_without_you(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual(chats.others("api.1"), ["api.2"])
            self.assertEqual(chats.others("api.2"), ["api.1"])

    def test_a_loose_file_under_the_frame_root_is_not_a_chat(self):
        """The directory is the list, and a file in it is not a directory. `exit`, a
        temp file a write was interrupted mid-`os.replace`, anything — none of them has a
        frame directory's shape and none may become a row."""
        _plant("api.1", workspace="api")
        (state._root() / "api.2").write_text("not a chat\n")
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_a_chat_with_no_recorded_workspace_belongs_to_none(self):
        """`frame_workspace` answers `None` for the migration case and the corrupt one,
        and `None` is not a workspace name — so the chat is in nobody's list rather than
        in everybody's."""
        _plant("api.1", workspace="api")
        state.frame_dir("api.2", create=True)
        self.assertEqual(chats.of_workspace("api"), ["api.1"])

    def test_the_roster_invents_nothing_for_a_frame_that_is_not_a_chat(self):
        """`roster` folds the asking frame in so a chat whose record is unreadable is
        still on its own bar — but only when it IS a chat. An old `{workspace}-{pid}`
        frame, or no frame at all, must not become a row that `select-window` would be
        aimed at."""
        _plant("api.1", workspace="api")
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"}, clear=False):
            self.assertEqual([c.id for c in chats.roster("")], ["api.1"])
            self.assertEqual([c.id for c in chats.roster("api-4242")], ["api.1"])
            # And it is not folded in twice when the scan already found it.
            self.assertEqual([c.id for c in chats.roster("api.1")], ["api.1"])

    def test_a_harness_recorded_as_whitespace_is_no_harness(self):
        """`state.identity` does not strip — it hands back what the file holds — so a
        note of three spaces would be a column of blank that reads as a harness charter
        could not name. Empty says the honest thing."""
        _plant("api.1", workspace="api")
        state.record_identity("api.1", {"CHARTER_HARNESS": "   "})
        self.assertEqual(chats.harness_of("api.1"), "")

    def test_a_frame_root_that_cannot_be_scanned_offers_no_chats_and_does_not_raise(self):
        """A bar that could not scan draws nothing; it does not take the panel down with
        it. `os.scandir` on a plane that has never launched a frame is the ordinary form
        of this, and it is the same answer."""
        self.assertEqual(chats.of_workspace("api"), [])
        with mock.patch("os.scandir", side_effect=OSError("nope")):
            self.assertEqual(chats.of_workspace("api"), [])

    def test_the_harness_comes_from_the_frames_own_record_not_this_process(self):
        """`state.identity`, never `os.environ` — a palette is a `run-shell` child of a
        server shared between every frame on the machine, so this process's own
        `$CHARTER_HARNESS` may be another chat's."""
        _plant("api.1", workspace="api", harness="Codex")
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "Something Else"},
                             clear=False):
            self.assertEqual(chats.harness_of("api.1"), "Codex")
        self.assertEqual(chats.harness_of("api.404"), "")


class TheCheckSaysWhichRefusalFired(PersonaIso, unittest.TestCase):
    """Every refusal names itself, because an assertion about a bare `ok is False` cannot
    tell two guards in sequence apart — this repository's commonest defect, and the
    reason the deletion sweep asks for the reason rather than the refusal."""

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_a_name_outside_the_alphabet_is_refused_as_a_name(self):
        out = chats.check("api.1", "api.2;kill-server")
        self.assertFalse(out.ok)
        self.assertIn("cannot name a chat", out.message)

    def test_an_absent_name_is_refused_as_a_name_rather_than_as_unknown(self):
        """Both spellings of absent, and `None` is the one that would be a `TypeError`
        out of `re.fullmatch` rather than a refusal. `charter frame-chat` cannot produce
        it — argparse gives a string — but this is the one rule two callers share, and a
        rule that raises for one of the ways of being asked nothing is not one."""
        for absent in ("", None):
            self.assertIn("cannot name a chat",
                          chats.check("api.1", absent).message, repr(absent))

    def test_an_unknown_chat_is_refused_with_the_ones_this_workspace_has(self):
        out = chats.check("api.1", "api.7")
        self.assertFalse(out.ok)
        self.assertIn("no chat 'api.7' here", out.message)
        self.assertIn("api.2", out.message)

    def test_a_chat_of_another_workspace_is_unknown_here(self):
        _plant("web.1", workspace="web")
        self.assertIn("no chat 'web.1' here", chats.check("api.1", "web.1").message)

    def test_switching_to_the_chat_you_are_in_is_refused_and_says_so(self):
        """Refused rather than performed: it would tear this chat's panels down and split
        them again for no change on screen, which is ~90 ms of blank panes nobody asked
        for."""
        out = chats.check("api.1", "api.1")
        self.assertFalse(out.ok)
        self.assertIn("already in chat 'api.1'", out.message)

    def test_a_chat_with_no_usable_pane_record_is_refused_with_the_fix(self):
        """#475's boundary: this value comes off disk and is about to be a tmux `-t`."""
        state.record_harness_pane("api.2", "%1;kill-server")
        out = chats.check("api.1", "api.2")
        self.assertFalse(out.ok)
        self.assertIn("no usable record", out.message)
        self.assertIn("relaunch that chat", out.message)

    def test_a_switch_that_may_go_ahead_names_where_it_is_going(self):
        out = chats.check("api.1", "api.2")
        self.assertTrue(out.ok)
        self.assertEqual(out.message, "chat → api.2")

    def test_every_message_is_one_line_whatever_the_name_carried(self):
        """A message is a line of charter's own output and reaches `display-message`,
        which is a tmux FORMAT. `contain.one_line` is what stops a name forging a second
        line of it (#453)."""
        for hostile in ("api\n✗ nope", "api x", "api\x1b[31mR", "api#(touch /tmp/x)"):
            msg = chats.check("api.1", hostile).message
            self.assertEqual(msg, contain.one_line(msg))
            self.assertNotIn("\n", msg)


class TheChatDoorwaySaysWhyBeforeTheKeypress(PersonaIso, unittest.TestCase):
    """`choose.pin_reason`'s rule, one noun over: an operator cannot ask about an option
    they cannot see (#512), so the doorway carries the reason and the picker is not
    opened over a list of one row that is yourself."""

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_one_chat_means_no_picker_and_the_row_says_how_to_get_a_second(self):
        _plant("api.1", workspace="api")
        reason = choose.pin_reason(choose.CHAT, "api.1")
        self.assertEqual(reason, chats.ONLY_CHAT)
        self.assertIn("charter <harness>", reason)

    def test_two_chats_means_the_doorway_opens(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        self.assertEqual(choose.pin_reason(choose.CHAT, "api.1"), "")

    def test_the_chat_you_are_in_is_the_marked_row(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        rows = choose.roster(choose.CHAT, "api.2").rows
        marked = [r.title for r in rows if r.title.startswith(choose.MARK[0])]
        self.assertEqual(marked, [f"{choose.MARK[0]}api.2"])

    def test_a_chats_row_carries_its_harness_and_the_other_nouns_carry_nothing(self):
        _plant("api.1", workspace="api", harness="Claude Code")
        _plant("api.2", workspace="api", harness="Codex")
        notes = {r.title[len(choose.MARK[0]):]: r.note
                 for r in choose.roster(choose.CHAT, "api.1").rows}
        self.assertEqual(notes, {"api.1": "Claude Code", "api.2": "Codex"})
        self.assertEqual(choose._note(choose.WORKSPACE, "api"), "")

    def test_no_launch_pin_can_refuse_a_chat_doorway(self):
        """`$CHARTER_SESSION_ID` IS a chat's identity and is set on every frame, so an
        entry in `choose.PIN` for `chat` would refuse the doorway on every frame there
        has ever been."""
        self.assertNotIn(choose.CHAT, choose.PIN)

    def test_a_chat_row_id_can_never_be_an_action_id(self):
        from charter.frame import component
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        ids = ([choose.OPEN_ID.format(choose.CHAT)]
               + [r.id for r in choose.roster(choose.CHAT, "api.1").rows])
        for rid in ids:
            self.assertFalse(component.usable_id(rid),
                             f"{rid!r} is a usable action id, so a provider shipping an "
                             f"action called `chat` could take the keypress")


class AHostileChatRendersAsOneRowAndRunsNothing(PersonaIso, unittest.TestCase):
    """Task 6, step 4, measured against the values that actually reach the surface.

    **There is no `label` file in this stage, so this measures what there IS.** Spec §3.5
    asks for one per chat; nothing here writes one (renaming a chat is not a task in this
    stage) and a reader with no writer is a line no test can turn red. What a chat is
    called today is two strings off disk with two alphabets, and both are exercised:

    * the **id**, which is a DIRECTORY NAME under `.charter/frame/` and therefore whatever
      is on disk. It is held to `chats.ID_RE` where it enters charter's vocabulary, so a
      hostile one is not a chat at all and contributes no row.
    * the **harness**, which is read out of the frame's `identity` file, has an OPEN
      alphabet (it is a harness's own display name) and goes in the row's note. It is
      contained where it is drawn — `overlay.Surface.render`, immediately before
      `tui.width` measures it (#472) — and never anywhere else.

    The hostile values are injected through `state.record_identity`, the production
    writer, so a test that stopped agreeing with it fails here rather than passing against
    its own fixture.
    """

    #: A pane narrow enough that the two-column layout has to squeeze, so the width
    #: arithmetic is exercised rather than trivially satisfied — `test_frame_pickers`'
    #: own size, for the same reason.
    SIZE = (44, 12)

    def setUp(self):
        super().setUp()
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)
        for i, bad in enumerate(HOSTILE, start=1):
            _plant(f"api.{i}", workspace="api", pane=f"%{i}", harness=bad)

    def _drawn(self) -> list[str]:
        from charter.frame import palette
        roster = choose.roster(choose.CHAT, "api.1")
        surface = palette.Palette(catalogue=roster.rows, label=choose.CHAT)
        return surface.render(*self.SIZE)

    def test_every_chat_is_exactly_one_row_however_its_harness_is_spelled(self):
        roster = choose.roster(choose.CHAT, "api.1")
        self.assertEqual(len(roster.rows), len(HOSTILE))
        self.assertEqual(len({r.id for r in roster.rows}), len(HOSTILE),
                         "two chats sharing a row id is one chat that cannot be chosen")

    def test_the_drawn_pane_is_exactly_as_tall_as_the_pane(self):
        drawn = self._drawn()
        self.assertEqual(len(drawn), self.SIZE[1])
        for line in drawn:
            self.assertEqual(line, "".join(line.splitlines()), repr(line))

    def test_no_hostile_byte_reaches_the_pane(self):
        """Asserted per LINE rather than on a join of them, so the separator this test is
        about cannot be one the test itself put there."""
        for line in self._drawn():
            for bad in ("\n", "\r", " ", " ", "", "\x1b[31m"):
                self.assertNotIn(bad, line, repr(line))

    def test_the_column_arithmetic_sees_the_contained_harness_not_the_raw_one(self):
        """#472 exactly. `tui.width` — never `len` — measures what `contain.one_line` has
        already made one line of, so a note holding a separator cannot make a row wider
        than the pane."""
        from charter import tui
        state.record_identity("api.1", {"CHARTER_HARNESS":
                                        "z" * 30 + " " + "y" * 30})
        for line in self._drawn():
            self.assertLessEqual(tui.width(tui.strip_ansi(line)), self.SIZE[0],
                                 repr(line))

    def _switch_argvs(self) -> list[list[str]]:
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", commands_frame.SOCKET)
        fake = _FakeServer()
        with mock.patch.object(tmuxctl, "run", fake), \
             mock.patch.object(commands_frame.tmuxctl, "run", fake), \
             mock.patch.object(tmuxctl, "version", lambda: (3, 7)), \
             mock.patch.object(commands_frame.tmuxctl, "version", lambda: (3, 7)), \
             mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1"}, clear=False):
            commands_frame.cmd_chat(mock.Mock(chat_id="api.2", chat="api.1"))
        self.assertTrue(fake.calls, "nothing was sent, so this measures nothing")
        return fake.calls

    def test_a_harness_name_reaches_a_command_line_only_as_an_e_VALUE(self):
        """**The stronger property, stated as it is actually true — and the spec's
        version of it is not.**

        Task 6 step 4 asks that a display string "never reaches a tmux argv or a tmux
        format string at all". The second half is the one that matters and it holds
        absolutely (see the test below). The FIRST half cannot: `_relayout_pane_env`
        replays what the LAUNCH put on `-e` onto every pane a re-layout splits, and
        `CHARTER_HARNESS` has been in `layout.CARRIABLE` since #411 — so a chat's harness
        name is on the `split-window` command line of every panel that chat ever gets,
        with or without this switch.

        That is not the same risk and this test is what says so. `-e NAME=VALUE` is one
        argv ELEMENT, handed to `execve` and never parsed by tmux, so a newline or an
        escape sequence in it is an odd environment variable and nothing more. What
        §7.3's measurement is about is a display string reaching a place tmux EVALUATES —
        and the value half of a `-e` is not one.

        So the property pinned here is the exact one: a hostile harness name appears in a
        tmux argv only as `-e CHARTER_HARNESS=<it>`, and never as a command word, a
        target, or an option value of any other kind.
        """
        for argv in self._switch_argvs():
            for i, word in enumerate(argv):
                for bad in HOSTILE:
                    if bad not in word:
                        continue
                    self.assertEqual(word, f"CHARTER_HARNESS={bad}",
                                     f"{bad!r} reached a tmux command line as {word!r}")
                    self.assertEqual(argv[i - 1], "-e",
                                     f"{word!r} was not the value half of a `-e`")

    def test_no_display_string_reaches_a_tmux_FORMAT(self):
        """The half that holds absolutely, and the one §7.3 measured the cost of.

        `rename-window 'chat#{pane_pid}X'` stores the name **already expanded** —
        `chat4327X` on 3.7c, `chat49359X` on 3.2 — and `#{E:@opt}` expands a `#{...}`
        sitting in a user option's value. So a display string inside a `#{…}` would be a
        format injection on the first hop. Every `#{…}` this switch sends is a literal
        charter wrote itself.
        """
        for argv in self._switch_argvs():
            for word in argv:
                if "#{" not in word:
                    continue
                for bad in HOSTILE:
                    self.assertNotIn(bad, word,
                                     f"{bad!r} is inside the tmux format {word!r}")
                self.assertIn(word, ("#{pane_id}",
                                     "#{window_width}:#{window_height}"),
                              f"an unexpected tmux format reached the switch: {word!r}")

    def test_every_chat_is_still_reachable_at_200_80_and_40_columns(self):
        """**Stage 5b's first exit criterion**: every chat reachable in ≤2 keystrokes at
        200, 80 and 40 columns — *including* the widths where no bar can be drawn.

        Two keystrokes is `F2` and then either an arrow or a filter; what has to be true
        at every width is that the row is in the surface and still maps back to its own
        chat. Asserted through `Roster.name_of`, which is what
        `commands_frame._chosen_name` calls — matching on the drawn title instead would be
        matching on a string `Surface.render` has already contained, and would pass while
        the switch went to the wrong chat or to none.

        The chat bar draws nothing at all at 40 columns for this many chats, which is the
        point: the palette is a full pane (§4k) and does not care how wide the frame is.
        """
        from charter.frame import overlay, palette
        roster = choose.roster(choose.CHAT, "api.1")
        self.assertEqual(len(roster.rows), len(HOSTILE))
        for cols in (200, 80, 40):
            with self.subTest(cols=cols):
                surface = palette.Palette(catalogue=roster.rows, label=choose.CHAT)
                drawn = surface.render(cols, 20)
                self.assertEqual(len(drawn), 20)
                for row in roster.rows:
                    self.assertIsNotNone(roster.name_of(row))
                # And typing narrows to exactly one of them, which is the second keystroke.
                surface.handle(overlay.Event(overlay.KEY, "2"), 20)
                self.assertEqual([r.id for r in surface.rows], ["chat:n1"],
                                 f"typing `2` at {cols} columns did not reach api.2")
                self.assertEqual(roster.name_of(surface.rows[0]), "api.2")
        # And the width at which the BAR gives up its names is not a width at which the
        # picker gives up anything — which is the whole of "the bar is a readout and the
        # palette is the mechanism".
        from charter import tui
        narrow = slots.chats_bar("api.1", 40)
        self.assertTrue(all("api.8" not in ln for ln in narrow),
                        f"the bar still listed every chat at 40 columns: {narrow!r}")
        for ln in narrow:
            self.assertLessEqual(tui.width(ln), 40)

    def test_a_chat_id_outside_the_alphabet_is_refused_before_it_becomes_an_argv(self):
        """"Runs nothing" is the other half, and it is asked of the ID rather than of the
        note: the id is the only one of the two that a switch ever puts on a command
        line."""
        for bad in HOSTILE:
            out = chats.check("api.1", bad)
            self.assertFalse(out.ok, f"{bad!r} was accepted as a chat")
            self.assertIn("cannot name a chat", out.message)
            self.assertEqual(out.message, "".join(out.message.splitlines()),
                             repr(out.message))


class _FakeServer:
    """A tmux that records what it was asked and answers what a test told it to.

    Everything `cmd_chat` sends goes through `tmuxctl.run`, so one patch is the whole
    server. `_relayout` and `_split_panels` go through it too, which is what makes the
    ORDER of the four steps assertable in one list.
    """

    def __init__(self, *, select_rc: int = 0):
        self.calls: list[list[str]] = []
        self.select_rc = select_rc

    def __call__(self, what, argv, **kw):
        import subprocess
        self.calls.append(list(argv))
        rc, out = 0, ""
        verb = self._verb(argv)
        if verb == "select-window":
            rc = self.select_rc
        elif verb == "split-window":
            out = "%9"
        elif verb == "display-message":
            out = "80:24"
        return subprocess.CompletedProcess(argv, rc, out, "")

    @staticmethod
    def _verb(argv) -> str:
        """The tmux command word — the first element after the socket flags.

        Spelled by scanning rather than by index, because `tmuxctl.server_argv` puts a
        different number of elements in front of it for `-L` and `-S`.
        """
        for i, word in enumerate(argv):
            if word in ("-L", "-S", "-f"):
                continue
            if i and argv[i - 1] in ("-L", "-S", "-f"):
                continue
            if word.startswith("-") or word.endswith("tmux"):
                continue
            return word
        return ""

    def verbs(self) -> list[str]:
        return [self._verb(a) for a in self.calls]


class TheSwitchIsFourStepsInOneOrder(PersonaIso, unittest.TestCase):
    """Task 4, against a fake server: what `cmd_chat` sends, and in which order.

    The ORDER is the requirement. `select-window` has to come first because tmux resizes
    the target window at the switch and never before it (measured on 3.7c and on 3.2), so
    a split issued earlier is a pane born at a width that is not its window's — the
    defect `panel._component_text`'s `width=slots._width()` guard exists for. And the
    teardown has to come after the select for the same measurement read the other way:
    the window being left is the one that is now stale.
    """

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api", pane="%1")
        _plant("api.2", workspace="api", pane="%2")
        state.record_server("api.1", commands_frame.SOCKET)
        state.record_server("api.2", commands_frame.SOCKET)
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4"})
        self.fake = _FakeServer()
        self._patches = [
            mock.patch.object(tmuxctl, "run", self.fake),
            mock.patch.object(commands_frame.tmuxctl, "run", self.fake),
            mock.patch.object(tmuxctl, "version", lambda: (3, 7)),
            mock.patch.object(commands_frame.tmuxctl, "version", lambda: (3, 7)),
            mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1",
                                         "CHARTER_WORKSPACE": "api"}, clear=False),
        ]
        for p in self._patches:
            p.start()
            self.addCleanup(p.stop)

    def _switch(self, target="api.2"):
        return commands_frame.cmd_chat(
            mock.Mock(chat_id=target, chat="api.1"))

    def test_the_client_is_moved_before_anything_is_torn_down_or_split(self):
        self._switch()
        verbs = self.fake.verbs()
        self.assertIn("select-window", verbs)
        first = verbs.index("select-window")
        for later in ("kill-pane", "split-window"):
            if later in verbs:
                self.assertLess(first, verbs.index(later),
                                f"{later} was issued before the window had been "
                                f"selected, so it acted on stale geometry")

    def test_the_window_is_named_by_the_targets_own_harness_pane(self):
        self._switch()
        sel = next(a for a in self.fake.calls
                   if _FakeServer._verb(a) == "select-window")
        self.assertEqual(sel[-2:], ["-t", "%2"])

    def test_the_chat_being_left_loses_its_panels(self):
        self._switch()
        killed = [a[-1] for a in self.fake.calls
                  if _FakeServer._verb(a) == "kill-pane"]
        self.assertEqual(sorted(killed), ["%3", "%4"])
        self.assertEqual(state.panes("api.1"), {})

    def test_the_chat_being_entered_gets_panels_and_a_bump(self):
        before = state.version("api.2")
        self._switch()
        self.assertIn("split-window", self.fake.verbs())
        self.assertTrue(state.panes("api.2"))
        self.assertNotEqual(state.version("api.2"), before)

    def test_a_window_that_is_gone_costs_a_sentence_and_no_panels(self):
        """The one refusal `chats.check` deliberately does not guess at. Nothing is torn
        down: the teardown is below the select for exactly this case."""
        self.fake.select_rc = 1
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self._switch()
        self.assertEqual(len(said), 1)
        self.assertIn("has no window any more", said[0])
        self.assertNotIn("kill-pane", self.fake.verbs())
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_outside_a_frame_it_touches_no_tmux_at_all(self):
        """**Found by hand against the real server, not by a test.** `_say_on_screen`'s
        `-t <fid>` with an empty target resolves to whichever session on the SHARED
        `-L charter` server was attached most recently — so `charter frame-chat api.2`
        typed in an ordinary shell drew charter's refusal across another operator's frame.

        Unlike `cmd_toggle`'s deleted `if not fid`, this refusal is not free: that command
        emits nothing with an empty id and its guard was an equivalent mutant, and this
        one issues a real tmux command aimed at a session it has no business naming.
        """
        said = []
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": ""}, clear=False), \
             mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self.assertEqual(
                commands_frame.cmd_chat(mock.Mock(chat_id="api.2", chat="")), 0)
        self.assertEqual(self.fake.calls, [])
        self.assertEqual(said, [], "a refusal was aimed at a frame this is not in")

    def test_a_refused_name_never_reaches_tmux_at_all(self):
        said = []
        with mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)):
            self._switch("api.9")
        self.assertEqual(self.fake.calls, [])
        self.assertIn("no chat 'api.9' here", said[0])

    def test_the_switch_lays_out_the_same_with_the_window_resized_hook_removed(self):
        """Task 4, step 5, and the measurement behind it: on tmux 3.2 —
        `tmuxctl.FLOOR` — `set-hook -w window-resized` answers `invalid option`, rc=1.
        The hook does not exist there.

        **Charter does still install it above `tmuxctl.RESIZE_HOOK_FLOOR`, and that is
        not what this is about.** `_relayout` arms it for the window's own later resizes,
        which is a different event from the switch. What the switch may not do is *rely*
        on it to correct geometry it did not assert itself — so the property is that the
        panels come out identical with the hook and without it, which is what a 3.2
        operator gets.
        """
        def panes_at(version):
            state.record_panes("api.2", panels={})
            with mock.patch.object(tmuxctl, "version", lambda: version), \
                 mock.patch.object(commands_frame.tmuxctl, "version",
                                   lambda: version):
                self.fake.calls.clear()
                self._switch()
            hooked = any("window-resized" in " ".join(a) for a in self.fake.calls)
            splits = [a for a in self.fake.calls
                      if _FakeServer._verb(a) == "split-window"]
            return sorted(state.panes("api.2")), len(splits), hooked

        at_37 = panes_at((3, 7))
        # Put the panels back where the 3.7 pass found them, so the second pass starts
        # from the same frame rather than from the one the first left behind.
        state.record_panes("api.1", panels={"top": "%3", "bottom": "%4"})
        at_32 = panes_at((3, 2))
        self.assertEqual(at_37[:2], at_32[:2],
                         "the switch laid the target out differently without the hook")
        self.assertTrue(at_37[2], "3.7 should still arm the hook for later resizes")
        self.assertFalse(at_32[2],
                         "the hook does not exist at the floor and must not be sent")

    def test_the_target_gets_its_own_visible_arrangement_and_not_this_chats(self):
        """`_visible_now(target, …)` — the chat being entered draws what IT was told to
        hide, not what the chat being left was."""
        frame = config.FRAME
        arrangement = instance.frame_arrangement(frame)
        state.record_hidden("api.2", [n for n in arrangement if n != "top"])
        self._switch()
        self.assertEqual(sorted(state.panes("api.2")), ["top"])

    def test_a_chat_charter_cannot_relayout_still_gets_the_client_moved(self):
        """The two re-layouts are attempted independently, and the client's move is not
        conditional on either. A chat charter has no usable pane record for cannot have
        its panels moved — but abandoning the switch over it would leave the operator on
        the window they asked to leave, which is the worse of the two failures."""
        state.record_harness_pane("api.1", "not-a-pane")
        self._switch()
        verbs = self.fake.verbs()
        self.assertIn("select-window", verbs)
        self.assertNotIn("kill-pane", verbs)
        self.assertTrue(state.panes("api.2"),
                        "the target lost its panels because the chat being left could "
                        "not be tidied")

    def test_a_tmux_whose_version_charter_cannot_read_moves_the_client_and_no_panes(self):
        """`_relayout_target` refuses on an unreadable version because every builder
        below it takes one. The switch itself needs none — `select-window` has been in
        tmux forever — so the client still moves and nothing is split blind."""
        with mock.patch.object(tmuxctl, "version", lambda: None), \
             mock.patch.object(commands_frame.tmuxctl, "version", lambda: None):
            self._switch()
        verbs = self.fake.verbs()
        self.assertEqual(verbs, ["select-window"])
        self.assertEqual(state.panes("api.1"), {"top": "%3", "bottom": "%4"})

    def test_the_presser_chat_is_the_one_left_not_the_session_variable(self):
        """One bind text is shared by every frame on the socket, so `$CHARTER_SESSION_ID`
        cannot tell two chats of one workspace apart — `--chat` can, and it is what
        decides whose panels are torn down."""
        state.record_panes("api.2", panels={"top": "%7"})
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.2"}, clear=False):
            commands_frame.cmd_chat(mock.Mock(chat_id="api.1", chat="api.2"))
        killed = [a[-1] for a in self.fake.calls
                  if _FakeServer._verb(a) == "kill-pane"]
        self.assertEqual(killed, ["%7"])


class ThePaletteStartsTheSwitchRatherThanPerformingIt(PersonaIso, unittest.TestCase):
    """§4g: a row returns having started. The palette's pane is killed the instant it has
    invoked, and `kill-pane` hands SIGHUP to that pane's process group — so a switch that
    ran in the palette's own process would be racing its own teardown for three of its
    four tmux calls."""

    def setUp(self):
        super().setUp()
        _plant("api.1", workspace="api", pane="%1")
        _plant("api.2", workspace="api", pane="%2")
        self._env = mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "api"},
                                    clear=False)
        self._env.start()
        self.addCleanup(self._env.stop)

    def test_the_palette_spawns_frame_chat_detached_with_the_target_on_the_argv(self):
        spawned = []
        with mock.patch.object(commands_frame.builtin_actions, "_spawn",
                               lambda argv, *, fid: spawned.append((argv, fid))):
            commands_frame._start_chat_switch("api.1", "api.2")
        self.assertEqual(len(spawned), 1)
        argv, fid = spawned[0]
        self.assertEqual(argv[-2:], ["frame-chat", "api.2"])
        self.assertEqual(fid, "api.1")

    def test_choose_switch_to_decides_and_does_not_perform(self):
        """`choose.switch_to` is asked in the palette's own process, which may make no
        tmux call — so for `chat` it returns the check and `_draw_palette` starts the
        command. The other three nouns still perform."""
        with mock.patch.object(tmuxctl, "run") as ran:
            out = choose.switch_to(choose.CHAT, "api.1", "api.2")
        self.assertTrue(out.ok)
        ran.assert_not_called()
