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
import pty
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, hooks, todos
from charter.frame import menu, notify, state

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


def _kill_pid(pid_str: str) -> None:
    """SIGKILL one process directly, tolerating it already being gone.

    `tmux kill-server` is a fire-and-forget signal TO THE SERVER, not a wait for it to
    actually finish tearing down and reaping every pane's process — confirmed by hand
    (`PanelIntegration`'s three tests, run back to back on ONE shared socket, left THREE
    separate orphaned `tmux` server processes behind, each with PPID 1 and each still
    holding its own live `python3 -m charter panel ...` child, even though every test's
    own `addCleanup(_tmux, "kill-server")` had already run and reported success — the
    review that found this counted 52 such orphans after a full suite run). Once a
    server has detached and reparented to init like that, no later `kill-server` call
    from a DIFFERENT test can reach it — the socket path it was using may already have
    rolled over to a fresh server for the next test's `new-session`. Killing the pane's
    OWN pid (captured via `#{pane_pid}` right after the pane is created) sidesteps the
    whole question of whether the server ever tears itself down cleanly."""
    try:
        pid = int(pid_str)
    except (TypeError, ValueError):
        return
    try:
        os.kill(pid, signal.SIGKILL)
    except OSError:
        pass  # already dead — exactly what this cleanup is trying to ensure


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

    # -- 5. Session-scoped id delivery for the hotkey menu --------------------------- #

    def test_a_second_frames_own_id_does_not_leak_the_firsts(self):
        """The exact bug `_session_id_env_argv` exists to close, verified against a
        real server rather than only asserted about: a `run-shell` with NO explicit
        `-t` of its own, fired against a session sharing this class's server, does not
        fall back to "unset" — it falls back to whatever tmux resolves as "the current
        session" absent one, which is NOT necessarily the session the caller meant
        (confirmed by hand: it tracked whichever session was created most recently, not
        the one named in the failing call). `cmd_launch` calls this for EVERY frame it
        launches, so both sessions below get their own call — mirroring that — and
        `sid-one` is seeded with a THIRD value neither call is expected to produce, so a
        version of `_session_id_env_argv` that dropped its own `-t` would show up as
        `sid-one` and `sid-two` BOTH reporting whichever session tmux's un-targeted
        default happened to prefer, not their own two distinct ids.

        `run-shell`, unlike `if-shell -F -t = ...` (`WheelUpPane`'s own idiom), needs a
        target NAME here to inherit a session's own tracked environment at all — `-t =`
        does not carry it (also verified by hand; see `conf_text`'s own docstring) — so
        this targets each session by its literal name, matching what firing from a live
        keypress inside that session's own pane does."""
        tmp = tempfile.mkdtemp(prefix="charter-integ-sid-")
        self.addCleanup(shutil.rmtree, tmp, True)
        first_out = os.path.join(tmp, "first")
        second_out = os.path.join(tmp, "second")

        r1 = _tmux("new-session", "-d", "-s", "sid-one", "-x", "80", "-y", "24", "cat",
                  env=dict(os.environ, CHARTER_SESSION_ID="neither-frames-own-id"))
        self.assertEqual(r1.returncode, 0, r1.stderr)
        r2 = _tmux("new-session", "-d", "-s", "sid-two", "-x", "80", "-y", "24", "cat")
        self.assertEqual(r2.returncode, 0, r2.stderr)

        # Both calls, in the order `cmd_launch` would issue them across two launches:
        # the OLDER frame's own id first, seeded before the newer one ever existed.
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session="sid-one")).returncode, 0)
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session="sid-two")).returncode, 0)

        _tmux("run-shell", "-t", "sid-one",
             f"env | grep CHARTER_SESSION_ID > {first_out} 2>&1 || echo NONE > {first_out}")
        _tmux("run-shell", "-t", "sid-two",
             f"env | grep CHARTER_SESSION_ID > {second_out} 2>&1 || echo NONE > {second_out}")
        time.sleep(0.5)

        with open(first_out) as f:
            self.assertEqual(f.read().strip(), "CHARTER_SESSION_ID=sid-one",
                             "the OLDER frame's own hotkey menu must still resolve its "
                             "own id after a second frame launches, not whichever "
                             "session tmux would pick as \"current\" by default")
        with open(second_out) as f:
            self.assertEqual(f.read().strip(), "CHARTER_SESSION_ID=sid-two",
                             "the second frame's own hotkey menu must resolve its own "
                             "id, not fall through to whatever the server started with")


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

    def _spawn_panel(self, session: str, fid: str) -> None:
        """One `charter panel bottom --session <fid>` pane, in a fresh session named
        *session* on this class's socket. Registers `_kill_pid` as a cleanup — see its
        own docstring for why `kill-server` alone leaves this process orphaned — using
        the pane's OWN pid (`#{pane_pid}`), captured immediately, rather than the
        session or pane id: those name tmux objects, not the OS process underneath
        them, and killing the process is the actual guarantee this method exists to
        give every caller."""
        argv = [sys.executable, "-m", "charter", "panel", "bottom", "--session", fid]
        r = _tmux("new-session", "-d", "-s", session, "-x", "40", "-y", "5",
                  "-c", str(_REPO_ROOT), "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = _tmux("display-message", "-p", "-t", session, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

    def test_a_bottom_panel_draws_and_repaints_on_a_real_state_bump(self):
        """Minimal shape: a real pane running `charter panel bottom --session <fid>`
        must (1) show real content rather than dying at startup — `capture-pane`
        containing "todo" kills that, since a dead pane under `remain-on-exit` shows
        nothing of the sort — and (2) actually repaint when the version file it polls
        changes, not just once at launch — a fresh todo plus a real `state.bump` must
        change what `capture-pane` reports within a few polls of `panel.TICK`."""
        fid = state.frame_id("panel-integ", os.getpid())
        self._spawn_panel("panel-live", fid)

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
        self._spawn_panel("panel-hook-live", fid)

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
        self._spawn_panel("panel-corrupt", fid)

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


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class MenuIntegration(PersonaIso, unittest.TestCase):
    """`charter frame-action` end to end: a REAL subprocess, spawned by a REAL `tmux
    run-shell` fired against a REAL session, resolving a menu entry whose LABEL is the
    task brief's own hostile string — through `menu.menu_argv`'s exact id-lookup
    mechanism, never a hand-simulated stand-in for it.

    `display-menu` itself needs an ATTACHED CLIENT to render to at all (confirmed by
    hand: `tmux display-menu -t <session> ...` against a session with none returns 1,
    `"no current client"`, before ever looking at an item) — every other test in this
    file, including this one, avoids attaching a client, so this drives the exact
    command text `menu_argv` embeds for ONE item directly, the same way firing it from
    inside a live `display-menu -t fid` would (see `menu_argv`'s own docstring for why
    `-t fid` is what supplies that scoping normally).

    Uses `sys.executable -m charter` rather than the bare `charter` text `menu_argv`
    itself embeds: this developer machine's own `$PATH` may resolve `charter` to an
    entirely different install than this checkout (confirmed on the machine this test
    was written on), and `PanelIntegration` above avoids the identical trap the same
    way, for `charter panel`."""

    def setUp(self) -> None:
        super().setUp()
        self.addCleanup(_tmux, "kill-server")
        self.addCleanup(lambda: (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET)
                        .unlink(missing_ok=True))
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)

    def test_a_hostile_label_never_reaches_what_the_resolved_action_runs(self):
        fid = state.frame_id("menu-integ", os.getpid())
        r = _tmux("new-session", "-d", "-s", fid, "-x", "80", "-y", "24", "cat", env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        # `_kill_pid` on the pane's OWN pid, not just `kill-server` (already in
        # `setUp`): `_kill_pid`'s own docstring names the failure mode this closes —
        # `kill-server` reliably ends the SERVER's acceptance of new commands but does
        # not reliably reap a session's still-running pane process in time, which left
        # this exact "cat" leaking past `kill-server` (confirmed by hand: `pgrep -f
        # 'tmux -L charter-integration-test'` still matched after the first version of
        # this test ran without this line).
        pid = _tmux("display-message", "-p", "-t", fid, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session=fid)).returncode, 0)

        tmp = tempfile.mkdtemp(prefix="charter-integ-menu-")
        self.addCleanup(shutil.rmtree, tmp, True)
        canary = os.path.join(tmp, "canary")
        pwned = os.path.join(tmp, "pwned")
        hostile_label = f'x" ; run-shell "touch {pwned}'

        menu.record(fid=fid, entries=[
            (hostile_label, [sys.executable, "-c",
                             f"open({canary!r}, 'w').close()"]),
        ])
        action_id = menu.build(fid)[0][1]
        self.assertRegex(action_id, r"^a[0-9]+$")

        # The exact template `menu_argv` embeds for this item, `charter` swapped for
        # `sys.executable -m charter` (see the class docstring) — everything else,
        # including the absence of any label text, matches byte for byte. `client`
        # (this test's own no-op stand-in) only affects `-c`, never the per-item
        # action text this line inspects.
        real_command = menu.menu_argv(fid, SOCKET, client="")[-1]
        self.assertNotIn("pwned", real_command,
                         "sanity: the hostile label must not have leaked into the "
                         "action text this integration test is about to run for real")
        action = f"{sys.executable} -m charter frame-action {action_id}"
        r = _tmux("run-shell", "-t", fid, action)
        self.assertEqual(r.returncode, 0, r.stderr)

        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and not os.path.exists(canary):
            time.sleep(0.2)

        self.assertTrue(os.path.exists(canary),
                        "the real, opaque-id-resolved action never ran — a real "
                        "`charter frame-action` subprocess, given the real id and a "
                        "real $CHARTER_SESSION_ID, must resolve the real stored argv")
        self.assertFalse(os.path.exists(pwned),
                         "the hostile LABEL text must never execute, even though it "
                         "sat right next to the real id in the same menu table")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class MenuFormatIntegration(unittest.TestCase):
    """The CRITICAL finding from the first review round, proved against a REAL,
    RENDERED `display-menu` — not only the argv shape `tests/test_frame_menu.py`'s
    `LabelSafety` checks. `display-menu`'s own docs: "The name and command are formats"
    — a menu item's NAME is a tmux FORMAT, not inert text, and an unescaped `#(shell
    command)` label runs the command the INSTANT tmux DRAWS the menu, no selection
    needed (confirmed by hand: the hostile row was invisible in the rendered menu —
    nothing about the menu LOOKED wrong, only the canary gave it away).

    A REAL, ATTACHED client is unavoidable here — `display-menu` refuses outright ("no
    current client") without one — which is why every OTHER test in this file avoids
    attaching one at all. `_attach_pty` is the one place in the suite that does.
    """

    def setUp(self) -> None:
        self.addCleanup(_tmux, "kill-server")
        self.addCleanup(lambda: (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET)
                        .unlink(missing_ok=True))

    def _new_session(self, fid: str) -> None:
        """A fresh `cat`-backed session named *fid*, with its pane's OWN pid registered
        for cleanup — `kill-server` alone (already in `setUp`) is documented elsewhere in
        this file (`_kill_pid`'s own docstring) as unreliable at reaping a still-running
        pane's process in time; `PanelIntegration`/`MenuIntegration` above both work
        around it the same way, and this class needs the identical fix (confirmed by
        hand: without this, three "cat" processes were still alive under `pgrep -f
        'tmux -L charter-integration-test'` after this class's own tests finished and
        `kill-server` had already run)."""
        r = _tmux("new-session", "-d", "-s", fid, "-x", "80", "-y", "24", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = _tmux("display-message", "-p", "-t", fid, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

    def _attach_pty(self, session: str) -> tuple[str, int]:
        """Forks a real `tmux attach -t session` under a pty — the one way to hand
        `display-menu` a client it will actually accept for `-c`/`-t` targeting.
        Registers the fork's own cleanup (SIGKILL, then reap — `_kill_pid` alone only
        signals; a pty-forked child is THIS process's own, so it is also this process's
        own zombie to reap, unlike a tmux-spawned pane's process, which reparents away)
        before returning the attached client's name (`#{client_name}`, read back from
        tmux itself once the attachment has had time to register) and the pty's own
        master fd — writing a KEY to the fd (not `tmux send-keys`, confirmed by hand:
        `send-keys` feeds a PANE's own input queue, which an active menu overlay never
        reads from) is the only way found to actually select a menu item from here.
        """
        pid, fd = pty.fork()
        if pid == 0:
            os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", session])

        def _reap():
            _kill_pid(str(pid))
            try:
                os.waitpid(pid, 0)
            except OSError:
                pass
            try:
                os.close(fd)
            except OSError:
                pass
        self.addCleanup(_reap)

        deadline = time.monotonic() + 3
        name = ""
        while time.monotonic() < deadline:
            out = _tmux("list-clients", "-t", session, "-F", "#{client_name}")
            name = out.stdout.strip()
            if name:
                break
            time.sleep(0.1)
        self.assertTrue(name, "the pty-attached client never registered with tmux")
        return name, fd

    def _drive_menu(self, fd: int, cmd: list[str]) -> None:
        """Bind *cmd* (a real `display-menu` invocation) to a throwaway key and press
        it through the attached pty — never issue *cmd* directly.

        Confirmed by hand, twice, with the identical *cmd*: issued DIRECTLY (via
        `subprocess.run` with a timeout, and separately via `Popen` with none) the menu
        drew its visible text but an unescaped `#(touch CANARY)` label created no
        canary either way — only firing the SAME command from a key binding did. This
        is also the more honest test regardless of that quirk: a real operator's hotkey
        never issues `display-menu` directly either, it always goes through a bind (see
        `commands_frame.cmd_menu`).

        `F2`'s own two escape sequences (`\\x1bOQ` application mode, `\\x1b[OQ` normal
        mode) are written with a pause between them because this pty's terminal mode is
        not otherwise known — sending both covers whichever one the attached client is
        actually in, harmlessly duplicating the keypress if it is in neither exclusively
        (redundant, not incorrect: two menus would just draw and one waits behind the
        other, and only ONE F2 is ever ACTUALLY interpreted per mode in practice).
        """
        bind_cmd = ["tmux", "-L", SOCKET, "bind", "-n", "F2"] + cmd[3:]
        r = subprocess.run(bind_cmd, capture_output=True, text=True, timeout=5)
        self.assertEqual(r.returncode, 0, r.stderr)
        os.write(fd, b"\x1bOQ")
        time.sleep(0.3)
        os.write(fd, b"\x1b[OQ")
        time.sleep(0.8)

    def _short_canary(self, name: str) -> str:
        """A canary path SHORT enough to survive `record`'s own `_MAX_LABEL` (60 chars)
        truncation intact when embedded directly IN A LABEL.

        `tempfile.mkdtemp()`'s own paths run 55-70+ characters before a filename is
        even added on this platform (`/var/folders/<hash>/T/<prefix>-<random>/`) —
        confirmed by hand that this silently truncates an embedded `#(touch ...)` job
        mid-path, cutting off its closing `)` and neutralising the injection BY
        ACCIDENT rather than by the escape this class exists to prove: the very first
        version of these three tests passed even with `_safe_label`'s own `#` -> `##`
        line deleted, for exactly this reason — a real bug in the test, not a real fix.
        `/tmp` directly, not the plane's own scratch dir, and not cleaned up via
        `shutil.rmtree` on a whole directory (there is no directory here to remove).
        """
        path = f"/tmp/chfi-{os.getpid()}-{name}"

        def _cleanup():
            try:
                os.remove(path)
            except OSError:
                pass
        self.addCleanup(_cleanup)
        return path

    def test_a_shell_job_label_never_executes_when_the_menu_renders(self):
        fid = f"menu-fmt-integ-{os.getpid()}"
        self._new_session(fid)
        client, fd = self._attach_pty(fid)

        canary = self._short_canary("a")
        hostile_label = f"#(touch {canary})"

        menu.record(fid=fid, entries=[(hostile_label, ["true"])])
        self._drive_menu(fd, menu.menu_argv(fid, SOCKET, client=client))

        self.assertFalse(os.path.exists(canary),
                         "an unescaped #(...) label executed the instant the menu was "
                         "drawn — the exact hole this fix round closes")

    def test_a_format_variable_label_also_never_executes(self):
        """`#{session_name}` substitutes a value rather than running a shell job, but it
        is the SAME `#`-triggered mechanism `_safe_label`'s escaping closes for both —
        this pins that a label combining both shapes still creates no canary against a
        real, rendered menu, not only in `menu_argv`'s own argv-shape unit test."""
        fid = f"menu-fmt-integ2-{os.getpid()}"
        self._new_session(fid)
        client, fd = self._attach_pty(fid)

        canary = self._short_canary("b")
        hostile_label = f"#{{session_name}} #(touch {canary})"

        menu.record(fid=fid, entries=[(hostile_label, ["true"])])
        self._drive_menu(fd, menu.menu_argv(fid, SOCKET, client=client))

        self.assertFalse(os.path.exists(canary),
                         "#{session_name} and #(...) both reach the SAME escape — a "
                         "label combining them must still create no canary")

    def test_an_escaped_label_still_lets_the_real_action_run_when_selected(self):
        """The other half of the property: `_safe_label` must not merely refuse to
        execute — the row still has to be USABLE. After `_drive_menu` opens it (F2),
        writes the bound key ('1') directly to the attached pty's own master fd — not
        `tmux send-keys` (confirmed by hand: `send-keys -t <pane>` feeds the PANE's own
        input queue, which an active menu overlay never reads from; a canary bound to
        key '1' was never created that way) — and confirms the REAL argv (spawned via
        the real id) is what runs, never anything derived from the label."""
        fid = f"menu-fmt-integ3-{os.getpid()}"
        self._new_session(fid)
        client, fd = self._attach_pty(fid)
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session=fid)).returncode, 0)

        tmp = tempfile.mkdtemp(prefix="charter-integ-fmt3-")
        self.addCleanup(shutil.rmtree, tmp, True)
        real_canary = os.path.join(tmp, "REAL_CANARY")  # in the ARGV, not the label —
                                                         # no length bound applies to it
        format_canary = self._short_canary("c")  # in the LABEL — must stay short
        hostile_label = f"#(touch {format_canary})"

        menu.record(fid=fid, entries=[
            (hostile_label, [sys.executable, "-c", f"open({real_canary!r}, 'w').close()"]),
        ])
        cmd = menu.menu_argv(fid, SOCKET, client=client)
        # `charter` (bare, per `menu_argv`'s own text) may not be this checkout on
        # `$PATH` — same substitution `MenuIntegration` above makes, for the same
        # reason. The item's own action is the LAST argv element, a single string
        # (`"run-shell 'charter frame-action a0'"`), not a separate "run-shell" token.
        cmd[-1] = cmd[-1].replace("charter frame-action",
                                 f"{sys.executable} -m charter frame-action")
        self._drive_menu(fd, cmd)

        self.assertFalse(os.path.exists(format_canary),
                         "the hostile label must not have executed merely by being "
                         "rendered")

        os.write(fd, b"1")
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline and not os.path.exists(real_canary):
            time.sleep(0.2)
        self.assertTrue(os.path.exists(real_canary),
                        "selecting the (escaped, still-usable) item must still run "
                        "its real, opaque-id-resolved action")
        self.assertFalse(os.path.exists(format_canary),
                         "selecting the item must not retroactively execute the label")


if __name__ == "__main__":
    unittest.main()
