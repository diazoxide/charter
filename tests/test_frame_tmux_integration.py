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
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, hooks, todos
from charter.frame import notify, state

from tests._isolation import PersonaIso, run_hook

_HAS_TMUX = shutil.which("tmux") is not None

#: Unique per test PROCESS, not merely per class — a socket left behind by an earlier,
#: interrupted run must never collide with (or be mistaken for) this one's.
SOCKET = f"charter-integration-test-{os.getpid()}"

#: `tests/_isolation.py`'s `PersonaIso` isolates the paths a Python IMPORT of `charter`
#: reads inside THIS process; `PanelIntegration` below needs a SUBPROCESS to see the
#: same throwaway plane, which only `$CHARTER_ROOT`/`$CHARTER_WORKSPACE` can hand it
#: across a process boundary. `charter panel` never dies over a missing repo, `.git`,
#: or any other plane furniture (this module's own `setUp` creates nothing beyond one
#: empty `charter.toml` marker), so nothing here needs `PersonaIso`'s SIBLING classes
#: (`ReportIso`, worktree scaffolding) — just its config-path redirection.
_REPO_ROOT = Path(__file__).resolve().parent.parent


def _tmux(*args: str, env: dict | None = None) -> subprocess.CompletedProcess:
    """*env* is `None` by every existing call site (inherit this process's own
    environment, unchanged) — `PanelIntegration` is the one caller that needs a
    DIFFERENT environment for a `new-session` call, so tmux hands its spawned pane a
    throwaway plane's `$CHARTER_ROOT` rather than this test process's real one."""
    return subprocess.run(["tmux", "-L", SOCKET, *args], capture_output=True, text=True,
                          timeout=10, env=env)


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

    # -- 4. Resize redistribution ----------------------------------------------------- #

    def test_a_fixed_size_panels_dimension_survives_a_real_window_resize(self):
        """Cross-task fix round, item 3: tmux's own layout engine redistributes EVERY
        pane proportionally on ANY resize, `-l size` notwithstanding — hand-verified
        against this exact tmux binary during development: a 120x30 frame grown to
        200x50 stretched a one-row panel to 8 rows, and only snapped back to 1 row on
        the way down because that particular shrink happened to be an exact round trip
        of the same grow. This drives `commands_frame._resize_hook_argv` — the real
        function, not a hand-retyped `resize-pane` — through three resizes (grow,
        shrink smaller than the original, grow past the first grow) to rule out "only
        works for a round trip" as the actual fix."""
        r = _tmux("new-session", "-d", "-s", "rsz", "-x", "120", "-y", "30",
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        top = _tmux("split-window", "-t", harness_pane, "-v", "-b", "-l", "1",
                   "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(top.returncode, 0, top.stderr)
        top_pane = top.stdout.strip()

        hook_cmd = commands_frame._resize_hook_argv(socket=SOCKET, harness_pane=harness_pane,
                                                    panes={"top": top_pane})
        self.assertEqual(_run(hook_cmd).returncode, 0, "installing the resize hook failed")

        for cols, rows in ((200, 50), (90, 25), (300, 100)):
            r = _tmux("resize-window", "-t", "rsz", "-x", str(cols), "-y", str(rows))
            self.assertEqual(r.returncode, 0, r.stderr)
            time.sleep(0.3)
            height = _tmux("display-message", "-p", "-t", top_pane,
                           "#{pane_height}").stdout.strip()
            self.assertEqual(height, "1",
                             f"the panel drifted to {height} rows after resizing to "
                             f"{cols}x{rows} — the hook did not hold")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class PanelIntegration(PersonaIso, unittest.TestCase):
    """`charter panel` end to end, closing the exact gap that let a launcher spawning
    real panel panes (Task 6) and a `charter panel` command (Task 7) ship across two
    whole tasks with a fully green suite even if the two had never actually agreed on
    an argv shape — every OTHER test in this file, and in `test_frame_launcher.py`,
    either builds argv without running it or runs tmux commands without a real
    `charter panel` process on the other end. This is the one place both are true at
    once: a real tmux pane running the REAL subprocess.

    A THROWAWAY plane, not the real one this repo's own suite runs from — `todos.add`
    below writes state, and PersonaIso's isolation (this class's own base) only covers
    what a Python IMPORT of `charter` reads in THIS process; the SUBPROCESS needs the
    same throwaway plane handed across the process boundary via `$CHARTER_ROOT`
    (`root.py`'s own override, which requires a real `charter.toml` marker to exist at
    that path or it silently falls back to the subprocess's own cwd instead) and
    `$CHARTER_WORKSPACE` (`workspace.resolve`'s own override, checked before any
    cwd-tree detection) — never the developer's real ``~/.charter`` or workspace data.
    """

    def setUp(self) -> None:
        super().setUp()
        self.addCleanup(_tmux, "kill-server")
        self.addCleanup(lambda: (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET)
                        .unlink(missing_ok=True))
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)  # let it derive under CHARTER_ROOT, like this
                                            # process's own PersonaIso-isolated config

    def test_a_bottom_panel_draws_and_repaints_on_a_real_state_bump(self):
        """Minimal shape: a real pane running `charter panel bottom --session <fid>`
        must (1) show real content rather than dying at startup — `capture-pane`
        containing "todo" kills that, since a dead pane under `remain-on-exit` shows
        nothing of the sort — and (2) actually repaint when the version file it polls
        changes, not just once at launch — a fresh todo plus a real `state.bump` must
        change what `capture-pane` reports within a few polls of `panel.TICK`."""
        fid = state.frame_id("panel-integ", os.getpid())
        argv = [sys.executable, "-m", "charter", "panel", "bottom", "--session", fid]
        r = _tmux("new-session", "-d", "-s", "panel-live", "-x", "40", "-y", "5",
                  "-c", str(_REPO_ROOT), "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)

        time.sleep(1)
        first = _tmux("capture-pane", "-p", "-t", "panel-live").stdout
        self.assertIn("todo", first, f"the pane never drew its content:\n{first!r}")
        self.assertEqual(_tmux("display-message", "-p", "-t", "panel-live",
                               "#{pane_dead}").stdout.strip(), "0",
                         "the pane died at startup — exactly the launcher-days-of-a-"
                         "green-suite gap this test exists to close")

        todos.add("demo", "a fresh todo the live panel should pick up")
        state.bump(fid)

        deadline = time.monotonic() + 3
        changed = False
        while time.monotonic() < deadline:
            if _tmux("capture-pane", "-p", "-t", "panel-live").stdout != first:
                changed = True
                break
            time.sleep(0.2)
        self.assertTrue(changed, f"the panel never repainted after a real state.bump; "
                                 f"still showing:\n{first!r}")
        self.assertEqual(_tmux("display-message", "-p", "-t", "panel-live",
                               "#{pane_dead}").stdout.strip(), "0",
                         "the pane died sometime after its first paint")

    def test_a_real_hook_call_repaints_a_live_panel_without_a_direct_state_bump(self):
        """Closes the gap the test above leaves open: that test calls `state.bump`
        directly, which proves the PANEL half of the liveness story (Task 6/7) but says
        nothing about the HOOK half (Task 8) — `notify.plane_changed`, wired into
        `charter/hooks.py`'s `sessionstart`/`userpromptsubmit`/`posttooluse` handlers,
        is the thing that is actually supposed to call `state.bump` in a running
        session. This drives a REAL hook handler (`hooks.posttooluse`, via
        `tests._isolation.run_hook` — the same stdin-JSON shape Claude Code itself
        feeds it) with `$CHARTER_SESSION_ID` set to the live panel's own frame id, and
        watches the SAME pane used above repaint from that call alone — no
        `state.bump` call anywhere in this test. `notify._last` is reset first so a
        debounce window left over from another test in this process can't mask a
        broken hook wiring the way `test_frame_liveness.py`'s own docstring warns
        about."""
        fid = state.frame_id("panel-hook-integ", os.getpid())
        argv = [sys.executable, "-m", "charter", "panel", "bottom", "--session", fid]
        r = _tmux("new-session", "-d", "-s", "panel-hook-live", "-x", "40", "-y", "5",
                  "-c", str(_REPO_ROOT), "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)

        time.sleep(1)
        first = _tmux("capture-pane", "-p", "-t", "panel-hook-live").stdout
        self.assertIn("todo", first, f"the pane never drew its content:\n{first!r}")
        self.assertEqual(_tmux("display-message", "-p", "-t", "panel-hook-live",
                               "#{pane_dead}").stdout.strip(), "0",
                         "the pane died at startup")

        todos.add("demo", "a todo a real hook call should surface")
        notify._last["at"] = 0.0
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid}):
            out = run_hook(hooks.posttooluse, {
                "tool_name": "Read",
                "tool_input": {"file_path": "/nonexistent"},
                "session_id": "hook-integ-session",
            })
        self.assertIsNone(out, "posttooluse should emit nothing for a plain Read")

        deadline = time.monotonic() + 3
        changed = False
        while time.monotonic() < deadline:
            if _tmux("capture-pane", "-p", "-t", "panel-hook-live").stdout != first:
                changed = True
                break
            time.sleep(0.2)
        self.assertTrue(changed, f"the panel never repainted after a real "
                                 f"hooks.posttooluse() call; still showing:\n{first!r}")
        self.assertEqual(_tmux("display-message", "-p", "-t", "panel-hook-live",
                               "#{pane_dead}").stdout.strip(), "0",
                         "the pane died sometime after its first paint")

    def test_a_corrupt_version_file_does_not_kill_a_live_panel(self):
        """Fix round 2, item 1: a non-UTF-8 `version` file used to reach a real panel's
        run loop as an uncaught `UnicodeDecodeError` and kill the pane — confirmed by
        hand before this fix (`#{pane_dead}` reported `1`, status `1`, the traceback
        visible in the pane itself). `remain-on-exit` is deliberately NOT armed for
        this session (see `setUp`): a crash here means the whole SESSION vanishes, not
        just the pane, so the `display-message` calls below would fail outright
        ("session not found") rather than merely answering `pane_dead=1` — a stronger
        signal than checking the pane alone would give.
        """
        fid = state.frame_id("panel-integ-corrupt", os.getpid())
        argv = [sys.executable, "-m", "charter", "panel", "bottom", "--session", fid]
        r = _tmux("new-session", "-d", "-s", "panel-corrupt", "-x", "40", "-y", "5",
                  "-c", str(_REPO_ROOT), "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)

        time.sleep(1)
        alive = _tmux("display-message", "-p", "-t", "panel-corrupt", "#{pane_dead}")
        self.assertEqual(alive.returncode, 0, "the panel never even started")
        self.assertEqual(alive.stdout.strip(), "0")

        frame_dir = state.frame_dir(fid, create=True)
        (frame_dir / "version").write_bytes(b"\xff\xfe not valid utf-8 \x80\x81")

        time.sleep(1)
        after = _tmux("display-message", "-p", "-t", "panel-corrupt", "#{pane_dead}")
        self.assertEqual(after.returncode, 0,
                         "the session vanished — the panel crashed reading the "
                         "corrupt version file, and remain-on-exit is not armed here "
                         f"(stderr: {after.stderr!r})")
        self.assertEqual(after.stdout.strip(), "0",
                         "the panel died reading a corrupt version file")


if __name__ == "__main__":
    unittest.main()
