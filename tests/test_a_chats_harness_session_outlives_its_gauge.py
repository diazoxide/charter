"""The harness's own session id is written twice and deleted once — §4e, reduced form.

One file was serving two purposes. `state.record_harness_session` records the id Claude
Code's own token-usage history is keyed by, and `state.clear_shape` deletes it with its
reason written at the line: *"a gauge reading somebody else's 78% is worse than either"*.
That reason is about a NUMBER DRAWN ON SCREEN and it is still right. It is not an argument
about the identifier, which is the only thing charter records that could ever ask a
harness for its conversation back.

So the id gets a durable sibling (`state._KEPT_SESSION_FILE`) that `clear_shape` does not
list, and **nothing else changes**: the gauge's own file is written, cleared and read
exactly as it was. `ZeroBehaviourChange` is the half of this module that says so, and it
is not decoration — the stage's whole claim is that it can ship on its own, ahead of the
gauge gate and ahead of reopen (§6 stages 2, 5 and 6 in that order).

Nothing in `charter/` reads the sibling yet. That is the stage, not an omission: reopen is
what reads it, and it cannot be written retroactively for a chat whose `session` a launch
has already cleared — which is why this ships first and alone.

No tmux anywhere in this module. Every fact here is a file in a plane's own state
directory.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import config
from charter.frame import state
from tests._isolation import PersonaIso

#: A chat id in the shape `state.new_chat_id` mints, so nothing here is testing a name
#: charter could not produce.
FID = "demo.1"

#: A harness session id shaped like Claude Code's own — a UUID, which is what
#: `statusline.main` reads off the JSON payload on its stdin and hands to
#: `record_harness_session`.
SID = "9f1c2a44-7b0e-4d21-9b3a-1f2e3d4c5b6a"


def _dir():
    """The frame directory for :data:`FID`, created — the plane-side view every
    assertion here is made against."""
    d = state.frame_dir(FID, create=True)
    assert d is not None
    return d


class TheIdIsWrittenTwice(PersonaIso, unittest.TestCase):
    """`record_harness_session` lands both files from one value."""

    def test_the_durable_sibling_is_written_beside_the_gauges_own_file(self):
        self.assertTrue(state.record_harness_session(FID, SID))
        self.assertEqual(state.harness_session(FID), SID)
        self.assertEqual(state.kept_harness_session(FID), SID)

    def test_the_two_files_can_only_disagree_by_the_sibling_being_absent(self):
        """One branch, one value, so there is no path on which the sibling holds a
        DIFFERENT id from `session` — the failure mode a second writer would have."""
        state.record_harness_session(FID, SID)
        state.record_harness_session(FID, "a-second-session")
        self.assertEqual(state.harness_session(FID), "a-second-session")
        self.assertEqual(state.kept_harness_session(FID), "a-second-session")

    def test_the_id_is_stripped_the_same_way_on_both_sides(self):
        state.record_harness_session(FID, f"  {SID}\n")
        self.assertEqual(state.kept_harness_session(FID), SID)

    def test_no_id_writes_neither_file(self):
        self.assertFalse(state.record_harness_session(FID, "   "))
        self.assertIsNone(state.harness_session(FID))
        self.assertIsNone(state.kept_harness_session(FID))

    def test_an_id_a_frame_directory_cannot_be_made_for_writes_neither(self):
        """`frame_dir` REFUSES rather than rewrites (`contain.child`), and the sibling is
        written after that refusal has already been taken, so there is no second chance
        for a name to reach the filesystem through this pair."""
        self.assertFalse(state.record_harness_session("../escape", SID))
        self.assertIsNone(state.kept_harness_session("../escape"))
        self.assertEqual(sorted(p.name for p in config.STATE_DIR.glob("**/session*")), [])


class TheSiblingSurvivesTheClear(PersonaIso, unittest.TestCase):
    """The property the whole stage exists for."""

    def test_clear_shape_takes_the_gauges_file_and_leaves_the_id(self):
        state.record_harness_session(FID, SID)
        state.clear_shape(FID)
        self.assertIsNone(state.harness_session(FID),
                          "the gauge's mapping must still be destroyed — #413")
        self.assertEqual(state.kept_harness_session(FID), SID,
                         "the identifier a resume needs must survive")

    def test_the_file_clear_shape_deletes_is_not_the_file_it_keeps(self):
        """Named against the list itself, not only against its effect. Adding
        `_KEPT_SESSION_FILE` to `clear_shape`'s tuple is a one-word edit that would take
        resume away with nothing on screen to say so, and this is what goes red for it."""
        d = _dir()
        state.record_harness_session(FID, SID)
        state.clear_shape(FID)
        left = sorted(p.name for p in d.iterdir())
        self.assertIn(state._KEPT_SESSION_FILE, left)
        self.assertNotIn("session", left)

    def test_a_second_clear_leaves_it_alone_too(self):
        """`clear_shape` runs on every launch that claims an id, so "survives one" is not
        the claim — "is never in the list" is."""
        state.record_harness_session(FID, SID)
        for _ in range(3):
            state.clear_shape(FID)
        self.assertEqual(state.kept_harness_session(FID), SID)


class ZeroBehaviourChange(PersonaIso, unittest.TestCase):
    """What a reviewer has to be able to check in one place: the gauge's half is
    untouched, so this stage can land ahead of the gate §4e settles."""

    def test_the_return_value_still_means_the_gauge_moved(self):
        """`statusline.main` repaints the frame's panels on a `True`, several times per
        turn. A second writer that changed what `True` means would repaint on every
        status-line render."""
        self.assertTrue(state.record_harness_session(FID, SID))
        self.assertFalse(state.record_harness_session(FID, SID),
                         "an unchanged id is still not a change")

    def test_a_sibling_that_cannot_be_written_does_not_take_the_gauge_down(self):
        """The durable write is the LAST thing that happens and its failure is quieter
        than the gauge's own: the mapping has already landed, so this chat still draws
        its context reading and simply cannot be resumed later."""
        real = config.write_for

        def refuse_the_sibling(path, text, **kw):
            if path.name.startswith("session.durable"):
                raise OSError("no room for the durable copy")
            return real(path, text, **kw)

        with mock.patch.object(config, "write_for", refuse_the_sibling):
            self.assertTrue(state.record_harness_session(FID, SID))
        self.assertEqual(state.harness_session(FID), SID)
        self.assertIsNone(state.kept_harness_session(FID))

    def test_a_chat_already_running_when_this_charter_arrived_gets_no_back_fill(self):
        """The migration gap, pinned rather than discovered. Its `session` was written by
        the previous version, so the no-op guard returns before the sibling is reached and
        the sibling first appears when that harness's own session id next changes. Reading
        `session` as a stand-in would be reading exactly the file whose deletion this pair
        exists to survive."""
        d = _dir()
        (d / "session").write_text(f"{SID}\n")
        self.assertFalse(state.record_harness_session(FID, SID))
        self.assertIsNone(state.kept_harness_session(FID))
        self.assertTrue(state.record_harness_session(FID, "the-next-session"))
        self.assertEqual(state.kept_harness_session(FID), "the-next-session")


class ItDoesNotOUTLIVETheChatItself(PersonaIso, unittest.TestCase):
    """The other end of "durable", and #731 is why it is asserted rather than assumed.

    #731 is a recycled chat id inheriting the PREVIOUS frame's workspace pointer and lock:
    `state.new_chat_id` frees an ordinal the moment its frame DIRECTORY is reaped, while
    `.charter/sessions/<fid>.workspace` and `.lock` live somewhere else entirely and are
    left behind — so `charter claude --workspace alpha` can come up labelled `gamma`.

    **This file cannot do that, and the reason is structural rather than careful.** It
    lives INSIDE `.charter/frame/<fid>/`, which is the very directory whose absence frees
    the ordinal — `reap` removes it whole. So "the id is free" and "the durable session id
    is gone" are one event, not two, and there is no window in which a new chat can be
    handed an old one's harness session.

    That is the invariant a resume has to rest on, so it is pinned here rather than left as
    a property the design happens to have. It is also the exact contrast with #731: the
    defect there is a per-chat file kept OUTSIDE the directory that decides the chat's
    lifetime, and the fix is to reap it with the frame — not to spread this file the same
    way.
    """

    def test_a_recycled_ordinal_cannot_inherit_the_previous_chats_durable_id(self):
        state.record_harness_session(FID, SID)
        self.assertEqual(state.kept_harness_session(FID), SID)
        # The whole of what frees the ordinal, and the whole of what removes the file.
        self.assertEqual(state.reap(set(), server=state.frame_server(FID) or ""), [FID])
        self.assertEqual(state.new_chat_id("demo"), FID,
                         "the ordinal is only free because the directory went")
        self.assertIsNone(state.kept_harness_session(FID),
                          "a new chat must not be handed the previous chat's session id")

    def setUp(self) -> None:
        super().setUp()
        # `reap` keeps a directory whose recorded server is not the one being reaped, and
        # a chat's own launcher writes that marker — so the fixture writes it too.
        state.frame_dir(FID, create=True)
        state.record_server(FID, "charter")
        state.record_workspace(FID, "demo")


class TheReaderAnswersNoneRatherThanRaising(PersonaIso, unittest.TestCase):
    """`kept_harness_session`'s own arms — `harness_session`'s, one file over, because a
    reader on a panel's repaint path may never take a frame down with it."""

    def test_never_recorded(self):
        _dir()
        self.assertIsNone(state.kept_harness_session(FID))

    def test_no_such_frame_directory(self):
        self.assertIsNone(state.kept_harness_session("nothing-here.7"))

    def test_an_id_that_is_not_a_frame_directorys_name(self):
        self.assertIsNone(state.kept_harness_session("../escape"))

    def test_a_file_that_cannot_be_read(self):
        """A directory where the file should be — an `IsADirectoryError`, which is an
        `OSError`, and the only way to make this read fail without root."""
        (_dir() / state._KEPT_SESSION_FILE).mkdir()
        self.assertIsNone(state.kept_harness_session(FID))

    def test_a_file_that_is_not_text(self):
        """The `ValueError` half of the same clause, and it is reachable rather than
        defensive: `UnicodeDecodeError` IS a `ValueError`, and this file is bytes on disk
        that some other process may have put there."""
        (_dir() / state._KEPT_SESSION_FILE).write_bytes(b"\xff\xfe not utf-8 \xff")
        self.assertIsNone(state.kept_harness_session(FID))

    def test_a_file_holding_only_whitespace(self):
        """`or None`, and it is the same rule `harness_session` keeps: "recorded nothing"
        and "not recorded" are one answer, because every caller does the same thing with
        both."""
        (_dir() / state._KEPT_SESSION_FILE).write_text("   \n")
        self.assertIsNone(state.kept_harness_session(FID))


if __name__ == "__main__":
    unittest.main()
