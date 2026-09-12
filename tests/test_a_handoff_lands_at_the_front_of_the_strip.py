"""A handoff moves the workspace it landed in to the front of the strip, and marks it.

This is the half of `charter handoff` the operator *sees*. A handoff opens a chat in the
background — no client moves, nothing attaches — so without this the work is invisible
until somebody happens to look at the right tab, which `docs/frame.md` calls silence by
construction.

Three things, and each is pinned here on its own terms:

* **The arrival move.** `switch.bring_to_front` is the ONE thing that moves a tab while a
  plane is up. Everything #767 and #923 bought — a tab never moves under a press, and every
  frame on the plane draws the same columns — has to survive it, so the move goes through
  the plane's own recorded order rather than around it.
* **The arrived mark.** The target tab is drawn in the `ok` accent AND carries
  `slots._ARRIVED_MARK` in the strip's reserved mark cell (the plan's ruling on Open
  question 13). The accent is what reads at a glance; the glyph is what survives
  `NO_COLOR`, where `panel._write` strips every escape off every row. Neither may move a
  cell or a column — `TheAccentAndTheGlyphMoveNoColumn` is that measurement, and
  `statusline._persona_chips` records two glyphs that shipped broken here for want of it.
* **The clear.** The mark is paid by a LOOK, plane-wide, at the three places a terminal
  comes to be looking at a workspace: a switch that tmux confirmed, a focus, and a launch
  that attaches. Not by the handoff, not by a refused switch, and not per tmux client — a
  pane draws the same bytes for every client of its session.
"""

from __future__ import annotations

import os
import stat
import subprocess
import unicodedata
import unittest
from unittest import mock

from charter import commands_frame, config, statusline, tui, workspace
from charter.frame import chrome, slots, state, switch

from tests._isolation import PersonaIso
from tests.test_a_chat_opens_in_the_background_with_its_first_message import (
    TheLaunchOpensWithoutMovingAnyone)
from tests.test_a_handoff_refuses_before_it_changes_anything import _AHandoffFromAlpha
from tests.test_a_workspace_tab_opens_what_it_names import _a_chat, _OpensBeta, _Server

#: The glyph the strip draws in the mark cell for a workspace a handoff landed in, spelled
#: by hand rather than read off `slots._ARRIVED_MARK`. A test that took the constant from
#: the module under test would follow it anywhere it went, and where it can go — an
#: East-Asian *Ambiguous* character a terminal may draw two cells wide — is exactly the
#: change this file exists to catch.
ARRIVED = "✶"


class TheArrivalMovesOneTab(PersonaIso, unittest.TestCase):
    """`switch.bring_to_front`: the target goes to the head of the plane's recorded order,
    and the order goes on being one order."""

    def setUp(self):
        super().setUp()
        for n in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def test_the_target_moves_to_the_front_of_the_recorded_order(self):
        workspace.record_tab_order(["alpha", "beta", "gamma", config.DEFAULT_WORKSPACE])
        switch.bring_to_front("gamma")
        self.assertEqual(workspace.tab_order(),
                         ["gamma", "alpha", "beta", config.DEFAULT_WORKSPACE])

    def test_a_plane_with_no_recorded_order_decides_one_then_moves_the_target(self):
        """A handoff can be the first thing that ever asks this plane for its order. The
        move must not write a one-name file and leave the rest to be decided later, by a
        repaint, against a record that already exists."""
        self.assertEqual(workspace.tab_order(), [])
        switch.bring_to_front("gamma")
        self.assertEqual(workspace.tab_order()[0], "gamma")
        self.assertEqual(set(workspace.tab_order()), set(switch.workspaces()))

    def test_a_name_the_plane_does_not_have_moves_nothing(self):
        workspace.record_tab_order(["alpha", "beta"])
        before = workspace._tab_order_file().read_bytes()
        switch.bring_to_front("nope")
        self.assertEqual(workspace._tab_order_file().read_bytes(), before)

    def test_the_moved_order_then_holds_still(self):
        """#923's whole property, re-asked on the far side of the one thing that moves a
        tab: the move happens once, and the next two paints draw what it left."""
        switch.bring_to_front("gamma")
        moved = workspace.tab_order()
        self.assertEqual(switch.workspaces(), moved)
        self.assertEqual(switch.workspaces(), moved)

    def test_the_strip_draws_the_moved_order(self):
        _a_chat("f1", ws="alpha", pane="%1")
        switch.bring_to_front("gamma")
        row = tui.strip_ansi(slots.workspaces_bar("f1", 200)[0])
        self.assertLess(row.index("gamma"), row.index("alpha"))


class TheArrivedTabIsDrawnInTheOkAccent(PersonaIso, unittest.TestCase):
    """The mark itself: an accent around the field and a glyph in the mark cell."""

    def setUp(self):
        super().setUp()
        for n in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)
        self.assertNotEqual(statusline.accent("ok"), "",
                            "this plane draws no ok accent, so nothing below is a test")

    def _line(self, names, here, arrived=frozenset(), width=200):
        return slots._compose(names, here, width, arrived=arrived)[0][0]

    def test_an_arrived_tab_is_wrapped_in_the_ok_accent(self):
        line = self._line(["alpha", "beta"], "alpha", frozenset({"beta"}))
        self.assertIn(f"{statusline.accent('ok')}{ARRIVED}beta{statusline._R}", line)

    def test_the_arrived_mark_is_one_cell_and_no_terminal_disagrees(self):
        """The ruling's second half: pin the width. `tui.width` reads the East-Asian
        tables, and an *Ambiguous* glyph is one a terminal may draw two cells wide while
        those tables say one — the failure `slots._BAR_RULE` is ASCII to avoid and the one
        `statusline._persona_chips` records breaking this layout twice. Neutral is the
        strongest property available short of ASCII, and it is the property the three
        `TAB_SPINNER` glyphs were chosen for."""
        self.assertEqual(slots._ARRIVED_MARK, ARRIVED)
        self.assertEqual(tui.width(slots._ARRIVED_MARK), 1)
        self.assertEqual(tui.width(slots._BAR_MARK[1]), 1)
        self.assertEqual(unicodedata.east_asian_width(slots._ARRIVED_MARK), "N")
        self.assertIn(slots._ARRIVED_MARK, slots.TAB_SPINNER)

    def test_the_glyph_is_what_survives_no_colour(self):
        """`panel._write` hands every row to `chrome.plain` under `NO_COLOR`. What is left
        of an arrived tab there is the glyph, and the tab you are on still has its `*`."""
        line = chrome.plain(self._line(["alpha", "beta"], "alpha", frozenset({"beta"})))
        self.assertEqual(line, f"{slots._inset()}*alpha  {ARRIVED}beta")

    def test_the_accent_and_the_glyph_move_no_column(self):
        """The alignment measurement, at three widths and two row counts. The arrival
        changes what a field SAYS and never what it measures, so the cut, every row's width
        and the click map are identical with and without it."""
        names = ["alpha", "beta", "gamma", "delta", "epsilon"]
        for width in (200, 60, 24):
            for rows in (1, 3):
                with self.subTest(width=width, rows=rows):
                    plain = slots._compose(names, "alpha", width, rows=rows)
                    marked = slots._compose(names, "alpha", width, rows=rows,
                                            arrived=frozenset({"epsilon"}))
                    self.assertEqual([tui.width(ln) for ln in marked[0]],
                                     [tui.width(ln) for ln in plain[0]])
                    self.assertEqual(marked[1], plain[1])

    def test_the_tab_you_are_on_keeps_its_own_highlight_when_it_arrived(self):
        """You cannot be owed a look at the workspace you are looking at, so the block
        wins the field outright — the mark cell keeps its `*` and no accent is written."""
        line = self._line(["alpha", "beta"], "alpha", frozenset({"alpha"}))
        self.assertIn(chrome.block("*alpha"), line)
        self.assertNotIn(f"{statusline.accent('ok')}*alpha", line)

    def test_only_the_arrived_tab_carries_the_accent(self):
        line = self._line(["alpha", "beta", "gamma"], "alpha", frozenset({"beta"}))
        self.assertEqual(line.count(statusline.accent("ok")), 1)

    def test_the_workspaces_bar_reads_the_planes_arrivals(self):
        _a_chat("f1", ws="alpha", pane="%1")
        workspace.record_arrival("beta")
        row = slots.workspaces_bar("f1", 200)[0]
        self.assertIn(f"{statusline.accent('ok')}{ARRIVED}beta", row)

    def test_the_chats_bar_draws_no_arrival(self):
        """A handoff lands in a workspace and never in a chat, so the chats strip passes
        no arrivals — a record naming something chat-shaped reaches nothing."""
        _a_chat("beta.1", ws="beta", pane="%1")
        workspace.record_arrival("beta.1")
        row = "".join(slots.chats_bar("beta.1", 200))
        self.assertEqual(chrome.plain(row), f"{slots._inset()}*beta.1  {slots.ADD_CHAT} "
                                            f"{slots.CLOSE_CHAT}")
        self.assertEqual(row.count(statusline.accent("ok")), 0)

    def test_an_unreadable_arrivals_record_draws_a_plain_strip(self):
        """A readout must never cost a pane. The degrade is the strip charter drew before
        there were any arrivals at all."""
        _a_chat("f1", ws="alpha", pane="%1")
        with mock.patch("charter.workspace.arrivals", side_effect=RuntimeError):
            row = slots.workspaces_bar("f1", 200)[0]
        self.assertEqual(chrome.plain(row), "  *alpha 1    beta      default      gamma")
        self.assertEqual(row.count(statusline.accent("ok")), 0)

    def test_the_sizer_asks_for_the_same_rows_with_arrivals(self):
        """`bar_rows_wanted` runs in the LAUNCHER, where nothing knows or should know which
        workspace a chat was handed to. A mark that changed the row count would resize a
        pane every time a handoff landed."""
        _a_chat("f1", ws="alpha", pane="%1")
        before = slots.bar_rows_wanted("f1", "workspaces", pane_cols=40, cap=3)
        workspace.record_arrival("beta")
        self.assertEqual(slots.bar_rows_wanted("f1", "workspaces", pane_cols=40, cap=3),
                         before)

    def test_a_hostile_line_in_the_record_reaches_no_row(self):
        """The record is charter's own private state, and it is still read with a name
        check — the line goes on to a strip. Both ways round: the record answers only the
        name that can be one, and the raw erase never reaches a row."""
        _a_chat("f1", ws="alpha", pane="%1")
        workspace._arrivals_file().parent.mkdir(parents=True, exist_ok=True)
        workspace._arrivals_file().write_text("beta\n\x1b[2Jgamma\n")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        row = slots.workspaces_bar("f1", 200)[0]
        self.assertNotIn("\x1b[2J", row)
        self.assertIn(f"{statusline.accent('ok')}{ARRIVED}beta", row)


class TheArrivalsRecord(PersonaIso, unittest.TestCase):
    """Where the marks live and what a reader of them is promised."""

    def test_it_is_private_plane_state_beside_the_tab_order(self):
        old = os.umask(0)
        self.addCleanup(os.umask, old)
        workspace.record_arrival("beta")
        f = workspace._arrivals_file()
        self.assertTrue(f.is_file())
        self.assertEqual(f.parent, workspace._tab_order_file().parent)
        mode = stat.S_IMODE(f.stat().st_mode)
        self.assertEqual(mode & 0o077, 0, f"the record came out {mode:04o}")
        self.assertEqual(mode, config.STATE_FILE_MODE, f"the record came out {mode:04o}")
        self.assertNotIn(state._root(), f.parents)

    def test_a_name_is_recorded_once(self):
        workspace.record_arrival("beta")
        workspace.record_arrival("beta")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        self.assertEqual(workspace._arrivals_file().read_text(), "beta\n")

    def test_a_line_that_cannot_name_a_workspace_is_not_read_back(self):
        workspace._arrivals_file().parent.mkdir(parents=True, exist_ok=True)
        workspace._arrivals_file().write_text("beta\n../x\n\n")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))

    def test_a_record_that_cannot_be_read_is_no_marks(self):
        with mock.patch("pathlib.Path.read_text", side_effect=OSError):
            self.assertEqual(workspace.arrivals(), frozenset())

    def test_clearing_a_name_that_never_arrived_writes_nothing(self):
        """Every switch on this plane calls this, and the ordinary plane HAS a record —
        `config.replace_for` would rewrite it whole, moving its mtime on every switch
        charter ever makes for no change at all. Asserted on the mtime as well as the
        bytes, because identical bytes are exactly what a needless rewrite produces."""
        workspace.record_arrival("gamma")
        f = workspace._arrivals_file()
        before = (f.read_bytes(), f.stat().st_mtime_ns)
        workspace.clear_arrival("beta")
        self.assertEqual((f.read_bytes(), f.stat().st_mtime_ns), before)

    def test_clearing_on_a_plane_with_no_record_creates_none(self):
        """The other half: a plane that has never had a handoff must not grow an empty
        record the first time somebody switches workspace."""
        config.private_mkdir(workspace._arrivals_file().parent)
        workspace.clear_arrival("beta")
        self.assertFalse(workspace._arrivals_file().exists())

    def test_clearing_removes_only_that_name(self):
        workspace.record_arrival("beta")
        workspace.record_arrival("gamma")
        workspace.clear_arrival("beta")
        self.assertEqual(workspace.arrivals(), frozenset({"gamma"}))

    def test_forgetting_drops_every_mark(self):
        workspace.record_arrival("beta")
        workspace.forget_arrivals()
        self.assertEqual(workspace.arrivals(), frozenset())
        workspace.forget_arrivals()


class TheMarkClearsWhenSomeoneLooks(_OpensBeta):
    """A switch, a focus and an attaching launch each pay the look — and a switch that did
    not move the terminal does not."""

    def setUp(self):
        super().setUp()
        _a_chat("beta.1", ws="beta", pane="%9")
        workspace.record_arrival("beta")

    def _server(self):
        server = _Server()
        server.opened = ["%9"]
        return server

    def test_switching_into_the_workspace_clears_its_mark(self):
        server = self._server()
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server):
            commands_frame._switch_client("alpha.1", "beta", said="workspace → beta")
        self.assertNotIn("beta", workspace.arrivals())

    def test_a_switch_that_did_not_move_the_terminal_keeps_the_mark(self):
        """`_switch_client` refuses for several reasons and only the CONFIRMED move is a
        look. A mark cleared by a switch tmux then refused loses the only thing pointing at
        the chat a handoff opened."""
        server = self._server()
        server.switched.append(("/dev/ttys001", "$2"))

        def refuse(cmd, **kw):
            if "list-clients" in cmd and cmd[cmd.index("-t") + 1] == "$2":
                return subprocess.CompletedProcess(list(cmd), 0, "", "")
            return server(cmd, **kw)

        with mock.patch("charter.commands_frame.subprocess.run", side_effect=refuse):
            commands_frame._switch_client("alpha.1", "beta", said="workspace → beta")
        self.assertIn("beta", workspace.arrivals())

    def test_clearing_bumps_every_frame(self):
        """Every frame on the plane draws the mark, so every frame has to be told it is
        gone — a panel polls `state.version` and would otherwise keep drawing it."""
        before = {f: state.version(f) for f in ("alpha.1", "beta.1")}
        server = self._server()
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server):
            commands_frame._switch_client("alpha.1", "beta", said="workspace → beta")
        for f, was in before.items():
            self.assertNotEqual(state.version(f), was, f)

    def test_a_switch_into_an_unmarked_workspace_tells_nobody(self):
        """Almost no switch follows a handoff. A fan-out on every one of them would bump
        every frame directory on the plane — and repaint every background frame's panels —
        to say that nothing changed."""
        workspace.clear_arrival("beta")
        before = {f: state.version(f) for f in ("alpha.1", "beta.1")}
        server = self._server()
        with mock.patch("charter.commands_frame.subprocess.run", side_effect=server), \
                mock.patch("charter.frame.notify.bump_everywhere") as fanout:
            commands_frame._switch_client("alpha.1", "beta", said="workspace → beta")
        fanout.assert_not_called()
        self.assertEqual(state.version("beta.1"), before["beta.1"])

    def test_a_focus_clears_the_mark_before_it_attaches(self):
        """`interact` IS the operator's terminal for as long as they stay, so a clear on
        the far side of it would drop the mark as they left rather than as they arrived."""
        seen = []

        def attaching(argv, **kw):
            seen.append(workspace.arrivals())
            return subprocess.CompletedProcess(argv, 0, "", "")

        with mock.patch.object(commands_frame.tmuxctl, "interact",
                               side_effect=attaching) as interact:
            commands_frame._focus_workspace("$2", "beta.1", ws="beta", picked=False)
        interact.assert_called_once()
        self.assertEqual(seen, [frozenset()])


class ALaunchThatAttachesClearsTheMark(TheLaunchOpensWithoutMovingAnyone):
    """Task 1's real `_launch`, with the arrival already recorded for `beta`.

    Task 1's own cases re-run here with a mark on the workspace being launched into, and
    that is the point of inheriting rather than copying the fixture: none of them may change
    because a handoff marked a workspace. The launcher is the one place where the clear sits
    inside the branch that decides whether this process becomes anybody's terminal.
    """

    def setUp(self):
        super().setUp()
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        workspace.record_arrival("beta")

    def test_a_launch_that_attaches_clears_the_mark_before_attaching(self):
        """The reading is taken from inside the clear, because the fixture owns the stand-in
        for `tmuxctl.interact` and an attach that has not happened is one that has not been
        called. Zero is the whole assertion: the mark was paid as the terminal arrived, not
        as it left."""
        real, seen = workspace.clear_arrival, []

        def clearing(ws):
            seen.append((ws, commands_frame.tmuxctl.interact.call_count))
            real(ws)

        with mock.patch.object(workspace, "clear_arrival", side_effect=clearing):
            self._launch(attach=None)
        self.assertEqual(seen, [("beta", 0)])
        self.assertEqual(workspace.arrivals(), frozenset())
        self.interact.assert_called_once()

    def test_a_background_open_looks_at_nothing(self):
        """A handoff's own launch never attaches, so the chat it opens cannot clear the
        mark it is the reason for."""
        self._launch(opening=commands_frame.Opening("fix it please"))
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))


class APlaneThatGoesColdForgetsItsArrivals(PersonaIso, unittest.TestCase):
    """`state.reap`'s cold-plane branch, one file over from the tab order: a mark is a look
    somebody is owed, and a plane with no frame has no strip to owe it on."""

    def setUp(self):
        super().setUp()
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        workspace.record_arrival("beta")

    def test_a_plane_that_goes_cold_forgets_its_arrivals(self):
        _a_chat("alpha.1", ws="alpha", pane="%1")
        self.assertEqual(state.reap(set(), server=commands_frame.SOCKET), ["alpha.1"])
        self.assertEqual(workspace.arrivals(), frozenset())

    def test_a_reap_that_leaves_a_frame_keeps_the_arrivals(self):
        """The other half, and the one #767 is about: a frame still on screen must not lose
        a mark because a SIBLING ended."""
        _a_chat("alpha.1", ws="alpha", pane="%1")
        state.reap({"alpha.1"}, server=commands_frame.SOCKET)
        self.assertIn("beta", workspace.arrivals())


class AHandoffArrives(_AHandoffFromAlpha):
    """Steps 7 and 8 of `charter handoff`, on the far side of the open."""

    def setUp(self):
        super().setUp()
        self.fanout = self.enterContext(
            mock.patch("charter.frame.notify.plane_changed_everywhere"))

    def test_a_handoff_elsewhere_moves_the_target_to_the_front_and_marks_it(self):
        self._handoff()
        self.assertEqual(workspace.tab_order()[0], "beta")
        self.assertIn("beta", workspace.arrivals())

    def test_a_handoff_into_this_workspace_moves_it_and_marks_nothing(self):
        """You are looking at it, and its chats strip already shows the new tab. The tab
        still moves: the order is about where work is."""
        self._handoff(ws="alpha")
        self.assertEqual(workspace.tab_order()[0], "alpha")
        self.assertEqual(workspace.arrivals(), frozenset())

    def test_the_calling_chats_attention_row_names_the_new_chat(self):
        self._handoff()
        self.assertEqual(state.notice("alpha.1"),
                         "charter: handoff → beta.1 opened in workspace 'beta'")

    def test_every_frame_is_told_once(self):
        self._handoff()
        self.assertEqual(self.fanout.call_count, 1)

    def test_a_handoff_that_did_not_open_moves_no_tab_and_marks_nothing(self):
        """Nothing on screen may point at a chat that does not exist."""
        workspace.record_tab_order(["alpha", "beta"])
        self.opened = commands_frame.Opened(False, "", "tmux said no")
        self._handoff()
        self.assertEqual(workspace.tab_order(), ["alpha", "beta"])
        self.assertEqual(workspace.arrivals(), frozenset())
        self.assertEqual(state.notice("alpha.1"), "")
        self.fanout.assert_not_called()


if __name__ == "__main__":
    unittest.main()
