"""Integration tests against a REAL tmux server — the properties `commands_frame.py`'s
exit-code hooks rest on, which a mock cannot observe: tmux's own hook-array replacement
semantics, and whether a session-scoped `set-environment` value actually reaches a
`run-shell`-spawned shell later. Both were hand-verified during development; this module
turns that hand-verification into something that reruns and fails loudly if tmux's own
behaviour (or this code) ever drifts.

A SEPARATE module from `tests/test_frame_launcher.py` on purpose, so that module's own
"no tmux, ever" promise stays literally true — this is the one place in the suite
allowed to start a real server. Skipped when tmux is absent
(`unittest.skipUnless(shutil.which("tmux"), ...)`), per this plane's Global Constraints:
"Tests never require tmux. Anything needing the binary skips when it is absent, and
asserts on probed capability rather than a version string." Every assertion below reads
tmux's own reported state (`show-hooks`, `display-message`, a file a real hook wrote) —
never a version string.

Every test gets its own tmux SESSION (hooks are per-pane, so a shared session would let
one test's hook leak into another's pane) on the ONE socket this module owns, and every
test kills that socket's server on the way out via `addCleanup` — so a failing test
can't leak a stale socket file into `/private/tmp/tmux-<uid>/` any more than a passing
one does.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from charter import commands_frame

_HAS_TMUX = shutil.which("tmux") is not None

#: Unique per test PROCESS, not merely per class — a socket left behind by an earlier,
#: interrupted run must never collide with (or be mistaken for) this one's.
SOCKET = f"charter-integration-test-{os.getpid()}"


def _tmux(*args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=10)


def _run(cmd: list[str]) -> subprocess.CompletedProcess:
    """Run one of `commands_frame`'s own argv-building functions' output — never a
    hand-retyped command — so this module tests the exact bytes the launcher sends."""
    return subprocess.run(cmd, capture_output=True, text=True, timeout=10)


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class TmuxIntegration(unittest.TestCase):
    def setUp(self) -> None:
        self.addCleanup(self._teardown_socket)
        self._pane_counter = 0

    def _teardown_socket(self) -> None:
        """`kill-server` ends the SERVER but does not remove its own socket FILE —
        confirmed by hand: `list-sessions` on it afterward correctly reports "no server
        running", yet the file stays in `/tmp/tmux-<uid>/`. A cleanup that only killed
        the server would still leak one stale entry per test run — which is exactly how
        this module's own hand-verification sessions left 52 of them behind before this
        fix. tmux computes this path itself from `-L SOCKET`; matched here rather than
        asked of tmux because there is no query command for it, only observed behaviour
        (`/tmp/tmux-<getuid()>/<socket name>` on every platform this repo runs on)."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _new_pane(self) -> str:
        """A fresh session on `SOCKET`, `remain-on-exit` armed, its one pane's id."""
        self._pane_counter += 1
        name = f"p{self._pane_counter}"
        r = _tmux("new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        # Global on this socket's server — cheap to repeat per pane, and every frame
        # wants it regardless (see `commands_frame._PLACEHOLDER_CONF`'s own docstring).
        _tmux("set", "-g", "remain-on-exit", "on")
        return name, r.stdout.strip()

    # -- 1. Append and the order trap ---------------------------------------------- #

    def test_the_write_hook_must_be_installed_before_the_teardown_hook(self):
        """The property `_pane_died_teardown_hook_argv`'s own docstring names as the
        entire reason for `cmd_launch`'s call order: an UNINDEXED `set-hook -p -t <pane>
        pane-died '<action>'` call does not overwrite index 0 of an existing hook array
        — it REPLACES THE WHOLE ARRAY. Installed write-then-teardown (the order
        `cmd_launch` actually uses), both indices survive. Installed teardown-then-write
        (the trap), the teardown hook is silently deleted the moment the write hook
        lands — which is precisely the mutation that reintroduced the original hang
        when the two `cmd_launch` calls were swapped."""
        _, pane_correct = self._new_pane()
        write = commands_frame._pane_died_write_hook_argv(socket=SOCKET, harness_pane=pane_correct)
        teardown = commands_frame._pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=pane_correct)
        self.assertEqual(_run(write).returncode, 0)
        self.assertEqual(_run(teardown).returncode, 0)
        hooks = _tmux("show-hooks", "-p", "-t", pane_correct).stdout
        self.assertIn("pane-died[0]", hooks)
        self.assertIn("pane-died[1]", hooks)

        _, pane_wrong = self._new_pane()
        write2 = commands_frame._pane_died_write_hook_argv(socket=SOCKET, harness_pane=pane_wrong)
        teardown2 = commands_frame._pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=pane_wrong)
        self.assertEqual(_run(teardown2).returncode, 0)
        self.assertEqual(_run(write2).returncode, 0)
        hooks_wrong = _tmux("show-hooks", "-p", "-t", pane_wrong).stdout
        self.assertIn("pane-died[0]", hooks_wrong)
        self.assertNotIn("pane-died[1]", hooks_wrong,
                         "installing the write hook AFTER the teardown hook must not "
                         "silently wipe the teardown hook out — this array-replacement "
                         "behaviour is the entire reason cmd_launch's install order is "
                         "load-bearing")

    # -- 2. Path delivery and injection ---------------------------------------------- #

    def test_the_exit_status_path_round_trips_a_hostile_plane_path(self):
        """`set-environment` (a single argv value, no shell involved) is how the
        exit-status path reaches the write hook's shell — verified here against a path
        containing a space, a literal `'`, and a `$(touch ...)` injection attempt all at
        once: the file at that exact path must hold the harness's real exit code, and
        nothing embedded in the path may execute."""
        session, pane = self._new_pane()
        tmp = tempfile.mkdtemp(prefix="charter-integ-inj-")
        self.addCleanup(shutil.rmtree, tmp, True)
        canary = os.path.join(tmp, "canary")
        hostile_dir = os.path.join(tmp, "it's a $(touch " + canary + ") dir")
        os.makedirs(hostile_dir, exist_ok=True)
        status_path = os.path.join(hostile_dir, "exit")

        env_cmd = commands_frame._exit_path_env_argv(socket=SOCKET, session=session,
                                                      status_path=status_path)
        self.assertEqual(_run(env_cmd).returncode, 0)
        write_cmd = commands_frame._pane_died_write_hook_argv(socket=SOCKET, harness_pane=pane)
        self.assertEqual(_run(write_cmd).returncode, 0)
        teardown_cmd = commands_frame._pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=pane)
        self.assertEqual(_run(teardown_cmd).returncode, 0)

        _tmux("send-keys", "-t", pane, "exit 42", "Enter")
        time.sleep(1)

        self.assertFalse(os.path.exists(canary),
                         "the $(touch ...) inside the plane path must never execute")
        with open(status_path) as f:
            self.assertEqual(f.read().strip(), "42")

    # -- 3. Signal death -------------------------------------------------------------- #

    def test_a_signal_death_writes_the_unknown_death_sentinel_not_an_empty_line(self):
        """Measured against tmux 3.7c: a pane killed by SIGKILL reports `#{pane_dead}`
        `1` with `#{pane_dead_status}` EMPTY, not a number — `display-message` is
        checked directly to confirm that is still what THIS tmux does, not merely
        assumed. The write hook's own `${v:-N}` fallback must turn that empty value into
        `_UNKNOWN_DEATH_CODE` at the point of writing, so the file it produces is always
        a parseable integer; `state.exit_code` cannot parse an empty line and would
        silently read it back as "nothing was ever recorded"."""
        session, pane = self._new_pane()
        tmp = tempfile.mkdtemp(prefix="charter-integ-sig-")
        self.addCleanup(shutil.rmtree, tmp, True)
        status_path = os.path.join(tmp, "exit")

        env_cmd = commands_frame._exit_path_env_argv(socket=SOCKET, session=session,
                                                      status_path=status_path)
        self.assertEqual(_run(env_cmd).returncode, 0)
        write_cmd = commands_frame._pane_died_write_hook_argv(socket=SOCKET, harness_pane=pane)
        self.assertEqual(_run(write_cmd).returncode, 0)

        _tmux("send-keys", "-t", pane, "kill -9 $$", "Enter")
        time.sleep(1)

        dead, _, status = _tmux("display-message", "-p", "-t", pane,
                                "#{pane_dead}:#{pane_dead_status}").stdout.strip().partition(":")
        self.assertEqual(dead, "1", "the pane must be confirmed dead before this test "
                                    "means anything")
        self.assertEqual(status, "", "this pins that tmux itself still reports an EMPTY "
                                     "status for a signal death, not a negative number "
                                     "— the wrong premise an earlier version of this "
                                     "suite assumed")

        with open(status_path) as f:
            content = f.read().strip()
        self.assertEqual(content, str(commands_frame._UNKNOWN_DEATH_CODE),
                         f"expected the sentinel, got {content!r} (an empty string here "
                         f"is exactly the bug: state.exit_code cannot parse it)")


if __name__ == "__main__":
    unittest.main()
