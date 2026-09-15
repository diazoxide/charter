"""A launch hands the harness its session in the pane, and a reopened chat keeps its id —
on real tmux (#1101).

The unit cases in `tests/test_a_tab_is_linked_to_one_harness_session.py` stand in for the
`exec`. This is the claim a stand-in cannot make: the words `--session-id <uuid> --name
<chat id>` reach the process a real pane becomes, and tmux — its start command, its
environment — never holds the id. Only the `--resume` FLAG may cross tmux, and the id it asks
for is read from the chat's own record in the pane.

The recorder standing in for `claude` is `tests/test_a_chat_pane_starts_as_charter_and_
becomes_the_harness.py`'s, first on the tmux client's `PATH`; every socket is named with
`tests._tmuxreap.name` and killed in cleanup.
"""

from __future__ import annotations

import os
import shutil
import unittest
import uuid
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from charter.frame import leave, reopen, state
from tests._isolation import wired_as_today
from tests.test_a_chat_pane_starts_as_charter_and_becomes_the_harness import (
    _ARealChatOnARealServer, _eventually)

_HAS_TMUX = shutil.which("tmux") is not None

#: Ruling 10, as the module this borrows its fixture from: no case here is about wiring.
_WIRED = None


def setUpModule():
    global _WIRED
    _WIRED = wired_as_today()
    _WIRED.start()


def tearDownModule():
    if _WIRED is not None:
        _WIRED.stop()


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class TheHarnessIsHandedItsSessionInThePane(_ARealChatOnARealServer, unittest.TestCase):
    SLUG = "session-in-the-pane"

    def _fresh_record(self) -> None:
        (self.records / "harness.json").unlink(missing_ok=True)

    def test_the_harness_argv_carries_the_chosen_id_and_tmux_never_saw_it(self):
        self.assertEqual(self._launch(), 0)
        argv = self._harness()["argv"]
        self.assertEqual(argv[0], "--session-id", argv)
        u = argv[1]
        self.assertEqual(uuid.UUID(u).version, 4)
        self.assertEqual(argv[2:], ["--name", "beta.1"])
        self.assertEqual(state.kept_harness_session("beta.1"), u)
        pane = state.harness_pane("beta.1")
        started = self._tmux("display-message", "-p", "-t", pane,
                             "#{pane_start_command}").stdout
        self.assertNotIn(u, started, "the session id crossed tmux's argument parser")
        for scope in (("show-environment", "-g"),
                      ("show-environment", "-t", state.workspace_prefix(self.WS))):
            self.assertNotIn(u, self._tmux(*scope).stdout)

    def test_a_reopened_chat_comes_back_under_its_own_id(self):
        """Two chats, a quit's record and its stop, the reap a restart ends in, then a reopen
        of each: each comes back as the chat it was, and the one whose conversation exists
        is handed `--resume` with the id charter chose for it."""
        self.assertEqual(self._launch(), 0)
        u1 = self._harness()["argv"][1]
        conversation = self.tmp / "conv-1.jsonl"
        conversation.write_text("{}\n")
        state.record_conversation("beta.1", str(conversation))
        self._fresh_record()
        self.assertEqual(self._launch(), 0)
        self._harness()

        servers = commands_frame._plane_servers()
        live, windows, active = commands_frame._plane_live(servers)
        doomed = leave.stopping(leave.plan(live=live, focus=self.WS))
        self.assertEqual([c.chat for c in doomed], ["beta.1", "beta.2"])
        self.assertIsNotNone(commands_frame._record_the_plane(
            doomed, focus=self.WS, active=active, windows=windows, capture=False))
        commands_frame._stop_chats(doomed, windows=windows)
        self.assertTrue(_eventually(
            lambda: not (commands_frame._live_chats(self.socket) or set())))

        # A restart: every launcher that claimed these directories is gone.
        with mock.patch.object(state, "_launcher_is_alive", return_value=False):
            state.reap(set(), server=self.socket)
            self.assertFalse(state.frame_dir("beta.1").exists())

            def launch(args):
                return commands_frame._launch(SimpleNamespace(**vars(args), attach=False,
                                                              size=(120, 40)))

            back = {}
            for c in reopen.read().all_chats():
                self._fresh_record()
                with mock.patch.object(commands_frame, "cmd_launch", side_effect=launch):
                    r = commands_frame._reopen_one(c, quiet=True)
                self.assertIsNotNone(r, c.chat)
                back[c.chat] = (r.fid, self._harness()["argv"])

        self.assertEqual(back["beta.1"], ("beta.1", ["--resume", u1, "--name", "beta.1"]))
        fid, argv = back["beta.2"]
        self.assertEqual(fid, "beta.2")
        self.assertEqual(argv[0], "--session-id",
                         "a chat with no conversation starts fresh on a new id")
        self.assertNotEqual(argv[1], u1)
        self.assertEqual(state.new_chat_id(self.WS), "beta.3",
                         "and no new chat is handed either id")


if __name__ == "__main__":
    unittest.main()
