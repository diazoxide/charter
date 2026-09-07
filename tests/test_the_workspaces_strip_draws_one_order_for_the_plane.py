"""#923: the workspaces strip draws one order, and it is the plane's.

*"when switching workspaces tabs — after each switch — order of workspaces tabs changing,
feeling that each switch is opening new window."*

**A regression from #903, and the mechanism it broke was the right one.**
`switch._by_use` freezes the order the first time it is asked and never re-decides, which
is exactly what `slots._cuts` and `chats.of_workspace` both demanded with a measurement.
The defect was the KEY. #903 stored the frozen order under `.charter/frame/<fid>/`, which
`frame.state`'s opening line says is per frame and never global — and a chat is a frame.
**Switching workspaces switches chats**, so every switch drew a strip ordered by a
different chat's snapshot of the recency, taken at a different moment. Measured on the
reporting plane: three chats, three orders, and two of them (`default.1`, `default.2`) in
the same workspace and still disagreeing.

The strip draws a plane-wide list. `TheStripIsOneListSoItHasOneOrder` is the reproduction
— frames in different workspaces, painting at different moments, drawing identical
columns — and it fails on #903's code in the way the report describes.

**Three decisions this file pins beyond the fix itself.**

* *Where it lives.* `workspace._tab_order_file()`, beside `active-workspace` and
  `sessions/` — plane-scoped, per developer, gitignored, private-mode, and not inside a
  tree `state.reap` deletes per frame. `TheOrderIsPlaneStateAndNotFrameState`.
* *When it is re-decided.* Once per plane launch: `state.reap` drops it when the reap
  leaves this plane with no frame state at all, so the order is still while an operator is
  looking at it and fresh when they come back. `APlaneThatGoesColdDecidesAgain`.
* *That the per-chat file is GONE rather than kept beside it.* Two records of one order
  are two orders. `TheOrderIsPlaneStateAndNotFrameState` asserts the frame writer no
  longer exists and that no frame directory grows the file.

**The three readings of the strip are checked together** (#880's rule), because anything
that changes which name is at which column re-enters `_cuts`, `bar_rows_wanted` and
`_tab_columns` at once: `TheThreeReadingsOfTheStripAgree` asks all three of two frames in
different workspaces and holds #767's rule — a wider bar never drops more than the one
name `_cuts` already admits to, and a tab never moves under a press.
"""

from __future__ import annotations

import os
import stat
import unittest
from pathlib import Path
from unittest import mock

from charter import config, tui, workspace
from charter.frame import slots, state, switch

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant

#: Far enough apart that no filesystem's mtime granularity can round two together.
DAY = 86400.0

#: The plane the report was measured on, chats and all: two chats in one workspace is the
#: shape that made the defect undeniable, because nothing about a workspace can explain
#: two of its own chats disagreeing.
PLANE = {"default.1": "default", "default.2": "default",
         "harness-wrapper.1": "harness-wrapper",
         "authority-audit.1": "authority-audit", "autonomy.1": "autonomy"}


def _touched(fid: str, when: float) -> None:
    """Make *fid*'s chat directory look as though charter last wrote to it at *when* —
    which is what `chats.touched_by_workspace` measures the recency with."""
    os.utime(state.frame_dir(fid), (when, when))


class _Plane(PersonaIso):
    """The reporting plane, planted: five chats over four workspaces, timed a day apart."""

    WIDTH = 200

    def setUp(self):
        super().setUp()
        for ws in set(PLANE.values()):
            (config.WORKSPACES_DIR / ws).mkdir(parents=True, exist_ok=True)
        for fid, ws in PLANE.items():
            _plant(fid, workspace=ws)
        self.age = {"autonomy.1": DAY, "authority-audit.1": DAY * 2,
                    "harness-wrapper.1": DAY * 3, "default.2": DAY * 3.5,
                    "default.1": DAY * 4}
        self._restate()
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def _restate(self) -> None:
        """Put every chat's mtime back where `self.age` says it is.

        Restated before each ask rather than set once, because recording the order is
        itself a write and moves the very number the next reader measures. The operator's
        plane has that property too; a fixture that did not would be measuring a quieter
        world than the one the report came from.

        A chat a reap has taken is skipped rather than recreated — a case about a plane
        going cold must not have its fixture quietly plant the frames back.
        """
        for fid, at in self.age.items():
            if state.frame_dir(fid).is_dir():
                _touched(fid, at)

    def arrive(self, fid: str) -> None:
        """The half of a switch this feature can see: arriving in a chat writes to it."""
        self.age[fid] = max(self.age.values()) + DAY
        self._restate()

    def strip(self, fid: str) -> list[str]:
        """The names frame *fid* draws, left to right, as an operator reads them."""
        self._restate()
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}, clear=False):
            rows = slots.workspaces_bar(fid, self.WIDTH)
        row = tui.strip_ansi("".join(rows))
        return sorted((n for n in switch.workspaces() if n in row), key=row.index)


class TheStripIsOneListSoItHasOneOrder(_Plane, unittest.TestCase):
    """**The reproduction.** Every case here draws the strip of two or three DIFFERENT
    frames, with the recency moved between them the way a real switch moves it, and
    asserts they came out identical. On #903's code each frame recorded its own order at
    its own moment and these are the orders the report lists."""

    def test_two_frames_in_different_workspaces_draw_the_same_order(self):
        """The gesture in the report, at its smallest: paint one frame, switch, paint the
        next. The switch really does move the measurement — the second assertion is what
        stops this passing on a plane that never changed."""
        first = self.strip("default.1")
        self.arrive("harness-wrapper.1")
        self.assertEqual(self.strip("harness-wrapper.1"), first,
                         "the frame arrived in drew a different strip")
        self.assertEqual(switch.current_workspace("harness-wrapper.1"), "harness-wrapper",
                         "the two frames are not in different workspaces")

    def test_two_chats_of_one_workspace_draw_the_same_order(self):
        """`default.1` and `default.2` are the pair that makes the scope unarguable: they
        are in one workspace, so nothing about which workspace a frame is in can account
        for their disagreeing. On #903's code they did."""
        first = self.strip("default.1")
        self.arrive("authority-audit.1")
        self.arrive("default.2")
        self.assertEqual(self.strip("default.2"), first)

    def test_the_three_chats_of_the_report_draw_one_order_between_them(self):
        """The report's own table, which held three different orders for one plane-wide
        list. Asked as a set of what was drawn, so the failure names how many orders the
        plane had rather than which one was wrong."""
        drawn = {"default.1": self.strip("default.1")}
        self.arrive("harness-wrapper.1")
        drawn["harness-wrapper.1"] = self.strip("harness-wrapper.1")
        self.arrive("authority-audit.1")
        self.arrive("default.2")
        drawn["default.2"] = self.strip("default.2")
        orders = {tuple(v) for v in drawn.values()}
        self.assertEqual(len(orders), 1,
                         f"{len(drawn)} chats drew {len(orders)} orders: {drawn}")

    def test_the_one_order_is_still_the_working_set_first(self):
        """The fix is about the SCOPE of the freeze and not about the ordering rule, so
        #903's own answer has to survive it: the plane leads with what it was last in."""
        self.assertEqual(self.strip("default.1")[:2], ["default", "harness-wrapper"])

    def test_a_frame_that_paints_first_decides_for_the_plane_and_not_only_for_itself(self):
        """Which frame asks first is a race between panel processes, and the answer must
        not depend on who won. Whichever asks, the record is the plane's and the next
        frame reads it rather than measuring again."""
        self.strip("harness-wrapper.1")
        recorded = workspace.tab_order()
        self.arrive("autonomy.1")
        self.assertEqual(self.strip("default.1"), recorded[:len(self.strip("default.1"))])
        self.assertEqual(workspace.tab_order(), recorded,
                         "the second frame to paint rewrote the plane's order")


class TheOrderIsPlaneStateAndNotFrameState(_Plane, unittest.TestCase):
    """Where the record lives, and that there is exactly one of it."""

    def test_it_is_written_to_the_planes_own_state_file(self):
        self.strip("default.1")
        self.assertTrue(workspace._tab_order_file().is_file(),
                        "the plane recorded no order")
        self.assertEqual(workspace._tab_order_file().parent, config.STATE_DIR,
                         "the plane's order is not beside the plane's other pointers")

    def test_the_path_is_the_one_charter_tells_operators_it_is(self):
        """**Spelled out, because the spelling is published.** `docs/news` shows an
        operator `.charter/workspace-tab-order` in the before-and-after this issue is
        about, and `docs/frame.md` describes what lives there. A path in a release note is
        a promise like any other name charter prints, and one the reader and the writer
        would go on agreeing about after a rename — both go through the same function, so
        nothing else in this file could tell.
        """
        self.assertEqual(workspace._tab_order_file(),
                         Path(config.STATE_DIR) / "workspace-tab-order")

    def test_no_frame_directory_grows_a_tab_order_file(self):
        """**The per-chat file is removed rather than left beside the plane-wide one.**
        Two records of one order are two orders, and the second one is the defect. Asked of
        every frame directory rather than of one name, so a writer that moved to another
        file inside the frame's tree is caught too."""
        self.strip("default.1")
        self.strip("harness-wrapper.1")
        left = sorted(p.name for d in state._root().iterdir() if d.is_dir()
                      for p in d.iterdir())
        self.assertNotIn("tab_order", left, f"a frame still holds an order: {left}")

    def test_the_per_frame_writer_and_reader_are_gone_from_frame_state(self):
        """`frame.state` is what one frame knows about ITSELF (its opening line), and an
        order the whole plane draws was never that. Named rather than merely unused, so a
        caller cannot quietly grow back."""
        self.assertFalse(hasattr(state, "record_tab_order"),
                         "the per-chat writer is still reachable")
        self.assertFalse(hasattr(state, "tab_order"),
                         "the per-chat reader is still reachable")

    def test_the_record_is_charters_own_and_no_other_accounts(self):
        """#894's rule, where this write lands: everything under `STATE_DIR` goes out
        through `config.write_for`/`replace_for`, so the mode is charter's decision and not
        the umask's.

        **Under a real `umask 000`**, which is what makes the claim measurable — the mode
        `write_for` names and the mode an ordinary `write_text` produces are the same 0600
        under nobody's umask, and differ by every bit under this one. A bare `write_text`
        here comes out 0666.

        The directory is asked about too: `record_tab_order` may be the writer that creates
        `.charter/` itself on a first launch, and a state directory any account can list is
        exactly the exposure #470 was filed for.
        """
        old = os.umask(0o000)
        self.addCleanup(os.umask, old)
        workspace.record_tab_order(["default"])
        mode = stat.S_IMODE(workspace._tab_order_file().stat().st_mode)
        self.assertEqual(mode & 0o077, 0, f"the record came out {mode:04o}")
        self.assertEqual(mode, config.STATE_FILE_MODE, f"the record came out {mode:04o}")
        d = stat.S_IMODE(Path(config.STATE_DIR).stat().st_mode)
        self.assertEqual(d & 0o077, 0, f"the state directory came out {d:04o}")

    def test_the_frame_id_is_no_longer_part_of_the_question(self):
        """`switch.workspaces` takes no frame, which is the fix stated in the signature:
        the ORDER is the plane's and only the MARK is the frame's. A parameter still
        accepted would be one a caller could go on keying an order to."""
        with self.assertRaises(TypeError):
            switch.workspaces("default.1")


class APlaneThatGoesColdDecidesAgain(_Plane, unittest.TestCase):
    """**When the order expires**, which is the second question #923 asks and the one with
    two wrong answers. Never re-deciding ossifies — a workspace made next month sorts last
    for good. Re-deciding on a repaint is the live reordering `slots._cuts` measured and
    refused. Once per plane launch is what is left, and `state.reap` is where charter sees
    a plane go cold."""

    def _reap(self, live=(), server="charter") -> None:
        state.reap(set(live), server=server)

    def test_a_reap_that_leaves_no_frame_forgets_the_order(self):
        self.strip("default.1")
        self.assertTrue(workspace._tab_order_file().is_file())
        self._reap()
        self.assertEqual(state.reap(set(), server="charter"), [],
                         "the plane still holds frame state, so it never went cold")
        self.assertFalse(workspace._tab_order_file().exists(),
                         "the order outlived every frame that could be drawing it")

    def test_the_next_launch_then_leads_with_what_the_plane_did_last(self):
        """The point of expiring it: a plane that has moved on comes back ordered by where
        the operator actually was, rather than by where they were the first time they ever
        opened it."""
        first = self.strip("default.1")
        self.assertEqual(first[0], "default")
        self._reap()
        for fid, ws in PLANE.items():
            _plant(fid, workspace=ws)
        self.age = dict(self.age, **{"autonomy.1": DAY * 40})
        self.assertEqual(self.strip("autonomy.1")[0], "autonomy",
                         "the plane came back in the order it had last month")

    def test_a_reap_that_leaves_one_frame_keeps_the_order(self):
        """The other half, and the one #767 is about: a frame still on screen must not have
        its columns rearranged because a SIBLING ended."""
        first = self.strip("default.1")
        self.arrive("harness-wrapper.1")
        self._reap(live={"default.1"})
        self.assertTrue(workspace._tab_order_file().is_file(),
                        "a live frame's order was forgotten under it")
        self.assertEqual(self.strip("default.1"), first)

    def test_a_frame_on_the_other_tmux_server_keeps_it_too(self):
        """`reap` is scoped to one server because "not live" is only an answer a frame's
        own server can give — so a frame in the operator's own tmux is kept by that rule
        and is an operator looking at a strip. The plane is cold only when NOTHING is left,
        which is why the test is on the directory rather than on this reap's own list."""
        self.strip("default.1")
        for fid in PLANE:
            state.record_server(fid, "other-socket")
        self._reap(server="charter")
        self.assertTrue(workspace._tab_order_file().is_file(),
                        "the other server's frames were treated as gone")

    def test_a_claim_that_is_not_a_frame_yet_leaves_the_plane_warm(self):
        """`reap` keeps a directory for four independent reasons and any one of them is a
        plane that is still up. The narrowest is #685's: a claim `new_chat_id` has made and
        whose marker has not landed, which is a frame about to be born rather than one that
        has gone."""
        self.strip("default.1")
        (state._root() / "later.1").mkdir()
        self._reap()
        self.assertTrue(workspace._tab_order_file().is_file(),
                        "a plane with a chat being claimed was read as cold")

    def test_a_stray_file_in_the_frame_root_is_not_a_frame(self):
        """The count is of DIRECTORIES, and a file there is neither a frame that was
        reaped nor a frame that is still up. Left in the count it would be a directory
        `reap` can never take — every keep-rule abstains on a file — so the plane would
        read as warm for good and the order would never be decided again."""
        self.strip("default.1")
        (state._root() / "stray").write_text("not a frame\n")
        self._reap()
        self.assertFalse(workspace._tab_order_file().exists(),
                         "a file in the frame root kept the plane warm for ever")

    def test_forgetting_an_order_that_was_never_recorded_is_not_an_error(self):
        """A plane that has never drawn a strip reaps like any other, and a reap runs on a
        launch path where a raise costs the launch."""
        self.assertFalse(workspace._tab_order_file().exists())
        workspace.forget_tab_order()
        self._reap()


class TheThreeReadingsOfTheStripAgree(_Plane, unittest.TestCase):
    """#880's rule: which name is at which column is read by `_cuts`, by
    `bar_rows_wanted` and by `_tab_columns`, and a change that moves one moves all three.
    Asked of two frames in different workspaces, which is the axis #923 changed."""

    #: Fifteen names is the list every measurement in `slots._cuts` and `test_frame_bars`
    #: is stated against, and the size the reporting plane is near.
    NAMES = [f"workspace-{i:02d}" for i in range(15)]

    def setUp(self):
        super().setUp()
        for name in self.NAMES:
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)

    def test_two_frames_want_the_same_number_of_rows(self):
        """The sizing pass runs in the LAUNCHER and in the `frame-resize` child, neither of
        which is the panel that draws. Two frames asking for different heights is a strip
        planned for one shape and drawn in another."""
        self.strip("default.1")
        self.arrive("harness-wrapper.1")
        wanted = [slots.bar_rows_wanted(fid, "workspaces", pane_cols=90, cap=3)
                  for fid in ("default.1", "harness-wrapper.1")]
        self.assertEqual(wanted[0], wanted[1], "the two frames sized the strip differently")
        self.assertGreater(wanted[0], 1, "this width does not overflow, so it measures "
                                         "nothing about the cut")

    def test_a_tab_is_at_the_same_column_for_both_frames(self):
        """*"a tab must never move under a press"* (#767) across the one gesture that
        used to move every one of them. The published map is what a click resolves
        against, so it is asked rather than the drawn row."""
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}, clear=False):
            slots.workspaces_bar("default.1", 160, rows=2)
            before = {(r, c): slots.TABS.tab_at(r, c)
                      for r in range(2) for c in range(160)}
            self.arrive("harness-wrapper.1")
            self._restate()
            slots.workspaces_bar("harness-wrapper.1", 160, rows=2)
            after = {(r, c): slots.TABS.tab_at(r, c)
                     for r in range(2) for c in range(160)}
        moved = {k: (before[k], after[k]) for k in before if before[k] != after[k]}
        self.assertEqual(moved, {}, f"a tab moved under the switch: {moved}")

    def test_a_wider_bar_never_drops_more_than_the_one_name_the_cut_admits_to(self):
        """#767's rule on the strip this issue changes, over the whole range `_cuts`'
        own measurements are stated across. The cut is deliberately not monotone — its
        docstring says so and `test_frame_bars` pins the count — but no step may cost more
        than a single name, and a plane-wide order must not have made it worse."""
        self.strip("default.1")
        # Every name the plane has and not only `NAMES`, because a count over a subset can
        # read two names leaving at once as a drop of two when one of them was replaced by
        # a name the subset does not hold. The property is about the ROW.
        names = switch.workspaces()
        drops, previous = [], None
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}, clear=False):
            for width in range(60, 281):
                row = tui.strip_ansi(slots.workspaces_bar("default.1", width)[0])
                count = len([n for n in names if n in row])
                if previous is not None and count < previous:
                    drops.append((width, previous - count))
                previous = count
        self.assertEqual([step for _w, step in drops], [1] * len(drops),
                         f"a drop cost more than one name: {drops}")


if __name__ == "__main__":
    unittest.main()
