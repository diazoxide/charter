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

**One tmux server per case, never one per class.** Both cases of a class used to share a
socket name, and on CI (pull_request run 34559973556, tmux 3.4) the second case's first
`new-session` came 7 ms after the first case's `kill-server` and failed with `server exited
unexpectedly`: the old server, already told to exit, was still accepting on that socket. The
other real-tmux modules draw a fresh name per case from a counter for the same reason. And a
session that tmux will not start fails with tmux's own account of why (:meth:`_diagnosis`),
so a flake on a runner nobody can log into names its cause instead of just its symptom.

**What the #959 tests already hold, and what this adds.** `tests/test_a_harness_argument_ending_
in_a_semicolon_arrives_whole.py` measures the builders byte for byte and
`tests/test_a_launch_too_long_for_tmux_says_so.py` tmux's command limit. This file sends a
brief through Task 1's own path — `first_message_argv`, the `Opening`, the launcher — and
checks that it arrives whole, that a first message at charter's own bound still fits under
tmux's limit with the identity a real launch carries, and that no window moves: once in a
window too small for panels (M3), and once with the panels really split and real clients
attached (:class:`ABackgroundChatWithItsPanelsMovesNoAttachedClient`).
"""

from __future__ import annotations

import fcntl
import itertools
import json
import os
import shutil
import struct
import subprocess
import sys
import termios
import time
import unittest
from contextlib import ExitStack
from unittest import mock

from charter import commands_frame, config
from charter.frame import layout, state, tmuxctl
from charter.harness import claude_code

from tests import _tmuxreap, _tmuxsocket
from tests._isolation import PersonaIso, make_plane, no_update_check_in

_HAS_TMUX = shutil.which("tmux") is not None

#: A fresh number for every case's socket name — see the module docstring for the CI run that
#: showed why a name must not outlive its case.
_SERVERS = itertools.count()

#: Terminal types to hand an attached client, the list
#: `tests/test_a_real_click_on_a_real_tab_bar_switches.py` keeps.
_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))


def _await(predicate, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()


class _TwoChatsOnARealServer(PersonaIso):
    """`alpha.1` and `beta.1`, each the first window of a real session on a private socket,
    recorded on disk the way a launcher records them, and a recorder standing in for the
    harness a background open starts."""

    #: The size each session starts at.
    COLS, ROWS = 80, 24
    #: This class's `tests._tmuxreap` slug, numbered per case.
    SLUG = "handoff"

    def setUp(self):
        super().setUp()
        v = tmuxctl.version()
        if v is None or v < tmuxctl.FLOOR:
            self.skipTest(f"the frame's floor is tmux {tmuxctl.FLOOR[0]}.{tmuxctl.FLOOR[1]};"
                          f" this machine has {v}")
        make_plane(self)
        #: The tmux binary this case measures, resolved once. Every command the case sends
        #: itself, and the client it attaches, runs this file; `tmuxctl`'s own calls resolve
        #: the same `$PATH`, which is how a run is pointed at the 3.2 floor binary.
        self.tmux = shutil.which("tmux")
        self.socket = _tmuxreap.name(f"{self.SLUG}-{next(_SERVERS)}")
        self.enterContext(mock.patch.object(commands_frame, "SOCKET", self.socket))
        self.addCleanup(subprocess.run, [self.tmux, "-L", self.socket, "kill-server"],
                        capture_output=True, timeout=20)
        #: Where the server this case starts writes its `-v` log, read only on a failure.
        self.tmux_logs = self.tmp / "tmux-logs"
        self.tmux_logs.mkdir()
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
        #: Each workspace's recorded chat pane.
        self.panes: dict[str, str] = {}
        for ws in ("alpha", "beta"):
            fid = f"{ws}.1"
            pane = self._session(ws, server_env)
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
            self.panes[ws] = pane

    def _tmux(self, *args: str, env=None) -> subprocess.CompletedProcess:
        return subprocess.run([self.tmux, "-L", self.socket, *args], capture_output=True,
                              text=True, timeout=20, env=env)

    def _session(self, name: str, env: dict) -> str:
        """A session running `sleep`, its first pane's id — or a failure naming tmux's cause.

        `-v` so the server this starts keeps a log, run from :attr:`tmux_logs` because that
        is where tmux writes it; read only when something went wrong."""
        argv = [self.tmux, "-v", "-L", self.socket, "-f", "/dev/null", "new-session", "-d",
                "-s", name, "-x", str(self.COLS), "-y", str(self.ROWS),
                "-P", "-F", "#{pane_id}", "--", "sleep", "600"]
        existed = os.path.exists(_tmuxsocket.socket_path(self.socket))
        started = subprocess.run(argv, capture_output=True, text=True, timeout=20, env=env,
                                 cwd=self.tmux_logs)
        if started.returncode != 0:
            self.fail(self._diagnosis(started, argv, env, socket_existed=existed))
        return started.stdout.strip()

    def _diagnosis(self, done: subprocess.CompletedProcess, argv: list[str], env: dict,
                   *, socket_existed: bool) -> str:
        """Everything a failed session start on a runner nobody can log into needs to say."""
        named = {k: env.get(k) for k in ("TERM", "SHELL", "HOME", "TMUX_TMPDIR", "LANG",
                                         "LC_ALL", "PATH")}
        version = subprocess.run([self.tmux, "-V"], capture_output=True,
                                 text=True).stdout.strip()
        logs = []
        for log in sorted(self.tmux_logs.glob("tmux-*.log")):
            lines = log.read_text(errors="replace").splitlines()
            logs.append(f"--- {log.name}, last 40 of {len(lines)} lines\n"
                        + "\n".join(lines[-40:]))
        return (f"tmux would not start a session: rc {done.returncode}, stderr "
                f"{done.stderr.strip()!r}\n{version}\nargv: {argv}\n"
                f"socket {_tmuxsocket.socket_path(self.socket)} existed before the call: "
                f"{socket_existed}\nenv: {named}\n" + ("\n".join(logs) or "no tmux log"))

    def _stand_ins(self) -> list:
        """Everything an open runs with here beyond the real launcher and the real tmux."""
        return [mock.patch.object(claude_code.ClaudeCodeHarness, "binary", str(self.recorder)),
                mock.patch.object(commands_frame, "_spawn_gather"),
                mock.patch("sys.stdout.isatty", return_value=True),
                mock.patch("sys.stdin.isatty", return_value=False)]

    def _open(self, text: str, *, ws: str = "beta"):
        with ExitStack() as stack:
            for stand_in in self._stand_ins():
                stack.enter_context(stand_in)
            return commands_frame.open_in_background(ws, caller="alpha.1",
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


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ABackgroundChatReallyStartsOnItsBrief(_TwoChatsOnARealServer, unittest.TestCase):

    #: A brief that tmux's own parser and a shell would each mangle if charter joined, split
    #: or failed to escape it: a trailing `;` (#957, carried through by #959's
    #: `tmuxctl.verbatim`), a blank line, quotes, command substitution and a tmux format.
    BRIEF = 'fix the widget;\n\nit says "$(echo boom)" and `x` at #{session_name};'

    #: Small enough that no panel is drawable, so no panel process starts (asserted below
    #: rather than assumed). The class after this one draws them.
    COLS, ROWS = 20, 6
    SLUG = "handoff-argv"

    def setUp(self):
        super().setUp()
        self.assertEqual(commands_frame._window_size(self.socket, self.panes["alpha"]),
                         (self.COLS, self.ROWS))
        self.assertEqual(commands_frame._drawable_slots(self.COLS, self.ROWS), [])

    def test_opening_a_background_chat_leaves_the_sessions_current_window_where_it_was(self):
        session = self._tmux("display-message", "-p", "-t", self.panes["beta"],
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


@unittest.skipUnless(_HAS_TMUX, "no tmux on this machine")
class ABackgroundChatWithItsPanelsMovesNoAttachedClient(_TwoChatsOnARealServer,
                                                        unittest.TestCase):
    """Real panels and real attached clients: a background open moves nobody.

    `docs/frame.md` promises both that no client moves and that the new window gets its
    panels, and the panels are `split-window`s with no `-d` (`layout.panel_argvs`) — so a
    window too small to draw any, as above, never asked the question. This one is big
    enough that `_draw_panels` really splits.

    Two views are compared before and after an open into a workspace somebody is looking
    at, and an open into the caller's own. **Every session's current window and that
    window's active pane**, which is what any client of the session is shown and which needs
    no client to ask. **Every attached client's own view**, from a real client on each
    workspace's session; where no client will attach on a machine, that half is a skipped
    subtest naming the terminal types tried, and the first half has still run.

    `layout.panel_command` is stood in with `sleep`: the splits are real, but no pane runs
    `charter panel`, which would be a child charter this test cannot see, spending a version
    check and a gather against whatever plane it resolves.
    """

    COLS, ROWS = 120, 40
    SLUG = "handoff-panels"

    def setUp(self):
        super().setUp()
        no_update_check_in(config.ROOT)
        #: The terminal types a client would not attach with, for the skip that says so.
        self.unattached: list[str] = []
        #: The client attached to each workspace's session, where one would attach.
        self.clients: dict[str, str] = {}
        for ws in ("alpha", "beta"):
            client = self._attach(ws)
            if client is not None:
                self.clients[ws] = client
        size = commands_frame._window_size(self.socket, self.panes["alpha"])
        self.slots = commands_frame._drawable_slots(*size)
        self.assertTrue(self.slots, f"nothing is drawable at {size}, so no panel is split")

    def _stand_ins(self) -> list:
        return [*super()._stand_ins(),
                mock.patch.object(layout, "panel_command", return_value=["sleep", "600"])]

    def _client_names(self, session: str) -> list[str]:
        return self._tmux("list-clients", "-t", session, "-F", "#{client_name}").stdout.split()

    def _attach(self, session: str) -> str | None:
        """A real client on *session*, on a pty sized to the window, and its tmux name — or
        ``None`` where no terminal type would attach.

        **The client is :attr:`tmux`, the binary this case measures, by its absolute path**:
        a bare `tmux` resolved again here could be a different binary from the server the case
        is asking about.

        Started through `subprocess.Popen` with the pty as its three streams, and not as a
        `pty.fork` child that `os.execvp`s, because `tests/test_plane_spawn_guard.py` requires
        every exec-family call in the tree to be one it has written down. That static guard is
        the whole of what covers this call: the runtime `_planeguard` does not resolve a bare,
        non-interpreter program through `$PATH`, so it would not refuse a charter posing as
        tmux. The client needs no controlling terminal — tmux's server opens the client's tty
        by name — measured attaching this way on tmux 3.7c and 3.2 on macOS and 3.4 on Linux."""
        for term in _TERM_CANDIDATES:
            master, slave = os.openpty()
            fcntl.ioctl(slave, termios.TIOCSWINSZ,
                        struct.pack("HHHH", self.ROWS, self.COLS, 0, 0))
            client = subprocess.Popen([self.tmux, "-L", self.socket, "attach", "-t", session],
                                      stdin=slave, stdout=slave, stderr=slave,
                                      env=dict(os.environ, TERM=term),
                                      start_new_session=True)
            os.close(slave)
            self.addCleanup(self._reap, client, master)
            if _await(lambda: bool(self._client_names(session)), timeout=10.0):
                return self._client_names(session)[0]
            self.unattached.append(term)
        return None

    @staticmethod
    def _reap(client: subprocess.Popen, master: int) -> None:
        """The client this case started, by the handle that started it."""
        for step in (client.kill, lambda: client.wait(timeout=10), lambda: os.close(master)):
            try:
                step()
            except (OSError, subprocess.TimeoutExpired):
                pass

    def _current(self) -> dict[str, list[str]]:
        """Each session's ``[current window, its active pane]`` — what any client of it is
        shown, attached or not."""
        listed = self._tmux("list-panes", "-a", "-F",
                            "#{session_name}\t#{window_active}\t#{pane_active}\t"
                            "#{window_id}\t#{pane_id}")
        current = {}
        for row in listed.stdout.splitlines():
            session, window_active, pane_active, window, pane = row.split("\t")
            if window_active == "1" and pane_active == "1":
                current[session] = [window, pane]
        return current

    def _views(self) -> dict[str, list[str]]:
        """Each attached client's ``[session, current window, active pane]``, by client name.

        Asked of `list-clients`, never `display-message -c <client>`. Measured with two real
        clients: on tmux 3.7c `display-message -c` answers the server's current session for
        every client alike, and at the 3.2 floor it fails with rc 1 — neither says where one
        client is looking. `list-clients` reports each client's own, on both, and follows a
        `select-window` that moves it."""
        listed = self._tmux("list-clients", "-F",
                            "#{client_name}\t#{session_name}\t#{window_id}\t#{pane_id}")
        rows = [row.split("\t") for row in listed.stdout.splitlines() if row]
        return {row[0]: row[1:] for row in rows}

    def _open_moves_no_client(self, ws: str) -> None:
        sessions_before = self._current()
        self.assertEqual({name: view[1] for name, view in sessions_before.items()},
                         self.panes, f"a session is not on its own chat: {sessions_before}")
        clients_before = self._views()
        opened = self._open("look at the widget please", ws=ws)
        self.assertTrue(opened.ok, opened.message)
        drawn = self._tmux("list-panes", "-t", state.harness_pane(opened.chat), "-F",
                           "#{pane_id}").stdout.split()
        self.assertEqual(len(drawn), 1 + len(self.slots),
                         f"the new chat's panels were not split, so this measured nothing "
                         f"about them: {drawn}")
        self.assertEqual(self._current(), sessions_before,
                         "a background open moved a session's current window or pane")
        with self.subTest(view="attached clients"):
            if sorted(self.clients) != ["alpha", "beta"]:
                self.skipTest(f"no real client would attach on this machine with TERM "
                              f"{self.unattached}; each session's current window and pane "
                              f"were still compared above")
            for name, client in self.clients.items():
                view = clients_before.get(client, [])
                self.assertEqual([view[0], view[2]] if len(view) == 3 else view,
                                 [name, self.panes[name]],
                                 f"the client on {name!r} is not on its own chat: {view!r}")
            self.assertEqual(self._views(), clients_before,
                             "a background open moved an attached client")

    def test_an_open_into_a_workspace_somebody_is_looking_at_moves_no_client(self):
        self._open_moves_no_client("beta")

    def test_an_open_into_the_callers_own_workspace_moves_no_client(self):
        self._open_moves_no_client("alpha")


if __name__ == "__main__":
    unittest.main()
