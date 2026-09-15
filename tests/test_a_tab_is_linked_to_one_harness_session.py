"""Every tab is linked to exactly one harness session, never guessed.

Claude Code is handed the id charter chooses (`--session-id <uuid> --name <name>`); Codex and
opencode report theirs, and charter records the first one each start reports. Only the chat's
own harness may change the link — a harness nested inside the chat changes nothing. Resume is
offered only when the conversation exists.

Measured 2026-09-15 (claude 2.1.272 live; codex-cli 0.147.0 and opencode 1.18.23 read from
their tagged source, not run). The readings are copied into ADR 0024.
"""

from __future__ import annotations

import os
import unittest
from pathlib import Path
from unittest import mock

from charter import config, contain
from charter.frame import state, tmuxctl
from tests._isolation import PersonaIso

FID = "beta.1"


def _chat(fid: str = FID, *, kind: str = "claude-code", pane: str = "",
          server: str = "srv") -> Path:
    assert state.claim_chat_id(fid), fid
    state.record_identity(fid, {"CHARTER_HARNESS": kind})
    state.record_server(fid, server)
    if pane:
        state.record_harness_pane(fid, pane)
    return state.frame_dir(fid)


class TheLinkIsRecordedBesideTheChat(PersonaIso, unittest.TestCase):
    """The records a link is made of, each in the chat's own directory."""

    def setUp(self) -> None:
        super().setUp()
        self.d = _chat()

    def test_a_session_id_is_held_to_a_shape_that_cannot_be_read_as_a_flag(self):
        """The id reaches a harness argv (`--resume <id>`), and one starting with `-` would
        be read as a flag. It arrives in a hook payload a chat's own shell can write."""
        for bad in ("-rf", "a b", "x" * 200, "", "a\nb", "../x", "ses;rm"):
            with self.subTest(sid=bad):
                self.assertFalse(state.record_harness_session(FID, bad))
                self.assertIsNone(state.kept_harness_session(FID))
        for good in ("e8962ccc-d263-4996-9881-c774dc586d3f", "ses_0123456789abABCDEFGHIJKLmn",
                     "x" * 128, "1234"):
            with self.subTest(sid=good):
                self.assertTrue(state.record_harness_session(FID, good))
                self.assertEqual(state.kept_harness_session(FID), good)

    def test_forgetting_the_link_takes_both_files(self):
        state.record_harness_session(FID, "u1")
        state.clear_harness_session(FID)
        self.assertIsNone(state.harness_session(FID))
        self.assertIsNone(state.kept_harness_session(FID))
        state.clear_harness_session("no.such")      # never raises

    def test_the_conversation_is_an_absolute_path_and_nothing_else(self):
        self.assertTrue(state.record_conversation(FID, "/abs/t.jsonl"))
        self.assertEqual(state.conversation(FID), "/abs/t.jsonl")
        for bad in ("t.jsonl", "/a\x00b", "/" + "x" * (contain.PATH_DISPLAY_LIMIT + 1), "",
                    None, 7):
            with self.subTest(path=bad):
                self.assertFalse(state.record_conversation(FID, bad))
                self.assertEqual(state.conversation(FID), "/abs/t.jsonl")
        self.assertTrue(state.record_conversation(
            FID, "/" + "x" * (contain.PATH_DISPLAY_LIMIT - 1)))
        state.clear_conversation(FID)
        self.assertIsNone(state.conversation(FID))

    def test_a_conversation_is_never_recorded_for_a_chat_with_no_directory(self):
        self.assertFalse(state.record_conversation("gamma.9", "/abs/t"))
        self.assertFalse((state._root() / "gamma.9").exists())

    def test_the_harness_pid_is_a_positive_number(self):
        state.record_harness_pid(FID, 4242)
        self.assertEqual(state.harness_pid(FID), 4242)
        for bad in (0, -3):
            with self.subTest(pid=bad):
                state.record_harness_pid(FID, bad)
                self.assertEqual(state.harness_pid(FID), 4242)
        for text in ("0", "-3", "x", "", "4.2"):
            with self.subTest(on_disk=text):
                (self.d / "harness.pid").write_text(text)
                self.assertIsNone(state.harness_pid(FID))

    def test_only_the_first_report_of_a_start_adopts(self):
        self.assertFalse(state.adopted(FID))
        self.assertTrue(state.adopt_report(FID))
        self.assertTrue(state.adopted(FID))
        self.assertFalse(state.adopt_report(FID))
        state.record_harness_pid(FID, 100)
        state.clear_adoption(FID)
        self.assertFalse(state.adopted(FID))
        self.assertIsNone(state.harness_pid(FID))
        self.assertTrue(state.adopt_report(FID))

    def test_an_adoption_for_a_chat_with_no_directory_is_refused(self):
        self.assertFalse(state.adopt_report("gamma.9"))
        self.assertFalse((state._root() / "gamma.9").exists())

    def test_the_adoption_is_claimed_through_config_exclusively(self):
        with mock.patch.object(state.config, "create_for", return_value=False) as create:
            self.assertFalse(state.adopt_report(FID))
        create.assert_called_once_with(self.d / "session.adopted", "")

    def test_a_start_says_whether_it_resumed(self):
        self.assertFalse(state.resumed_start(FID))
        state.record_start(FID, resumed=True)
        self.assertTrue(state.resumed_start(FID))
        state.record_start(FID, resumed=False)
        self.assertFalse(state.resumed_start(FID))

    def test_none_of_the_link_records_is_part_of_the_shape(self):
        """`clear_shape` runs when a launch claims the id; the link is what a reopen needs
        on the far side of it."""
        state.record_harness_session(FID, "u1")
        state.record_conversation(FID, "/abs/t")
        state.record_harness_pid(FID, 9)
        state.adopt_report(FID)
        state.record_start(FID, resumed=True)
        state.clear_shape(FID)
        self.assertEqual(state.kept_harness_session(FID), "u1")
        self.assertEqual(state.conversation(FID), "/abs/t")
        self.assertEqual(state.harness_pid(FID), 9)
        self.assertTrue(state.adopted(FID))
        self.assertTrue(state.resumed_start(FID))


class AChatIsFoundByItsPane(PersonaIso, unittest.TestCase):
    """opencode's hook overwrites `$CHARTER_SESSION_ID` with its own id, so its chat is found
    from `$TMUX_PANE` against the panes charter recorded (ruled, open question 1 (A))."""

    def test_exactly_one_chat_recorded_on_the_pane_is_the_answer(self):
        _chat("beta.1", pane="%4", server="srv")
        _chat("beta.2", pane="%5", server="srv")
        self.assertEqual(state.chat_in_pane("%4", "srv"), "beta.1")

    def test_a_pane_two_chats_record_resolves_to_none(self):
        _chat("beta.1", pane="%4", server="srv")
        _chat("beta.2", pane="%4", server="srv")
        self.assertIsNone(state.chat_in_pane("%4", "srv"))

    def test_a_pane_on_another_server_is_not_this_one(self):
        _chat("beta.1", pane="%4", server="other")
        self.assertIsNone(state.chat_in_pane("%4", "srv"))

    def test_the_server_is_compared_as_the_socket_it_names(self):
        """`$TMUX` names a socket PATH; a chat's record names the server by name."""
        _chat("beta.1", pane="%4", server="srv")
        self.assertEqual(state.chat_in_pane("%4", tmuxctl.socket_path("srv")), "beta.1")

    def test_a_chat_with_no_server_record_is_on_the_legacy_server(self):
        _chat("beta.1", pane="%4", server="srv")
        (state.frame_dir("beta.1") / "server").unlink()
        self.assertEqual(state.chat_in_pane("%4", tmuxctl.LEGACY_SOCKET), "beta.1")

    def test_an_old_frame_directory_is_not_a_chat(self):
        _chat("beta.1", pane="%4", server="srv")
        state.record_server("beta-4242", "srv")
        state.record_harness_pane("beta-4242", "%4")
        self.assertEqual(state.chat_in_pane("%4", "srv"), "beta.1")

    def test_a_pane_nobody_recorded_is_none(self):
        _chat("beta.1", pane="%4", server="srv")
        self.assertIsNone(state.chat_in_pane("%9", "srv"))

    def test_an_unlistable_frame_root_is_none(self):
        with mock.patch.object(state.os, "scandir", side_effect=OSError(13, "denied")):
            self.assertIsNone(state.chat_in_pane("%4", "srv"))


if __name__ == "__main__":
    unittest.main()
