"""#903 item 2: the workspaces strip leads with the workspaces you have been in, and
then does not move.

*"active last used tabs should be in first order, then olds."*

**The interesting part is that this contradicts two measured decisions and both of them
were right.** `slots._cuts` refused a window centred on the marked tab — *"a window
CENTRED on the marked tab moves every column each time the operator switches… on this
project's own fifteen workspaces at 160 columns, six of the nine drawn tabs answer a
second press at the identical column with a SECOND, different workspace"* — and
`chats.of_workspace` refused recency for the same reason: *"`api.1` stays leftmost, where
an operator learned to look for it, instead of jumping to the end because it happens to
be the newest."*

Both are about a row that RE-SORTS WHILE YOU LOOK AT IT. Neither argues against an order
that is fixed for as long as the frame is open. So the recency is measured once, written
into the frame's own state directory (`state.record_tab_order`), and read by every process
that draws or walks the strip afterwards; a switch updates the mtimes for the next frame
and moves no column now.

**Chats keep ordinal order and that is not an oversight** — `TheChatsStripIsUnchanged` is
the control. A handful of numbered siblings is a stronger promise kept as `api.1`,
`api.2`, `api.3` than it would be as a recency list.

**No new kind of state and no tally.** The recency is `chats.touched_by_workspace`, which
is the mtime of each chat's own directory under `.charter/frame/` — a number charter
already moves every time it writes anything about that chat. The per-frame ORDER is a file
beside the density a palette row chose and the height `F3` recorded, so `state.reap`
deletes it with the frame and there is nothing for `doctor` to explain.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config
from charter.frame import chats, choose, slots, state, switch

from tests._isolation import PersonaIso
from tests.test_frame_chat_switch import _plant

#: Far enough apart that no filesystem's mtime granularity can round two of them together
#: — one second is the coarsest resolution charter has to survive, and these are a day.
DAY = 86400.0


def _touched(fid: str, when: float) -> None:
    """Make *fid*'s chat directory look as though charter last wrote to it at *when*.

    The DIRECTORY and not a file inside it, because that is what
    `chats.touched_by_workspace` reads — see there for why it is the directory rather than
    `version`: a rename into it moves its mtime, so every writer in `frame/state.py`
    counts and a writer added tomorrow counts too.
    """
    d = state.frame_dir(fid)
    os.utime(d, (when, when))


class TheRecencyIsMeasuredAndNotTallied(PersonaIso, unittest.TestCase):
    """`chats.touched_by_workspace` — the timestamps, with no ordering on top of them."""

    def test_a_workspace_is_as_recent_as_its_newest_chat(self):
        """The NEWEST and not the oldest or the mean: a workspace you were in an hour ago
        is a workspace you were in an hour ago however long its other chats have sat."""
        _plant("alpha.1", workspace="alpha")
        _plant("alpha.2", workspace="alpha")
        _touched("alpha.1", DAY)
        _touched("alpha.2", DAY * 9)
        self.assertEqual(chats.touched_by_workspace(), {"alpha": DAY * 9})

    def test_a_workspace_with_no_chats_is_absent_rather_than_zero(self):
        """`counts_by_workspace`' own rule, and for its reason: this answers about the
        chats on disk and the caller is asking about the names on its own strip. The two
        lists are not the same list."""
        _plant("alpha.1", workspace="alpha")
        (config.WORKSPACES_DIR / "beta").mkdir(parents=True, exist_ok=True)
        self.assertEqual(list(chats.touched_by_workspace()), ["alpha"])

    def test_a_chat_charter_cannot_place_is_timed_nowhere(self):
        """It is in no workspace's roster either, so a timestamp for it would order a tab
        nothing on the strip could be pressed to reach."""
        os.makedirs(state._root() / "orphan.1", exist_ok=True)
        _plant("alpha.1", workspace="alpha")
        self.assertEqual(list(chats.touched_by_workspace()), ["alpha"])

    def test_an_unreadable_frame_root_is_no_timestamps_and_no_raise(self):
        """This runs in the LAUNCHER and on a panel's first paint. A plane charter cannot
        scan is "no recency", which degrades to the alphabetical order that shipped before
        this feature — never an exception out of a strip."""
        with mock.patch.object(chats.os, "scandir", side_effect=OSError("gone")):
            self.assertEqual(chats.touched_by_workspace(), {})

    def test_a_chat_directory_that_goes_between_the_scan_and_the_stat_is_skipped(self):
        """`state.reap` can remove a chat while this is walking. The one that vanished
        contributes nothing and every other workspace is still timed."""
        _plant("alpha.1", workspace="alpha")
        _plant("beta.1", workspace="beta")
        _touched("beta.1", DAY)
        real = state.frame_dir

        def gone(fid, **kw):
            d = real(fid, **kw)
            return d / "not-a-directory" if fid == "alpha.1" else d

        with mock.patch.object(chats.state, "frame_dir", side_effect=gone):
            self.assertEqual(chats.touched_by_workspace(), {"beta": DAY})


class TheStripLeadsWithTheWorkingSet(PersonaIso, unittest.TestCase):
    """`switch.workspaces(fid)` — the order, asked of the function the strip asks."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        for name in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)

    def _plane(self, **when: float) -> None:
        """One chat per named workspace, each touched at its own instant."""
        for ws, at in when.items():
            _plant(f"{ws}.1", workspace=ws)
            _touched(f"{ws}.1", at)

    def test_the_most_recently_used_workspace_is_leftmost(self):
        self._plane(alpha=DAY, beta=DAY * 3, gamma=DAY * 2)
        self.assertEqual(switch.workspaces(self.FID)[:3], ["beta", "gamma", "alpha"])

    def test_the_opposite_recency_gives_the_opposite_order(self):
        """**Two opposite inputs and the two orders they must produce**, which is what
        makes the sort direction pinned rather than merely exercised: a `sorted` that
        dropped the negation, or reversed the comparison, passes the case above on some
        plane and fails here on every one.
        """
        self._plane(alpha=DAY * 3, beta=DAY, gamma=DAY * 2)
        self.assertEqual(switch.workspaces(self.FID)[:3], ["alpha", "gamma", "beta"])

    def test_two_workspaces_touched_in_the_same_tick_fall_back_to_name_order(self):
        """**A deterministic tie-break and not `os.scandir`'s order.** Mtimes have a
        resolution and two chats written in the same second are a real state, so the
        second key is the name — which is also the order a workspace charter has no
        timestamp for at all falls into. Reversing an ascending sort would answer these
        two backwards while agreeing with every case above.
        """
        self._plane(alpha=DAY, beta=DAY, gamma=DAY * 5)
        self.assertEqual(switch.workspaces(self.FID)[:3], ["gamma", "alpha", "beta"])

    def test_a_workspace_charter_has_never_seen_used_sorts_after_every_one_it_has(self):
        """`default` is folded in whether or not it has a directory and no chat has ever
        been in it; it is at the end rather than at the front, where an unmeasured name
        would be if the missing timestamp read as "now"."""
        self._plane(alpha=DAY)
        got = switch.workspaces(self.FID)
        self.assertEqual(got[0], "alpha")
        self.assertEqual(sorted(got[1:]), got[1:])
        self.assertIn(config.DEFAULT_WORKSPACE, got)

    def test_a_caller_with_no_frame_gets_the_alphabet(self):
        """`to_workspace` wants membership and a "have: …" sentence, and the launch picker
        runs before any frame exists. Neither is about which order a strip draws, and
        neither has a frame to record one in."""
        self._plane(alpha=DAY, beta=DAY * 3, gamma=DAY * 2)
        self.assertEqual(switch.workspaces(), sorted(switch.workspaces()))
        self.assertEqual(switch.workspaces(""), sorted(switch.workspaces()))

    def test_asking_without_a_frame_records_nothing_for_any_frame(self):
        """The empty id must not reach `state.tab_order` and be answered "nothing
        recorded" on every call — that is live reordering arriving through the one caller
        with no frame to hold an order for."""
        self._plane(alpha=DAY)
        switch.workspaces("")
        self.assertEqual(state.tab_order(""), [])


class TheOrderIsHeldForTheFramesLife(PersonaIso, unittest.TestCase):
    """The half the two prior refusals are about: a column an operator aimed at is still
    that name a moment later."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        for name in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
            _plant(f"{name}.1", workspace=name)
        _touched("alpha.1", DAY * 3)
        _touched("beta.1", DAY * 2)
        _touched("gamma.1", DAY)

    def test_using_another_workspace_does_not_move_a_column_now(self):
        """**The whole feature.** A switch touches the arriving chat — `_switch_client`
        re-lays the frame out, which bumps it — so the recency measurement really does
        change under the running frame. What the strip draws does not.
        """
        first = switch.workspaces(self.FID)
        _touched("gamma.1", DAY * 99)
        self.assertEqual(switch.workspaces(self.FID), first)
        self.assertEqual(chats.touched_by_workspace()["gamma"], DAY * 99,
                         "the case did not actually move the thing it is about")

    def test_a_second_process_asking_draws_the_identical_order(self):
        """A panel is torn down and re-split by every re-layout, so "compute it once" has
        to mean once per FRAME and not once per process. The record is what makes the
        launcher, the `frame-resize` child and each panel agree."""
        first = switch.workspaces(self.FID)
        _touched("gamma.1", DAY * 99)
        # The file, read the way another process reads it — no `switch.workspaces` in
        # front of it, so this is what a launcher and a `frame-resize` child see and not
        # a second call into the function that might recompute.
        self.assertEqual(state.tab_order(self.FID), first,
                         "the order a second process reads is not the one that was drawn")

    def test_a_workspace_made_since_goes_on_the_end_and_moves_nothing(self):
        """A name created mid-session is appended, in name order, where it cannot move a
        column an operator is already aiming at. Re-recording the whole order to fit it in
        by recency is exactly the live reordering this refuses."""
        first = switch.workspaces(self.FID)
        (config.WORKSPACES_DIR / "aaa-new").mkdir(parents=True, exist_ok=True)
        (config.WORKSPACES_DIR / "zzz-new").mkdir(parents=True, exist_ok=True)
        got = switch.workspaces(self.FID)
        self.assertEqual(got[:len(first)], first)
        self.assertEqual(got[len(first):], ["aaa-new", "zzz-new"])

    def test_a_workspace_deleted_since_is_simply_not_drawn(self):
        """The record is an ORDER and never a roster: `workspace.list_workspaces` decides
        which names exist, so a stale line cannot resurrect a directory that has gone."""
        switch.workspaces(self.FID)
        (config.WORKSPACES_DIR / "beta").rmdir()
        self.assertNotIn("beta", switch.workspaces(self.FID))
        self.assertIn("beta", state.tab_order(self.FID),
                      "the record was rewritten, so nothing was being tested")

    def test_a_recorded_name_that_is_not_a_name_is_dropped_on_the_way_out(self):
        """`state.tab_order` holds every line to `workspace.valid_name`, like every other
        name charter reads off disk and joins onto a path (#442). The file is charter's
        own, so this is a floor rather than the whole guard."""
        state.record_tab_order(self.FID, ["../../etc", "", "beta", "alpha"])
        self.assertEqual(state.tab_order(self.FID), ["beta", "alpha"])
        self.assertEqual(switch.workspaces(self.FID)[:2], ["beta", "alpha"])

    def test_an_unreadable_record_recomputes_rather_than_raising(self):
        """This is read on a panel's render path and inside the `frame-resize` child. A
        truncated write degrades to "no order recorded", which is the same answer the file
        was written from a moment ago."""
        state.record_tab_order(self.FID, ["beta"])
        with mock.patch.object(state.Path, "read_text", side_effect=OSError("gone")):
            self.assertEqual(state.tab_order(self.FID), [])
            self.assertEqual(switch.workspaces(self.FID)[0], "alpha",
                             "an unreadable order did not fall back to the recency")


class EverySurfaceDrawsTheOneOrder(PersonaIso, unittest.TestCase):
    """The strip, the palette and the keyboard walk are three front doors onto one list,
    and #882 already settled that they may not disagree — `statusline._persona_chip_cells`
    lifting the active persona while the picker did not is the defect that produced
    `persona.by_use`. This is that rule arriving at the second noun."""

    FID = "alpha.1"

    def setUp(self):
        super().setUp()
        for name in ("alpha", "beta", "gamma"):
            (config.WORKSPACES_DIR / name).mkdir(parents=True, exist_ok=True)
            _plant(f"{name}.1", workspace=name)
        _touched("alpha.1", DAY)
        _touched("beta.1", DAY * 3)
        _touched("gamma.1", DAY * 2)
        slots.TABS.forget()
        self.addCleanup(slots.TABS.forget)

    def test_the_strip_draws_the_names_in_the_frames_own_order(self):
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": ""}, clear=False):
            row = slots.workspaces_bar(self.FID, 200)[0]
        drawn = sorted((row.index(n), n) for n in switch.workspaces(self.FID)
                       if n in row)
        self.assertEqual([n for _at, n in drawn],
                         [n for n in switch.workspaces(self.FID) if n in row],
                         f"the strip drew the names in another order: {row!r}")
        self.assertLess(row.index("beta"), row.index("gamma"))
        self.assertLess(row.index("gamma"), row.index("alpha"))

    def test_the_palette_offers_them_in_the_same_order(self):
        """`choose.names_of` takes the frame's id and hands it on. A palette listing the
        alphabet beside a strip listing the working set is two answers to one question."""
        self.assertEqual(choose.names_of(choose.WORKSPACE, self.FID),
                         switch.workspaces(self.FID))


class TheChatsStripIsUnchanged(PersonaIso, unittest.TestCase):
    """**The control, and the decision it stands for.** #903 asks for recency on the
    WORKSPACES strip and explicitly leaves the chats strip alone: *"`api.1` staying
    leftmost is a stronger promise for a handful of numbered siblings than recency would
    be."*"""

    def test_a_workspaces_chats_stay_in_ordinal_order_however_recent_they_are(self):
        _plant("api.1", workspace="api")
        _plant("api.2", workspace="api")
        _plant("api.3", workspace="api")
        _touched("api.1", DAY)
        _touched("api.2", DAY * 9)
        _touched("api.3", DAY * 5)
        self.assertEqual(chats.of_workspace("api"), ["api.1", "api.2", "api.3"])


if __name__ == "__main__":
    unittest.main()
