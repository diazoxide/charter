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

    def test_the_name_on_disk_is_the_contract_and_not_an_implementation_detail(self):
        """**The spelling is load-bearing across VERSIONS, which is why it is asserted
        rather than read back through the constant.** Every other test here goes through
        `state._KEPT_SESSION_FILE`, so a rename would move writer and reader together and
        no test would notice — the deletion sweep found exactly that and was right to.

        But this file is durable state in a directory a much newer charter reopens (§5's
        "survive a schema change across a long-lived chat directory"). Renaming it after
        this ships silently loses the id for every chat already recorded, and the operator
        sees a resume that quietly does not resume. So the name is pinned as bytes, and
        changing it is a deliberate act with a red test in front of it.

        The exact-set assertion is the second half: `record_harness_session` leaves the two
        files it writes and no `.tmp` beside them, so a half-finished atomic write cannot
        be mistaken for state either."""
        state.record_harness_session(FID, SID)
        self.assertEqual(sorted(p.name for p in _dir().iterdir()),
                         ["session", "session.durable"])

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


class AHookIsTheWriterNow(PersonaIso, unittest.TestCase):
    """#895 deleted the only process that wrote this id, so a hook writes it instead.

    Charter no longer puts a `statusLine` in Claude Code's settings, and that command was
    `record_harness_session`'s one caller. Without a replacement every Claude Code chat
    would answer `charter reopen` with *"no session id recorded for this chat yet"* — a
    feature going out silently, which #895 neither asked for nor mentioned. A hook holds
    both ids for the same two reasons the status line did: it runs in the chat's own pane,
    so `$CHARTER_SESSION_ID` is in its environment, and its stdin payload carries
    `session_id`.
    """

    def _run(self, *, harness="claude-code", chat=FID, sid=SID, in_plane=True):
        import os

        from charter import hooks
        env = {}
        if chat is not None:
            env["CHARTER_SESSION_ID"] = chat
        if harness is not None:
            env["CHARTER_HARNESS"] = harness
        with mock.patch.dict(os.environ, env, clear=False), \
                mock.patch.object(hooks, "_in_a_plane", return_value=in_plane):
            if chat is None:
                os.environ.pop("CHARTER_SESSION_ID", None)
            if harness is None:
                os.environ.pop("CHARTER_HARNESS", None)
            hooks._record_harness_session({"session_id": sid} if sid else {})

    def test_a_claude_code_hook_records_the_id_reopen_asks_with(self):
        self._run()
        self.assertEqual(state.harness_session(FID), SID)
        self.assertEqual(state.kept_harness_session(FID), SID)

    def test_it_wakes_the_frame_so_a_panel_sees_the_new_chat(self):
        """`record_harness_session` returning True is what the old writer used to decide a
        repaint on, and a panel that never learns the mapping changed draws the previous
        chat's row until something else happens to bump it."""
        before = state.version(FID)
        self._run()
        self.assertNotEqual(state.version(FID), before)

    def test_a_second_hook_with_the_same_id_writes_nothing_new(self):
        """`sessionstart` fires on resume, clear and compact as well as startup. The no-op
        guard is what keeps that from being a repaint storm."""
        self._run()
        after_first = state.version(FID)
        self._run()
        self.assertEqual(state.version(FID), after_first)

    def test_another_harness_records_nothing(self):
        """The gate `_turn_begin` already keeps, for `state.harness_session`'s reason:
        nothing but Claude Code is handed a usage payload, and `leave.resumable_harness`
        offers a resume for nothing else — so an id written here would be a record with no
        reader."""
        self._run(harness="opencode")
        self.assertIsNone(state.harness_session(FID))

    def test_an_unknown_harness_records_nothing_either(self):
        self._run(harness=None)
        self.assertIsNone(state.harness_session(FID))

    def test_a_payload_with_no_session_id_records_nothing(self):
        """A hook can fire before the harness has an id to give. Writing an empty one would
        put a chat into the resumable state holding nothing to resume."""
        self._run(sid=None)
        self.assertIsNone(state.harness_session(FID))

    def test_outside_a_frame_there_is_no_chat_to_record_against(self):
        self._run(chat=None)
        self.assertIsNone(state.harness_session(FID))

    def test_outside_a_plane_nothing_is_written(self):
        """#852's rule, kept: the record lands under `config.STATE_DIR`, which outside a
        plane is a `.charter/` in somebody else's checkout."""
        self._run(in_plane=False)
        self.assertIsNone(state.harness_session(FID))

    def test_sessionstart_is_where_it_is_called_from(self):
        """The wiring, not just the helper. A function nothing dispatches into records
        nothing, and that is exactly the state this class exists to prevent returning to."""
        import inspect

        from charter import hooks
        self.assertIn("_record_harness_session(data)",
                      inspect.getsource(hooks.sessionstart))


if __name__ == "__main__":
    unittest.main()
