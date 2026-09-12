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
import shutil
import stat
import subprocess
import threading
import unicodedata
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import (commands_frame, commands_handoff, config, statusline, tui,
                     workspace)
from charter.frame import (builtin_actions, builtins, choose, chrome, picker,
                           notify, slots, state, switch)

from tests._isolation import PersonaIso, wired_as_today
from tests.test_a_chat_opens_in_the_background_with_its_first_message import (
    TheLaunchOpensWithoutMovingAnyone)
from tests.test_a_handoff_refuses_before_it_changes_anything import _AHandoffFromAlpha
from tests.test_a_workspace_tab_opens_what_it_names import _a_chat, _OpensBeta, _Server


#: Ruling 10, and the reason is inherited with the fixture: `ALaunchThatAttachesClearsTheMark`
#: re-runs `TheLaunchOpensWithoutMovingAnyone`'s real `_launch`, whose own module stands in
#: `wiring.refusal` — but a module fixture belongs to the module that runs it, so here the
#: suite's `claude` guard read every one of those launches as "could not tell" and refused
#: it. No test in this module is about wiring.
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()

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
        # **`statusline.accent("ok")` is a COLLABORATOR here, not the code under test**,
        # which is the line between an indirection that is fine and the one that pins
        # nothing: `slots._compose` is what these cases measure, and which SGR this plane
        # draws `ok` in is `[frame] accents` — an operator's choice, so spelling the escape
        # would pin a theme rather than the property. What the indirection could hide is an
        # accent that is EMPTY, where every `assertIn` below passes on the empty string, so
        # that one case is closed here and by the literal beside it.
        self.assertNotEqual(statusline.accent("ok"), "",
                            "this plane draws no ok accent, so nothing below is a test")
        self.assertEqual(statusline.accent("ok"), "\x1b[32m",
                         "the shipped `ok` accent moved; these rows are written for it")

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
        self.assertEqual(line, "  *alpha  ✶beta")

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
        self.assertEqual(chrome.plain(row), "  *beta.1  + -")
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

    def test_a_hostile_name_in_the_record_reaches_no_row(self):
        """The record is charter's own private state, and it is still read with a name
        check — the name goes on to a strip. Both ways round: the record answers only the
        name that can be one, and the raw erase never reaches a row."""
        _a_chat("f1", ws="alpha", pane="%1")
        d = workspace._arrivals_dir()
        d.mkdir(parents=True, exist_ok=True)
        (d / "beta").write_text("")
        (d / "\x1b[2Jgamma").write_text("")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        row = slots.workspaces_bar("f1", 200)[0]
        self.assertNotIn("\x1b[2J", row)
        self.assertIn(f"{statusline.accent('ok')}{ARRIVED}beta", row)

    def test_an_arrival_wins_the_mark_cell_from_a_spinner(self):
        """The two can only meet if a caller hands both — no strip does — and the docstring
        says which wins. Pinned so the order is a decision rather than the way the `if`s
        happen to be stacked. The spinner frame is fixed here because it is a clock
        reading, and `_ARRIVED_MARK` is itself one of its three glyphs."""
        with mock.patch("charter.frame.slots.tab_spinner_frame", return_value="✢"):
            line = slots._compose(["alpha", "beta"], "alpha", 200,
                                  arrived=frozenset({"beta"}),
                                  busy=("beta",))[0][0]
        self.assertIn(f"{statusline.accent('ok')}{ARRIVED}beta", line)
        self.assertNotIn("✢beta", line)


class TheArrivalsRecord(PersonaIso, unittest.TestCase):
    """Where the marks live and what a reader of them is promised."""

    def test_it_is_private_plane_state_beside_the_tab_order(self):
        old = os.umask(0)
        self.addCleanup(os.umask, old)
        workspace.record_arrival("beta")
        d = workspace._arrivals_dir()
        self.assertTrue((d / "beta").is_file())
        self.assertEqual(d, Path(config.STATE_DIR) / "workspace-arrivals")
        self.assertEqual(d.parent, Path(config.STATE_DIR),
                         "beside `workspace-tab-order`, which lives here too")
        mode = stat.S_IMODE((d / "beta").stat().st_mode)
        self.assertEqual(mode & 0o077, 0, f"the mark came out {mode:04o}")
        self.assertEqual(mode, config.STATE_FILE_MODE, f"the mark came out {mode:04o}")
        dmode = stat.S_IMODE(d.stat().st_mode)
        self.assertEqual(dmode & 0o077, 0, f"the record came out {dmode:04o}")
        self.assertNotIn(state._root(), d.parents)

    def test_the_mark_is_the_files_existence_and_carries_nothing_else(self):
        """ADR 0011 said in bytes: nothing reads WHEN a workspace arrived, so there is
        nowhere for a field nothing reads to go."""
        workspace.record_arrival("beta")
        self.assertEqual((workspace._arrivals_dir() / "beta").read_bytes(), b"")

    def test_a_name_is_recorded_once(self):
        workspace.record_arrival("beta")
        workspace.record_arrival("beta")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        self.assertEqual(sorted(q.name for q in
                                (Path(config.STATE_DIR) / "workspace-arrivals").iterdir()),
                         ["beta"])

    def test_a_name_that_cannot_be_a_workspace_is_never_joined_onto_a_path(self):
        """The name check moved to the WRITE when the record grew a path per name. `..`
        under `STATE_DIR` is #442 arriving through a record instead of a listing."""
        workspace.record_arrival("../escaped")
        self.assertEqual(workspace.arrivals(), frozenset())
        self.assertFalse((Path(config.STATE_DIR).parent / "escaped").exists())
        self.assertFalse(workspace._arrivals_dir().exists())

    def test_clearing_a_name_that_cannot_be_a_workspace_deletes_nothing(self):
        """**The guard with teeth, and it is the clear side.** `record_arrival`'s identical
        check only ever fails to create a file; this one decides what `unlink` is aimed at,
        and `../workspace-tab-order` under the arrivals directory is the plane's tab order.
        Unreachable today — every caller name-checks first — which is exactly why it is
        pinned here rather than left to the day one stops."""
        workspace.record_tab_order(["alpha", "beta"])
        workspace.record_arrival("beta")
        order = workspace._tab_order_file()
        before = order.read_bytes()
        for escape in ("../workspace-tab-order", "..", "../..", "/etc/passwd", ""):
            with self.subTest(escape=escape):
                workspace.clear_arrival(escape)
                self.assertTrue(order.is_file(), f"{escape!r} took the tab order")
                self.assertEqual(order.read_bytes(), before)
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))

    def test_a_mark_is_reported_when_it_could_not_be_written(self):
        """A handoff that opened a chat nobody will be pointed at is the one outcome this
        record exists to prevent, so a failed write answers `False` rather than going
        quiet. The name-refused case answers `False` on the same terms: it marked nothing."""
        self.assertTrue(workspace.record_arrival("beta"))
        self.assertFalse(workspace.record_arrival("../escaped"))
        with mock.patch("charter.config.touch_for", side_effect=OSError):
            self.assertFalse(workspace.record_arrival("gamma"))
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))

    def test_a_workspace_with_a_very_long_name_can_still_be_marked(self):
        """**The ceiling `replace_for` put here, measured.** An atomic replace writes a
        temp beside the target, so a name within a few characters of `NAME_MAX` made a temp
        name past it — `ENAMETOOLONG`, swallowed, and a workspace that could be created and
        never marked. A marker's content is its existence; there is no half-written empty
        file to protect a reader from, so it is created directly and the ceiling is gone."""
        name = "w" * 240
        self.assertTrue(workspace.valid_name(name), "the plane would refuse this name")
        self.assertTrue(workspace.record_arrival(name))
        self.assertEqual(workspace.arrivals(), frozenset({name}))
        workspace.clear_arrival(name)
        self.assertEqual(workspace.arrivals(), frozenset())

    def test_a_name_that_cannot_name_a_workspace_is_not_read_back(self):
        d = workspace._arrivals_dir()
        d.mkdir(parents=True, exist_ok=True)
        (d / "beta").write_text("")
        (d / ".DS_Store").write_text("")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))

    def test_a_record_that_cannot_be_listed_is_no_marks(self):
        """The fixture has a real mark in it, so the two branches answer differently: with
        the guard `frozenset()`, without it a traceback out of a panel's render."""
        workspace.record_arrival("beta")
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        with mock.patch("os.listdir", side_effect=OSError):
            self.assertEqual(workspace.arrivals(), frozenset())

    def test_a_plane_that_has_never_had_a_handoff_has_no_record_to_read(self):
        """The ordinary answer, taken without a mock: the directory is simply not there."""
        self.assertFalse(workspace._arrivals_dir().exists())
        self.assertEqual(workspace.arrivals(), frozenset())

    def test_clearing_a_name_that_never_arrived_writes_nothing(self):
        """Every switch on this plane calls this. It must touch neither the record's own
        directory nor another workspace's mark — asserted on the mtimes, because identical
        bytes are exactly what a needless rewrite produces."""
        workspace.record_arrival("gamma")
        d = workspace._arrivals_dir()
        before = (d.stat().st_mtime_ns, (d / "gamma").stat().st_mtime_ns)
        workspace.clear_arrival("beta")
        self.assertEqual((d.stat().st_mtime_ns, (d / "gamma").stat().st_mtime_ns), before)
        self.assertEqual(workspace.arrivals(), frozenset({"gamma"}))

    def test_clearing_on_a_plane_with_no_record_creates_none(self):
        """The other half: a plane that has never had a handoff must not grow a record the
        first time somebody switches workspace."""
        config.private_mkdir(Path(config.STATE_DIR))
        workspace.clear_arrival("beta")
        self.assertFalse(workspace._arrivals_dir().exists())

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


class TwoWritersNeverLoseAMark(PersonaIso, unittest.TestCase):
    """**Real threads on real files, and the reason the record is a directory.**

    The single-file shape was a read-modify-write over the whole set, and the plane runs
    many charter processes at once: a handoff writing `gamma` read the set before a
    concurrent handoff wrote `beta` and then wrote its own read back over it. Measured, all
    three interleavings lost or resurrected a mark — and a lost mark is exactly the
    invisibility this whole task exists to end.

    Each case runs :data:`ROUNDS` times with a `threading.Barrier`, because a race is
    pinned by repetition rather than by one lucky schedule: one round of the old shape
    passed perhaps half the time, and this many rounds of it did not.
    """

    #: Enough rounds that the old read-modify-write loses on one of them with near
    #: certainty, and few enough that the three cases together cost well under a second.
    ROUNDS = 60

    def _race(self, first, second):
        """Run *first* and *second* on two real threads released together, and answer what
        the record then holds. Exceptions are carried out rather than swallowed — a writer
        that raised inside a thread would otherwise read as a passing round."""
        raised: list = []

        def run(fn):
            gate.wait()
            try:
                fn()
            except BaseException as e:  # noqa: BLE001 - re-raised on the main thread
                raised.append(e)

        gate = threading.Barrier(2)
        threads = [threading.Thread(target=run, args=(fn,)) for fn in (first, second)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(10)
        self.assertEqual([t for t in threads if t.is_alive()], [])
        if raised:
            raise raised[0]
        return workspace.arrivals()

    def test_two_handoffs_landing_together_keep_both_marks(self):
        for _ in range(self.ROUNDS):
            workspace.forget_arrivals()
            got = self._race(lambda: workspace.record_arrival("beta"),
                             lambda: workspace.record_arrival("gamma"))
            self.assertEqual(got, frozenset({"beta", "gamma"}))

    def test_a_handoff_landing_while_a_switch_clears_does_not_bring_it_back(self):
        for _ in range(self.ROUNDS):
            workspace.forget_arrivals()
            workspace.record_arrival("beta")
            got = self._race(lambda: workspace.record_arrival("gamma"),
                             lambda: workspace.clear_arrival("beta"))
            self.assertEqual(got, frozenset({"gamma"}))

    def test_a_switch_clearing_while_a_handoff_lands_does_not_lose_it(self):
        """The same two operations with the threads started the other way round. Not a
        duplicate: which thread reads first is what decided the old shape's outcome, and
        the two orders lost different marks."""
        for _ in range(self.ROUNDS):
            workspace.forget_arrivals()
            workspace.record_arrival("beta")
            got = self._race(lambda: workspace.clear_arrival("beta"),
                             lambda: workspace.record_arrival("gamma"))
            self.assertEqual(got, frozenset({"gamma"}))


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

    def test_pressing_the_tab_you_are_already_on_clears_its_mark(self):
        """The one state where the mark could never be paid. A frame in `beta` draws
        `*beta` — the block takes the mark's cell, so that operator never sees the mark —
        and `to_workspace` refuses the switch before `_switch_client` runs. They are the
        person looking at it, so the look counts."""
        args = SimpleNamespace(workspace="beta", persona=None)
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "beta.1"}, clear=True):
            self.assertEqual(commands_frame.cmd_switch(args), 0)
        self.assertEqual(workspace.arrivals(), frozenset())
        self.said.assert_called_once()
        self.assertEqual(self.said.call_args.args[1], "already in workspace 'beta'")

    def test_every_route_to_the_workspace_you_are_in_clears_it(self):
        """The docs say the mark clears "however you get there", and every workspace-switch
        surface on this plane ends at `cmd_switch` — a tab press, a palette row, the
        keyboard walk and a typed command all spawn `frame-switch --workspace <name>`. So
        the sentence is true for all four exactly if it is true for this one entry point,
        and this drives it with the argv each of them builds.

        **The list of routes lives in TWO places and this is the other half.** The prose
        one is `docs/frame.md`'s *"however you get there"* sentence (and its copies in
        `docs/handoff.md` and the news entry); a route added to one belongs in both. The
        argv assertion below is what keeps the claim honest as the code moves — a surface
        given a spelling of its own stops reaching this clear, and nothing else in the
        suite would notice that the sentence had silently become false for that route.
        What it cannot do is notice a route added to the code and not to the prose, which
        is why this paragraph is here rather than left to be re-derived.
        """
        # **Every surface's argv, held equal to the one this case then drives.** The three
        # spellings are deliberately separate in the source — `frame/builtins.py` is a
        # renderer a palette process has no reason to import, and `_start_workspace_switch`
        # restates the pair rather than reaching for it — so the equality lives here, which
        # is `tests/test_the_keyboard_walks_the_tab_strips.py`'s own arrangement. A surface
        # given a spelling of its own stops reaching the clear below, and nothing else in
        # the suite would notice that the docs had become false for that route.
        spawned: list = []
        with mock.patch("charter.frame.builtin_actions._spawn",
                        side_effect=lambda argv, **kw: spawned.append(argv)):
            commands_frame._start_workspace_switch("beta.1", "beta")
        self.assertEqual(spawned[0][-2:], ["--workspace", "beta"],
                         "the palette starts a switch nothing else spells")
        self.assertEqual(builtin_actions._STRIPS[1].command,
                         ("frame-switch", "--workspace"))
        self.assertEqual(builtins._WORKSPACE_SWITCH, ("frame-switch", "--workspace"))
        for surface in ("tab", "palette row", "keyboard walk", "typed command"):
            with self.subTest(surface=surface):
                workspace.record_arrival("beta")
                args = SimpleNamespace(workspace="beta", persona=None)
                with mock.patch.dict(os.environ,
                                     {"CHARTER_SESSION_ID": "beta.1"}, clear=True):
                    self.assertEqual(commands_frame.cmd_switch(args), 0)
                self.assertEqual(workspace.arrivals(), frozenset())

    def test_pressing_a_tab_that_is_refused_for_any_other_reason_keeps_the_mark(self):
        """`to_workspace` refuses three things and only one of them means "you are here".

        The refusal has to name the MARKED workspace or the case proves nothing: a clear
        for some other name is a no-op whether it is guarded or not. A workspace deleted
        while its mark stands is the one state where a plane can refuse `beta` by name and
        still hold `beta`'s mark — and that mark must survive, because nobody looked."""
        shutil.rmtree(config.WORKSPACES_DIR / "beta")
        args = SimpleNamespace(workspace="beta", persona=None)
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "alpha.1"}, clear=True):
            self.assertEqual(commands_frame.cmd_switch(args), 0)
        self.assertEqual(workspace.arrivals(), frozenset({"beta"}))
        self.assertIn("no workspace 'beta'", self.said.call_args.args[1])

    def test_a_frame_root_that_cannot_be_listed_costs_the_switch_nothing(self):
        """`bump_everywhere` runs on the switch path, so a repaint nobody asked for must
        never be the thing that raises out of one. The sweep reported this catch unpinned:
        without it the failure comes out of `_looked_at`, through `_switch_client`, and the
        operator's switch dies for a notification."""
        with mock.patch("charter.frame.state._root", side_effect=OSError):
            self.assertIsNone(notify.bump_everywhere())
            # And through the one function every clear site calls, which is where the
            # failure would actually escape from: the mark is still paid, and the switch
            # that paid it does not die because nobody could be told.
            commands_frame._looked_at("beta")
        self.assertEqual(workspace.arrivals(), frozenset())

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


class EverySurfaceThatListsWorkspacesSaysSo(PersonaIso, unittest.TestCase):
    """**The strip is not the only place a workspace is named, and it is the one place with
    no room.** At 24 columns an arrival can be behind a `+2` and at 12 the strip is `3/5`,
    so the surfaces an operator reaches when the strip has run out of room have to carry the
    fact too — and those have a note column and a whole terminal to draw in, so they say it
    in words rather than in a glyph."""

    def setUp(self):
        super().setUp()
        for n in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / n).mkdir(parents=True, exist_ok=True)
        _a_chat("alpha.1", ws="alpha", pane="%1")
        workspace.record_arrival("beta")

    def _notes(self, rows):
        return {r.title: r.note for r in rows}

    def test_the_palettes_workspace_picker_names_the_arrival(self):
        notes = self._notes(choose.roster(choose.WORKSPACE, "alpha.1").rows)
        self.assertEqual(notes["beta"], "handoff arrived")
        self.assertEqual(notes["gamma"], "")
        self.assertEqual(notes["alpha"], "")

    def test_the_palettes_typed_rows_keep_the_kind_and_the_arrival(self):
        """A top-level row's note is the KIND, which tells `zeb` the persona from `zeb-api`
        the workspace and cannot go. The arrival is why the operator is typing that name,
        so it is kept beside it rather than displacing it or being displaced."""
        roster = choose.roster(choose.WORKSPACE, "alpha.1")
        notes = self._notes(choose.labelled(roster))
        self.assertEqual(notes["beta"], "workspace · handoff arrived")
        self.assertEqual(notes["gamma"], "workspace")

    def test_a_reason_still_displaces_both(self):
        """A row that cannot run has one thing to say and it is why."""
        roster = choose.roster(choose.WORKSPACE, "alpha.1")
        rows = choose.labelled(roster, "pinned by $CHARTER_WORKSPACE")
        self.assertEqual(self._notes(rows)["beta"], "pinned by $CHARTER_WORKSPACE")
        self.assertTrue(all(r.refused for r in rows))

    def test_only_the_workspace_picker_asks_the_plane_for_arrivals(self):
        """The cost guard the sweep reported unpinned. Reading it for every noun would
        change no output — a chat row's note is its harness and a persona row has none — so
        nothing but this can see the difference: one `os.listdir` per picker open, on a
        surface an operator reaches by keystroke, asking a question that noun cannot use."""
        with mock.patch("charter.workspace.arrivals") as asked:
            for noun in (choose.CHAT, choose.PERSONA, choose.CHANGE):
                with self.subTest(noun=noun):
                    choose.roster(noun, "alpha.1")
                    asked.assert_not_called()
            choose.roster(choose.WORKSPACE, "alpha.1")
            self.assertEqual(asked.call_count, 1, "and exactly once for the whole roster")

    def test_a_picker_charter_cannot_ask_draws_no_note_and_raises_nothing(self):
        with mock.patch("charter.workspace.arrivals", side_effect=RuntimeError):
            notes = self._notes(choose.roster(choose.WORKSPACE, "alpha.1").rows)
        self.assertEqual(set(notes.values()), {""})

    def test_the_chats_picker_still_says_which_harness(self):
        """The workspace branch is new and must not have eaten the one note that was
        already there."""
        notes = self._notes(choose.roster(choose.CHAT, "alpha.1").rows)
        self.assertEqual(notes["alpha.1"], "claude-code")

    def test_the_launch_picker_names_the_arrival_beside_the_clone_count(self):
        """The earliest surface that can say it: before tmux, before a frame, before any
        strip exists to draw a mark on."""
        rows = picker.rows(["alpha", "beta"], lambda n: 0, workspace.arrivals())
        drawn = [tui.strip_ansi(ln) for ln in picker.render(rows, "alpha", 120).splitlines()]
        self.assertIn("     1  * alpha  —", drawn)
        self.assertIn("     2    beta   —  handoff arrived", drawn)

    def test_the_launch_asks_the_plane_for_its_arrivals(self):
        """The wiring, not the renderer: a picker handed a renderer that can draw the mark
        and an empty set would look right in every test of the renderer alone."""
        seen: list = []

        def ask(rows_, current, **kw):
            seen.append(rows_)
            return picker.Choice(picker.CANCEL)

        with mock.patch.dict(os.environ, {}, clear=True), \
                mock.patch("sys.stdin.isatty", return_value=True), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("charter.frame.picker.ask", side_effect=ask):
            commands_frame._choose_workspace(
                SimpleNamespace(workspace=None, pick=True))
        self.assertEqual([r.name for r in seen[0] if r.arrived], ["beta"])

    def test_a_narrow_terminal_cuts_the_note_and_never_drops_it(self):
        """`docs/frame.md` says cut, never dropped, and this is the measurement behind that
        wording — it said "at any width" until the widths were asked. What the operator
        keeps at 24 columns is a row that visibly carries something the others do not,
        which is more than the strip has at that width."""
        rows = picker.rows(["alpha", "beta"], lambda n: 0, frozenset({"beta"}))
        for width, drawn in ((120, "     2    beta   —  handoff arrived"),
                             (30, "     2    beta   —  handoff a…"),
                             (24, "     2    beta   —  han…"),
                             (16, "     2    beta …")):
            with self.subTest(width=width):
                # Split BEFORE stripping: `tui.strip_ansi` is asked about one row, and the
                # render is several joined by newlines.
                row = [tui.strip_ansi(ln)
                       for ln in picker.render(rows, "alpha", width).splitlines()
                       if "beta" in ln][0]
                self.assertEqual(row, drawn)
                self.assertLessEqual(tui.width(row), width)

    def test_the_launch_picker_without_arrivals_draws_exactly_what_it_drew_before(self):
        rows = picker.rows(["alpha", "beta"], lambda n: 0)
        drawn = [tui.strip_ansi(ln) for ln in picker.render(rows, "alpha", 120).splitlines()]
        self.assertIn("     2    beta   —", drawn)
        self.assertEqual([ln for ln in drawn if "handoff" in ln], [])


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

    def test_a_created_workspace_is_on_the_strip_and_at_the_front_of_it(self):
        """`bring_to_front` moves nothing for a name the plane does not have, so a
        `--create`d workspace has to be a workspace by the time it is asked — otherwise the
        one handoff that most needs pointing at is the one that points at nothing."""
        self.opened = commands_frame.Opened(True, "gamma.1", "")
        self._handoff(ws="gamma", create=True, vision="Ship the thing.")
        self.assertEqual(workspace.tab_order()[0], "gamma")
        self.assertIn("gamma", switch.workspaces())
        self.assertIn("gamma", workspace.arrivals())

    def test_the_calling_chats_attention_row_names_the_new_chat(self):
        self._handoff()
        self.assertEqual(state.notice("alpha.1"),
                         "charter: handoff → beta.1 opened in workspace 'beta'")

    def test_every_frame_is_told_once(self):
        self._handoff()
        self.assertEqual(self.fanout.call_count, 1)

    def test_a_mark_that_could_not_be_written_is_said_and_the_chat_still_opened(self):
        """The one outcome this record exists to prevent, arriving through the record: a
        chat is open and nothing on screen will point at it. So it is said — naming the
        workspace, because going to look is the job the mark would have done — and nothing
        after it is conditional on the mark, because the chat is real either way."""
        with mock.patch("charter.workspace.record_arrival", return_value=False):
            rc, out, err = self._handoff()
        self.assertEqual(rc, 0)
        # **Spelled out, not `UNMARKED.format(...)`.** An expectation computed from the
        # template under test moves with it: removing `{ws}` — the very thing this case
        # exists to confirm — leaves both sides equal and the suite green, and so does
        # inverting the sentence. The words are the property here, so the words are
        # written down. The `! ` is `util.warn`'s marker and belongs in the same string:
        # a handoff that opened a chat did not fail, so this is not a `✗`.
        self.assertEqual(
            err.strip(),
            "! charter handoff: beta.1 is open in 'beta', but charter could not mark "
            "that workspace's tab as arrived — its state directory refused the write. "
            "Nothing on the strip will point at the new chat; its tab did move to the "
            "front.")
        self.assertIn("opened chat beta.1 in workspace 'beta'", out)
        self.assertEqual(workspace.tab_order()[0], "beta")
        self.assertEqual(state.notice("alpha.1"),
                         "charter: handoff → beta.1 opened in workspace 'beta'")
        self.assertEqual(self.fanout.call_count, 1)

    def test_a_mark_that_was_written_says_nothing_extra(self):
        """The other half, so the sentence above is a report and not decoration."""
        rc, _out, err = self._handoff()
        self.assertEqual((rc, err), (0, ""))

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
