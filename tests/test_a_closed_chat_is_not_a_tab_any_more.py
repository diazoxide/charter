"""**A chat the operator closed stops being a tab — #929.**

*"`-` button is not working … after choosing close - its not closing and breaking entire
flow, even hard to explain."*

Reproduced on a real tmux frame with a real client on a real pty: the close **ran**. The
confirmation drew, Enter committed, `charter frame-close` wrote `state.record_closed` and
`kill-window` took the chat's window off the server. Every part of the teardown worked. What
did not happen is the only part the operator could see — **the tab was still on the strip**,
in the workspace's other chats and in the chat's own, because `chats._by_workspace` scanned
the frame root and never asked `state.was_closed`.

So the report is exact and the words are the right ones. From the operator's seat closing a
chat left the strip byte-for-byte as it was, which reads as *nothing happened* — and then
the ghost tab goes on being clickable, which is the *"breaking entire flow"*: pressing it
answers `cannot switch: chat 'alpha.1' has no window any more`, measured, and it will do that
for as long as the directory survives `state.reap`.

**This is older than the `-` that reported it.** `state.record_closed` has been written since
#796 and `state.was_closed` has been read in exactly one place ever — `leave.plan`, so that a
later quit does not resurrect a closed chat. The strip has never asked. #925 is what made it
matter: it put the close affordance ON the chat strip, so the operator's eye is now on the
one surface that does not change, where `F2 → chat: close` left them looking at a harness.
A defect nobody could see became one nobody could miss, and the fix belongs where the fact
was always missing rather than in the affordance that surfaced it.

**The fix is in the scan and not in the strip**, which is what makes the roster, the counts
and the switcher agree: `_by_workspace` is the one walk `of_workspace`, `counts_by_workspace`
and `touched_by_workspace` are, so a closed chat leaves every surface at once and none of
them holds a rule of its own about it.

**Two things deliberately keep seeing a closed chat**, and both are asserted below because
each would be a real defect if the filter reached it:

* `leave.plane_chats` — the teardown's own scan, which must enumerate every chat directory
  on the plane whatever it says, so that a quit stops a harness that is still running rather
  than skipping it and recording nothing (that function's own docstring is the argument).
  `leave.plan` then skips closed chats by its own rule, one layer up.
* `chats.roster`'s fold-in of the chat ASKING. `cmd_close` writes the marker *before* it
  kills the window, so between those two the closing chat's own panel repaints — and a strip
  that dropped the tab you are typing in would be drawing a list the operator is not in.
"""

from __future__ import annotations

import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from charter.frame import chats, leave, slots, state

from tests._isolation import PersonaIso

SERVER = commands_frame.SOCKET


def _plant(fid: str, *, ws: str) -> None:
    """One chat on the plane, recorded the way a launch records it."""
    state.frame_dir(fid, create=True)
    state.record_workspace(fid, ws)
    state.record_server(fid, SERVER)
    state.record_harness_pane(fid, "%1")
    state.record_identity(fid, {"CHARTER_HARNESS": "claude-code"})


class AClosedChatLeavesTheStrip(PersonaIso, unittest.TestCase):
    """What every surface built on `chats._by_workspace` says once a chat is closed."""

    def setUp(self) -> None:
        super().setUp()
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        _plant("alpha.3", ws="alpha")

    def test_the_roster_still_has_every_chat_before_one_is_closed(self):
        """The negative control this whole class needs: the filter below removes something
        that was there, rather than passing on a plane the fixture never planted."""
        self.assertEqual(chats.of_workspace("alpha"),
                         ["alpha.1", "alpha.2", "alpha.3"])

    def test_a_closed_chat_is_not_in_the_workspace_roster(self):
        state.record_closed("alpha.2")

        self.assertEqual(chats.of_workspace("alpha"), ["alpha.1", "alpha.3"])

    def test_a_closed_chat_is_not_a_tab_on_another_chats_strip(self):
        """The surface the report is about, asked as the strip asks it."""
        state.record_closed("alpha.2")

        names, here, _add, _close, _counts = slots._chats_strip("alpha.1")

        self.assertEqual(names, ["alpha.1", "alpha.3"])
        self.assertEqual(here, "alpha.1")

    def test_the_closed_chat_is_not_drawn_on_the_row(self):
        """One rung down: the text a terminal receives, not the list behind it."""
        state.record_closed("alpha.2")

        row = slots._bar(*slots._chats_strip("alpha.1")[:2], 80,
                         note=slots.ADD_CHAT, close=slots.CLOSE_CHAT)[0]

        self.assertNotIn("alpha.2", row)
        self.assertIn("alpha.1", row)
        self.assertIn("alpha.3", row)

    def test_nothing_can_be_switched_to_a_closed_chat(self):
        """`chats.others` is what a picker offers and what "is there anything to switch
        to" is asked as — a closed chat in it is a row that answers *no window any more*."""
        state.record_closed("alpha.2")

        self.assertEqual(chats.others("alpha.1"), ["alpha.3"])

    def test_a_closed_chat_is_not_counted_on_the_workspaces_strip(self):
        """The count beside a workspace tab is the same walk, so it moves with the roster
        rather than going on promising a chat that is gone."""
        state.record_closed("alpha.2")

        self.assertEqual(chats.counts_by_workspace().get("alpha"), 2)

    def test_the_chat_asking_is_still_its_own_tab_even_once_marked(self):
        """`cmd_close` writes the marker BEFORE `kill-window`, so this state is real for as
        long as the teardown takes — and a strip that dropped the tab the operator is typing
        in would be drawing a list they are not in (`chats.roster`'s fold-in)."""
        state.record_closed("alpha.1")

        self.assertEqual([c.id for c in chats.roster("alpha.1")],
                         ["alpha.1", "alpha.2", "alpha.3"])
        self.assertTrue(chats.roster("alpha.1")[0].active)

    def test_the_teardown_still_sees_every_chat_on_the_plane(self):
        """`leave.plane_chats` must NOT inherit this filter: it is the scan a quit stops
        harnesses from, and a chat it skipped would be killed and recorded nowhere. The skip
        for a closed chat belongs to `leave.plan`, one layer up, and is still there."""
        state.record_closed("alpha.2")

        self.assertEqual(leave.plane_chats(), ["alpha.1", "alpha.2", "alpha.3"])
        self.assertEqual([c.chat for c in leave.plan(live=None, focus="alpha").chats],
                         ["alpha.1", "alpha.3"])


class ClosingWakesTheStripsThatMustRedraw(PersonaIso, unittest.TestCase):
    """**The half of the fix that is not on disk.**

    A panel repaints on a version bump and on nothing else (`frame/panel.py`), and closing a
    chat moves no version but the closed chat's own — whose window is being killed. Measured
    on a real frame with the scan already fixed: the surviving chat's strip still drew the
    closed tab seven seconds later and corrected only when a click woke that panel for an
    unrelated reason. Right plane, wrong screen, and from the operator's seat that is the
    same report.
    """

    def _close(self, target: str, *, on: str) -> int:
        """`cmd_close` with the tmux round trips stood in for — every one of them is
        asserted for real in `tests/test_close_is_the_one_teardown_that_forgets.py`, and
        what this class is about is the writes charter makes either way."""
        with mock.patch.multiple(commands_frame,
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = [(c, f"@{i}", False) for i, c in
                                             enumerate(leave.plane_chats())]
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 1
            return commands_frame.cmd_close(
                SimpleNamespace(chat=on, chat_id=target))

    def test_every_other_chat_on_the_plane_is_bumped(self):
        """Every other chat, and not only this workspace's: the workspaces strip draws a
        count for every workspace on the plane, and closing a chat changes one of them."""
        for fid, ws in (("alpha.1", "alpha"), ("alpha.2", "alpha"), ("beta.1", "beta")):
            _plant(fid, ws=ws)
        before = {f: state.version(f) for f in ("alpha.1", "alpha.2", "beta.1")}

        self.assertEqual(self._close("alpha.2", on="alpha.1"), 0)

        self.assertNotEqual(state.version("alpha.1"), before["alpha.1"])
        self.assertNotEqual(state.version("beta.1"), before["beta.1"])

    def test_the_closed_chat_is_not_bumped(self):
        """Its window is going, and `chats.roster` folds the chat asking into its own strip
        whatever this answers — so there is nothing a repaint there could change."""
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        before = state.version("alpha.2")

        self._close("alpha.2", on="alpha.1")

        self.assertEqual(state.version("alpha.2"), before)

    def test_a_close_that_could_not_stop_the_window_still_wakes_them(self):
        """`state.was_closed` is true from the moment the marker lands, whether or not the
        kill after it worked — so the strips are stale on that path too, and a repaint that
        waited for the kill would leave the screen disagreeing with the plane on exactly the
        path where charter has already said something went wrong."""
        _plant("alpha.1", ws="alpha")
        _plant("alpha.2", ws="alpha")
        before = state.version("alpha.1")

        with mock.patch.multiple(commands_frame,
                                 _chat_seats=mock.DEFAULT,
                                 _capture_transcript=mock.DEFAULT,
                                 _stop_chats=mock.DEFAULT) as m:
            m["_chat_seats"].return_value = [("alpha.1", "@0", False),
                                             ("alpha.2", "@1", False)]
            m["_capture_transcript"].return_value = False
            m["_stop_chats"].return_value = 0
            self.assertEqual(commands_frame.cmd_close(
                SimpleNamespace(chat="alpha.1", chat_id="alpha.2")), 1)

        self.assertNotEqual(state.version("alpha.1"), before)


if __name__ == "__main__":
    unittest.main()
