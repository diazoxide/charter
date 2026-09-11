"""A chat opened in the background really starts on its whole first message, and nobody's
window moves.

Real tmux on a private socket (`tests._tmuxreap`), driven through `open_in_background` and
the real `_launch`, with only the harness binary swapped for a recorder that writes the argv
it was started with and then stays alive — so the launch goes all the way through
`select-pane` rather than stopping at an early death.

**Why `_spawn_gather` is stood in.** A launch fills the new chat's gather cache in a detached
`charter frame-gather` child (`util.detach_self`, `start_new_session=True`), and
`tests/_planeguard.py` refuses exactly that child in the suite: it would outlive the test and
spend a gather on whatever plane its copied environment resolves. What is asked here is what
tmux does, not what the gather writes.

**What the #959 tests already hold, and what this adds.** `tests/test_a_harness_argument_ending_
in_a_semicolon_arrives_whole.py` measures the builders byte for byte and
`tests/test_a_launch_too_long_for_tmux_says_so.py` tmux's command limit. This file sends a
brief through Task 1's own path — `first_message_argv`, the `Opening`, the launcher — and
checks that it arrives whole, that the session's current window stays where it was (M3), and
that a first message at charter's own bound still fits under tmux's limit with the identity a
real launch carries.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
import unittest
from unittest import mock

from charter import commands_frame, config
from charter.frame import state, tmuxctl
from charter.harness import claude_code

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane

_HAS_TMUX = shutil.which("tmux") is not None


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ABackgroundChatReallyStartsOnItsBrief(PersonaIso, unittest.TestCase):

    #: A brief that tmux's own parser and a shell would each mangle if charter joined, split
    #: or failed to escape it: a trailing `;` (#957, carried through by #959's
    #: `tmuxctl.verbatim`), a blank line, quotes, command substitution and a tmux format.
    BRIEF = 'fix the widget;\n\nit says "$(echo boom)" and `x` at #{session_name};'

    #: The calling chat's window — small enough that no panel is drawable, so no panel
    #: process starts (asserted in `setUp` rather than assumed).
    COLS, ROWS = 20, 6

    def setUp(self):
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]};"
                          f" this machine has {v}")
        make_plane(self)
        self.socket = _tmuxreap.name("handoff-argv")
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        self.addCleanup(subprocess.run, ["tmux", "-L", self.socket, "kill-server"],
                        capture_output=True, timeout=20)
        self.records = self.tmp / "records"
        self.records.mkdir()
        self.recorder = self.tmp / "claude-recorder"
        self.recorder.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys, time\n"
            "p = os.path.join(os.environ['RECORD_DIR'],\n"
            "                 os.environ['TMUX_PANE'].lstrip('%') + '.json')\n"
            "with open(p + '.tmp', 'w') as f:\n"
            "    json.dump(sys.argv[1:], f)\n"
            "os.replace(p + '.tmp', p)\n"
            "time.sleep(120)\n")
        self.recorder.chmod(0o755)
        server_env = dict(os.environ, RECORD_DIR=str(self.records),
                          CHARTER_ROOT=str(config.ROOT))
        self.caller_pane = self._session("alpha", server_env)
        self.target_pane = self._session("beta", server_env)
        for fid, ws, pane in (("alpha.1", "alpha", self.caller_pane),
                              ("beta.1", "beta", self.target_pane)):
            (config.WORKSPACES_DIR / ws).mkdir(parents=True, exist_ok=True)
            state.frame_dir(fid, create=True)
            state.record_workspace(fid, ws)
            state.record_server(fid, self.socket)
            state.record_identity(fid, {"CHARTER_HARNESS": "claude-code"})
            state.record_harness_pane(fid, pane)
            # What a launcher writes on the window, and what `reap` keeps a chat alive by.
            named = self._tmux("set-option", "-w", "-t", pane, commands_frame._CHAT_OPTION,
                               fid)
            self.assertEqual(named.returncode, 0, named.stderr)
        self.assertEqual(commands_frame._window_size(self.socket, self.caller_pane),
                         (self.COLS, self.ROWS))
        self.assertEqual(commands_frame._drawable_slots(self.COLS, self.ROWS), [])

    def _tmux(self, *args: str, env=None) -> subprocess.CompletedProcess:
        return subprocess.run(["tmux", "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20, env=env)

    def _session(self, name: str, env: dict) -> str:
        started = self._tmux("-f", "/dev/null", "new-session", "-d", "-s", name,
                             "-x", str(self.COLS), "-y", str(self.ROWS),
                             "-P", "-F", "#{pane_id}", "--", "sleep", "600", env=env)
        self.assertEqual(started.returncode, 0, started.stderr)
        return started.stdout.strip()

    def _open(self, text: str):
        with mock.patch.object(claude_code.ClaudeCodeHarness, "binary", str(self.recorder)), \
                mock.patch.object(commands_frame, "_spawn_gather"), \
                mock.patch("sys.stdout.isatty", return_value=True), \
                mock.patch("sys.stdin.isatty", return_value=False):
            return commands_frame.open_in_background("beta", caller="alpha.1",
                                                     first_message=text)

    def _received(self, pane: str) -> list[str]:
        """The recorder's argv — failing as soon as tmux no longer lists the pane, because
        `display-message` on a pane that has gone answers rc 0 with an empty line (#959)."""
        record = self.records / (pane.lstrip("%") + ".json")
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if record.is_file():
                return json.loads(record.read_text())
            listed = self._tmux("list-panes", "-a", "-F", "#{pane_id}")
            if pane not in listed.stdout.split():
                if record.is_file():
                    return json.loads(record.read_text())
                self.fail(f"the harness in {pane} ended without recording its argv")
            time.sleep(0.05)
        self.fail(f"the harness in {pane} never recorded its argv")

    def test_opening_a_background_chat_leaves_the_sessions_current_window_where_it_was(self):
        session = self._tmux("display-message", "-p", "-t", self.target_pane,
                             "#{session_id}").stdout.strip()
        before = self._tmux("display-message", "-p", "-t", session, "#{window_id}")
        self.assertEqual(before.returncode, 0, before.stderr)
        opened = self._open(self.BRIEF)
        self.assertTrue(opened.ok, opened.message)
        pane = state.harness_pane(opened.chat)
        self.assertEqual(self._received(pane), [self.BRIEF])
        after = self._tmux("display-message", "-p", "-t", session, "#{window_id}")
        self.assertEqual(after.stdout.strip(), before.stdout.strip(),
                         "opening a chat in the background moved the session's window")
        windows = self._tmux("list-windows", "-t", session, "-F", "#{window_id}")
        self.assertEqual(len(windows.stdout.split()), 2,
                         "the chat was not opened as a window in the target's session")

    def test_a_first_message_at_charters_bound_still_fits_under_tmuxs_limit(self):
        """`FIRST_MESSAGE_MAX_BYTES` is charter's own bound, not a prediction of tmux's
        limit (ADR 0009). What this asks is that the bound leaves room: a first message
        exactly at it, with the names, directory and identity a real launch carries beside
        it, still starts and still arrives whole."""
        text = "x" * (commands_frame.FIRST_MESSAGE_MAX_BYTES - 2) + " y"
        opened = self._open(text)
        self.assertTrue(opened.ok, opened.message)
        self.assertEqual(self._received(state.harness_pane(opened.chat)), [text])


if __name__ == "__main__":
    unittest.main()
