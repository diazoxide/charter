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

**Presence is not capability, and that gap cost a whole CI matrix.** These tests were
first written against tmux 3.7c on a developer's Mac, inside a real terminal, and gated
on `shutil.which("tmux")` alone — so on `ubuntu-latest` (tmux 3.4, `TERM=dumb`, no
controlling terminal) seven of them failed rather than skipped, and each failure
described the environment rather than charter. Three capabilities are now PROBED, each
where it is needed, each skipping with a message that names what was missing:

* an attachable tmux CLIENT (`_NeedsAttachedClient`) — `tmux attach` refuses a terminal
  that cannot clear, which is what a headless CI step's `TERM=dumb` is;
* a `pane-died` hook that FIRES (`TmuxIntegration._require_pane_died_fires`) — the one
  thing the two exit-status tests cannot substitute for;
* a tmux parser that lets the hotkey injection through at all
  (`test_the_hotkey_injection_this_guards_against_is_live_on_this_tmux`) — tmux 3.4's
  refuses it outright, so on 3.4 there is no live exploit to control against.

None of these weakens an assertion: every test that RUNS asserts exactly what it always
did, and a machine that cannot run one says which capability it lacked.

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
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, hooks, instance, todos
from charter.frame import gather, layout, menu, notify, state, tmuxctl

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

#: The plane this test PROCESS was started in, captured at IMPORT — before any `setUp`
#: has had a chance to repoint `config`, so it is unavoidably the developer's REAL
#: `.charter/`. Kept only so `_TmuxServerFixture.setUp` can refuse to run a test against
#: it (see that method); nothing here ever reads or writes under this path.
_REAL_STATE_DIR = Path(config.STATE_DIR)


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped.

    Load-bearing since #383, not a flourish: `state.reap` reads the number at the end of
    a frame id as the launcher's pid and KEEPS any directory whose launcher is still
    running. A hand-written `-1` reads as pid 1 — `launchd`/`init`, which never exits —
    so a fixture named that way is kept by the pid rule and an "it was reaped" assertion
    about it can never fail. A pid that genuinely ended is the only way to leave the
    server/liveness rule as the one thing deciding."""
    proc = subprocess.Popen([sys.executable, "-c", "pass"])
    proc.wait()
    return proc.pid


def _importable_env(env: dict) -> dict:
    """*env* with this checkout on `$PYTHONPATH`, so a panel spawned with `-P` can still
    import it.

    Every panel argv in this module now comes from `layout.panel_command`, which builds
    its interpreter half with `util.self_relaunch_argv()` — `[sys.executable, "-P", "-m",
    "charter", ...]`. `-P` is the whole point (#390: without it a panel spawned into a
    cwd that happens to hold a `charter/` package imports THAT tree), and it is also
    exactly what stops `cwd=_REPO_ROOT` from making `charter` importable the way these
    classes used to rely on. `$PYTHONPATH` is the substitute `tests/
    test_self_relaunch_shadowing.py` already established and measured for this: `-P`
    strips only the cwd/script-dir entry `-m` auto-prepends, never a `PYTHONPATH` entry
    or a real site-packages one, so a checkout reached this way stands in for a real
    install without weakening the flag being tested.

    Prepended rather than assigned: a runner that already exports `$PYTHONPATH` keeps it,
    and this checkout still wins over anything else on it — a panel under test must be
    THIS tree's panel.
    """
    existing = env.get("PYTHONPATH", "")
    return dict(env, PYTHONPATH=(f"{_REPO_ROOT}{os.pathsep}{existing}" if existing
                                 else str(_REPO_ROOT)))


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


def _gate_argv(gate: str, dies_by: str) -> list[str]:
    """A pane program that waits for the file *gate* and then dies by *dies_by*.

    **Never tmux's default pane command, and never driven by `send-keys`.** Both of those
    were how this module used to kill a pane, and both were measured wrong:

    * tmux's default is `default-shell` run as a LOGIN shell, which charter never
      launches — `frame/layout.session_argv` always ends `new-session … -- <harness
      argv>`. On `ubuntu-latest` (tmux 3.4, Ubuntu 24.04) a login `bash` exiting runs the
      runner's own `~/.bash_logout`, after which tmux reports the pane `#{pane_dead}` `1`
      with `#{pane_dead_status}` EMPTY and **fires no `pane-died` hook at all** — so the
      file these tests read was never written. Every CI job failed that way.
    * `send-keys` into an INTERACTIVE shell, telling it to `exit 42` or `kill -9 $$`, is
      not reliable: measured 30 trials per shape on tmux 3.4, `send-keys` shapes fired
      the hook 26-28 times out of 30, while a pane whose own program died on its own
      fired it 30/30 (`exit 42` through this gate, `kill -9 $$` through this gate, and an
      external SIGKILL to `#{pane_pid}` — all three perfect). The failures all looked
      identical: `#{pane_dead}` `1`, `#{pane_dead_status}` empty, no hook. That is tmux
      3.4's own `server_destroy_pane` refusing to fire `pane-died` until `PANE_STATUSREADY`
      is set, and an interactive shell's job-control handling of a keyed-in death
      sometimes leaves it unset. It is also nothing charter does: a harness dies because
      it exited or was signalled, never because someone typed at it.

    So a pane here dies the way a harness dies — its own program reaching its own end —
    and the test says WHEN by opening the gate. That is both the faithful shape and the
    reliable one.

    Three argv words, also deliberately: tmux runs a ONE-ARGUMENT pane command through a
    shell (`$SHELL -c '<the argument>'`) and only `execvp`s it directly when there is
    more than one, so a single-word command can leave the pane's own process a `sh -c`
    WRAPPER with the real program as its child — measured in an Ubuntu 24.04 container,
    where that wrapper turned a SIGKILLed child into a normal exit with status 137.
    """
    return ["/bin/sh", "-c", f"while [ ! -f '{gate}' ]; do sleep 0.05; done; {dies_by}"]

#: Terminal types tried, in order, when this module forks a tmux CLIENT onto a pty.
#:
#: `tmux attach` REFUSES to start a client on an unsuitable terminal — "open terminal
#: failed: terminal does not support clear" — and a headless CI step is exactly that:
#: measured on `ubuntu-latest`, a workflow `run:` step has `TERM=dumb`. That is a
#: property of the ENVIRONMENT, not of tmux's version and not of anything charter does:
#: the same tmux 3.4 that refuses `TERM=dumb` attaches immediately when the forked
#: client's own `TERM` names a terminal that can clear. A real operator's `charter
#: <harness>` always has one; a test's forked client has to be given one.
#:
#: `$TERM` first when this process has a usable one (a developer's own terminal is the
#: most faithful thing to attach with), then two entries carried by every terminfo
#: database on a machine that has tmux at all. `dumb` is never tried — tmux's refusal of
#: it is the measured fact above, so retrying it would only spend the timeout.
_TERM_CANDIDATES = tuple(dict.fromkeys(
    ([os.environ["TERM"]] if os.environ.get("TERM", "dumb") != "dumb" else [])
    + ["xterm-256color", "screen", "vt100"]))

#: Whether a pane-scoped `pane-died` hook FIRES on this machine — probed once per test
#: process by `TmuxIntegration._require_pane_died_fires`, and a list rather than a bool
#: so "not probed yet" and "probed, and the answer is False" stay distinguishable.
_PANE_DIED_FIRES: list[bool] = []

#: Whether THIS tmux's `new-session` accepts `-e` at all. Probed once per process, the
#: same shape as `_PANE_DIED_FIRES` and for the same reason: it is a property of the
#: binary on this machine rather than of any one test, and `-e` arrived in tmux 3.2
#: (`tmuxctl.SESSION_ENV_FLOOR`) while charter still launches below that.
_NEW_SESSION_TAKES_ENV: list[bool] = []

#: How long ONE forked client gets to register with tmux before this gives up on it.
#:
#: Generous on purpose, and NOT the thing that detects a refusal: a refused `tmux attach`
#: EXITS, which `_await_client` notices immediately, so the next terminal type is tried
#: without spending this at all. Cutting it short instead would abandon a client that was
#: merely slow and move on to a terminal type whose KEY ENCODINGS differ — and every test
#: using an attached client drives it by writing a raw key sequence to the pty, so a
#: silently-substituted terminal type is a silently-broken test rather than an honest
#: skip. (Measured: that is exactly what a 2s cap did on a loaded developer machine —
#: `MenuClientIntegration` failed roughly one run in ten with the menu never opening.)
_ATTACH_TIMEOUT = 10.0


def _fork_attach(session: str, term: str) -> tuple[int, int]:
    """`tmux attach -t session` under a fresh pty, with *term* as the client's `$TERM`.

    Returns the child's pid and the pty's master fd. The child never returns — the
    `finally` is there so a failed `execvp` cannot fall through into the test process's
    own code as a second copy of the whole suite.
    """
    pid, fd = pty.fork()
    if pid == 0:
        try:
            os.environ["TERM"] = term
            os.execvp("tmux", ["tmux", "-L", SOCKET, "attach", "-t", session])
        finally:
            os._exit(127)
    return pid, fd


def _reap_pty(pid: int, fd: int) -> None:
    """SIGKILL, reap, close — a pty-forked child is THIS process's own zombie to reap,
    unlike a tmux-spawned pane's process, which reparents away (see `_kill_pid`)."""
    _kill_pid(str(pid))
    try:
        os.waitpid(pid, 0)
    except OSError:
        pass
    try:
        os.close(fd)
    except OSError:
        pass


def _refusal(fd: int) -> str:
    """Whatever a refused `tmux attach` printed to its pty — tmux's own words, so a skip
    can name the capability that was missing instead of guessing at it."""
    try:
        os.set_blocking(fd, False)
        out = os.read(fd, 4096)
    except OSError:
        return ""
    finally:
        try:
            os.set_blocking(fd, True)
        except OSError:
            pass
    return out.decode("utf-8", "replace").strip()


#: How long ANY one thing this module waits to happen gets to happen — #409.
#:
#: **One constant, because the flake was module-wide and the deadline was not.** Eleven
#: separate waits carried 3, 5, 8 and 15 seconds apiece, each number picked where it was
#: written and none of them re-examined after CI got slower; the signal-death test alone
#: cost a re-run on five PRs, which is exactly the tax that teaches people to merge over
#: red. A per-call number is a per-call judgement about a machine nobody who wrote it was
#: running on.
#:
#: **Only POSITIVE waits use it** — "wait until X happens, then assert it did". A wait
#: that exists to show something does NOT happen must keep its own short, deliberate
#: number: raising one of those buys no reliability and spends the time on every single
#: run. This one is spent only by a test that is already failing or skipping.
#:
#: Generous rather than tuned, for the reason `_ATTACH_TIMEOUT` gives: polling makes a
#: slow machine slow instead of wrong, and every wait below returns the instant its
#: condition holds, so the number is what a LOADED runner may take and never what a
#: healthy one does.
_DEADLINE = 20.0


def _await_file(path: str, timeout: float = _DEADLINE) -> bool:
    """Wait for a hook-written file to appear, up to *timeout*.

    A fixed `time.sleep(1)` here was a guess about how long a `run-shell` takes to fork a
    shell and redirect one line — which is the wrong shape of question for a test whose
    assertion is about the file's CONTENT. Polling makes a slow machine slow rather than
    wrong, and a machine where the hook never fires still reaches the same assertion with
    the same missing file (see `TmuxIntegration._require_pane_died_fires`, which is what
    tells those two apart).
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline and not os.path.exists(path):
        time.sleep(0.1)
    return os.path.exists(path)


def _await_client(session: str, exclude: frozenset, pid: int) -> str:
    """The name of a client attached to *session* that the caller does not already know
    about, or `""` when the forked client at *pid* will never register.

    Two ways to learn that, and the fast one is watching the CHILD: a `tmux attach` that
    the terminal type was wrong for exits at once, so `waitpid(WNOHANG)` reporting it gone
    is a definitive refusal, available in milliseconds and independent of tmux's wording.
    :data:`_ATTACH_TIMEOUT` is only the backstop for a client that neither registers nor
    exits.
    """
    deadline = time.monotonic() + _ATTACH_TIMEOUT
    while time.monotonic() < deadline:
        out = _tmux("list-clients", "-t", session, "-F", "#{client_name}")
        fresh = {n.strip() for n in out.stdout.splitlines() if n.strip()} - exclude
        if fresh:
            return next(iter(fresh))
        try:
            if os.waitpid(pid, os.WNOHANG)[0] == pid:
                return ""   # the client exited rather than attaching — a refusal
        except OSError:
            return ""
        time.sleep(0.1)
    return ""


def _await_dead(pane: str, timeout: float = _DEADLINE) -> int | None:
    """Poll the launcher's OWN `_query_pane_dead_status` until *pane* is gone.

    Polled rather than slept for the same reason `_await_file` is: how long tmux takes to
    reap a child and mark its pane dead is a property of the machine, and a fixed sleep
    turns a loaded one into a failure instead of a slow pass. `None` back means the pane
    was still alive when time ran out — which the caller asserts on, since every command
    handed to this module's own helper is built to die at once.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        code = commands_frame._query_pane_dead_status(SOCKET, pane)
        if code is not None:
            return code
        time.sleep(0.1)
    return None


class _NeedsAttachedClient:
    """`_attach_pty` for the two classes that need a REAL, ATTACHED client, and the
    capability probe that decides whether this machine can give them one.

    `display-menu` refuses outright ("no current client") without an attached client, so
    both classes below are unrunnable — not passable, not failable — where tmux will not
    attach one. They skip, naming what they could not get and quoting tmux's own refusal,
    rather than asserting anything about a menu nothing drew.
    """

    def _attach_pty(self, session: str, exclude: frozenset = frozenset()) -> tuple[str, int]:
        """Forks a real `tmux attach -t session` under a pty — the one way to hand
        `display-menu` a client it will actually accept for `-c`/`-t` targeting.

        *exclude* names clients the caller already knows about: `list-clients -t session`
        lists every client attached to it, not only the one just forked, so a SECOND pty
        on the same session has to pick its own name out from among several.

        Registers the fork's own cleanup (SIGKILL, then reap — see `_reap_pty`) before
        returning the attached client's name (`#{client_name}`, read back from tmux
        itself once the attachment has had time to register) and the pty's own master fd
        — writing a KEY to the fd (not `tmux send-keys`, confirmed by hand: `send-keys`
        feeds a PANE's own input queue, which an active menu overlay never reads from) is
        the only way found to actually select a menu item from here.
        """
        refusals = []
        for term in _TERM_CANDIDATES:
            pid, fd = _fork_attach(session, term)
            try:
                name = _await_client(session, exclude, pid)
            except Exception:
                _reap_pty(pid, fd)
                raise
            if name:
                self.addCleanup(_reap_pty, pid, fd)
                return name, fd
            refusals.append(f"TERM={term}: {_refusal(fd) or '(tmux printed nothing)'}")
            _reap_pty(pid, fd)
        self.skipTest(
            "no tmux client can attach on this machine, and a rendered `display-menu` "
            "needs one — tmux refused every terminal type tried: " + " | ".join(refusals))


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class _TmuxServerFixture:
    """One real tmux server per test, and the helpers every class here needs.

    A MIXIN rather than a base TestCase, and that is not style. `unittest` collects a
    subclass's inherited tests as its own, so `class WindowInsideAnOperatorsTmux
    (TmuxIntegration)` would silently re-run every test in `TmuxIntegration` a second
    time — each one starting its own real tmux server — which is how a "14 tests"
    surprise showed up the first time this module grew a second class. Mixing the
    fixture in leaves each class owning exactly its own tests.

    **Mixed in OVER `PersonaIso`, never over a bare `unittest.TestCase`, and `setUp`
    enforces that rather than trusting it.** This module runs charter's REAL command
    handlers, and `commands_frame.cmd_launch`'s very first act on either path is
    `state.reap(...)` — an `rmtree` of every frame directory under `config.STATE_DIR`
    that the named server does not report, which for a directory carrying no `server`
    marker is every server there is. Against an unisolated `config` that is the
    developer's own `.charter/frame/`, and a single test run silently deletes the state
    of the live frames on the machine. Measured, not theorised: a marker frame directory
    planted in the real plane was gone after one run of
    `test_nothing_of_the_operators_tmux_is_written_by_a_whole_launch`, back when this
    class's own subclass listed `unittest.TestCase` here instead.
    """

    def setUp(self) -> None:
        # FIRST, and cooperatively: `PersonaIso.setUp` is what repoints `config`, and it
        # only runs if this method hands control up the MRO. A `setUp` that quietly
        # skipped this line would leave a class whose bases READ as isolated running
        # against the real plane anyway — invisible from the subclass, which is the
        # shape of the defect this whole docstring is about.
        super().setUp()
        # Then check that it actually took, because "the bases say PersonaIso" and "the
        # config in front of this test is a throwaway" are different claims and only the
        # second one is the safe one. Fails LOUDLY, naming the fix, rather than letting
        # a real `rmtree` run over the developer's frames.
        self.assertNotEqual(
            Path(config.STATE_DIR), _REAL_STATE_DIR,
            "this test would run charter's real command handlers against the "
            "developer's own control plane, whose frame state `state.reap` deletes — "
            "a class using _TmuxServerFixture must mix it in over `PersonaIso` "
            "(`class X(_TmuxServerFixture, PersonaIso)`), not over `unittest.TestCase`")
        self.addCleanup(self._teardown_socket)
        self._pane_counter = 0
        self._gate_dir = tempfile.mkdtemp(prefix="charter-integ-gate-")
        self.addCleanup(shutil.rmtree, self._gate_dir, True)

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

    def _new_pane(self, dies_by: str = "exit 0") -> tuple[str, str, str]:
        """A fresh session on `SOCKET`, `remain-on-exit` armed; its name, pane id and GATE.

        The pane runs a program that WAITS for its gate file and then dies by *dies_by* —
        the caller opens the gate (`_release`) when it wants the death. See `_gate_argv`
        for why the pane is never driven with `send-keys` instead, which is the shape this
        module used to use and the reason it failed roughly one CI run in ten.
        """
        self._pane_counter += 1
        name = f"p{self._pane_counter}"
        gate = os.path.join(self._gate_dir, f"gate-{name}")
        r = _tmux("new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                  "-P", "-F", "#{pane_id}", "--", *_gate_argv(gate, dies_by))
        self.assertEqual(r.returncode, 0, r.stderr)
        # Global on this socket's server — cheap to repeat per pane, and every frame
        # wants it regardless (see `commands_frame._PLACEHOLDER_CONF`'s own docstring).
        _tmux("set", "-g", "remain-on-exit", "on")
        return name, r.stdout.strip(), gate

    @staticmethod
    def _release(gate: str) -> None:
        """Open a pane's gate: its program stops waiting and dies the way it was built to."""
        Path(gate).touch()

    def _hook_reaches_a_shim(self, *, socket, pane, gate, interpreter_dir):
        """Arm *pane*'s respawn hook with a shim as charter's interpreter, open the gate,
        and return whatever argv the shim recorded (``None`` if it never ran).

        The shim stands in for `sys.executable`, so the argv it records is what charter
        would REALLY have been invoked with — and *interpreter_dir* is how a caller
        chooses what shape of path that is.
        """
        os.makedirs(interpreter_dir, exist_ok=True)
        marker = os.path.join(self._gate_dir, f"argv-{os.path.basename(pane)}")
        shim = os.path.join(interpreter_dir, "fake charter py")
        Path(shim).write_text(f'#!/bin/sh\nprintf "%s" "$*" > "{marker}"\n')
        os.chmod(shim, 0o755)
        with mock.patch("charter.commands_frame.util.self_relaunch_argv",
                        side_effect=lambda *a: [shim, "-P", "-m", "charter", *a]):
            argv = commands_frame._panel_died_hook_argv(
                socket=socket, panel_pane=pane, slot="top", fid="demo-1")
        self.assertIsNotNone(argv, f"charter refused to arm a hook for {shim!r}")
        self.assertEqual(_run(argv).returncode, 0)
        self._release(gate)
        if not _await_file(marker):
            return None
        return Path(marker).read_text().split()

    def _require_pane_died_fires(self) -> None:
        """Skip unless a pane-scoped `pane-died` hook actually FIRES on this machine.

        The two exit-status tests below read a file a hook WROTE. If the hook never runs
        there is no file, and `open()` raises a `FileNotFoundError` that says nothing
        about which of the four moving parts (`remain-on-exit`, `set-hook -p`,
        `set-environment`, `run-shell`) was missing — the shape this module's CI failure
        actually took. This probes the ONE capability those tests cannot substitute for,
        against a real pane dying the same way theirs do, and does it with a hook whose
        action is a CONSTANT path: no `set-environment` value, no `#{pane_dead_status}`,
        nothing but "did tmux run this at all". A machine that passes this and then fails
        a test below has a real defect in what the test is about; a machine that fails
        this could not have run the test in the first place.

        Probed once per process (`_PANE_DIED_FIRES`) — the answer is a property of this
        tmux and this environment, not of any one test, and each probe costs a pane.
        """
        if not _PANE_DIED_FIRES:
            _, pane, gate = self._new_pane("exit 7")
            tmp = tempfile.mkdtemp(prefix="charter-integ-probe-")
            self.addCleanup(shutil.rmtree, tmp, True)
            marker = os.path.join(tmp, "fired")
            _tmux("set-hook", "-p", "-t", pane, "pane-died",
                  f'run-shell "touch {marker}"')
            self._release(gate)
            _PANE_DIED_FIRES.append(_await_file(marker))
        if not _PANE_DIED_FIRES[0]:
            self.skipTest(
                "a pane-scoped `pane-died` hook does not fire on this machine, so "
                "nothing here can carry a harness's exit status out of a dead pane — "
                "the capability these tests measure is not present to measure")


class TmuxIntegration(_TmuxServerFixture, PersonaIso):
    """`PersonaIso`, not `unittest.TestCase` — see `_TmuxServerFixture`'s own docstring.
    Nothing in THIS class writes plane state today, so the isolation here is inert; it
    is listed anyway because the fixture requires it of every class, and a requirement
    with an exception in it is one the next class copies the exception from."""

    # -- 0. The hotkey is not a free string ----------------------------------------- #

    def _source_hotkey(self, conf_dir: Path, hotkey: str, name: str
                       ) -> subprocess.CompletedProcess:
        """`conf_text`'s own output for *hotkey*, written out and `source-file`'d for
        real — never a hand-retyped config."""
        conf = conf_dir / f"{name}.conf"
        conf.write_text(commands_frame.conf_text(
            hotkey=hotkey, mouse=False, history_limit=1, session="p1"))
        return _tmux("source-file", str(conf))

    def test_the_hotkey_injection_this_guards_against_is_live_on_this_tmux(self):
        """The POSITIVE CONTROL for the test below, and the reason it can fail at all:
        an unfiltered `[frame] hotkey` really does run a command at launch — no keypress,
        no attached client, nothing but `source-file` accepting the file.

        **Its own capability, and not every tmux has it.** Measured against tmux 3.7c the
        injected line parses and the canary appears. Measured against tmux 3.4 (Ubuntu
        24.04, the `ubuntu-latest` runner) tmux's OWN parser refuses the whole file first
        — `command run-shell: too many arguments (need at most 2)`, `source-file` returns
        1 — and no canary is ever created. charter's `instance._HOTKEY_RE` is what closes
        the hole on both, but on a tmux whose parser refuses the exploit outright there is
        no live exploit here to control against, so this skips and says so rather than
        asserting that a hostile value is dangerous on a tmux where it is not. Split out
        of the test below for exactly that reason: the property charter owns must keep
        running everywhere, and only the control it is a control FOR is version-shaped.
        """
        armed = f"/tmp/charter-c1-{os.getpid()}-armed"
        self.addCleanup(lambda: Path(armed).unlink(missing_ok=True))
        conf_dir = Path(tempfile.mkdtemp(prefix="charter-integ-hotkey-"))
        self.addCleanup(shutil.rmtree, conf_dir, True)
        self._new_pane()

        r = self._source_hotkey(conf_dir, f"F2\nrun-shell 'touch {armed}'", "armed")
        if r.returncode != 0:
            self.skipTest(
                "this tmux's own parser refuses the injected hotkey before running any "
                "of it (`source-file` returned "
                f"{r.returncode}: {(r.stdout + r.stderr).strip()!r}), so the exploit "
                "this controls for is not reachable here")
        time.sleep(0.5)
        self.assertTrue(os.path.exists(armed),
                        "`source-file` accepted the hostile hotkey and returned 0, but "
                        "the injected command never ran — the control proves nothing in "
                        "that state, and neither does the test it controls for")

    def test_a_hostile_hotkey_from_charter_toml_runs_nothing_at_launch(self):
        """CRITICAL: `[frame] hotkey` reaches `source-file`'s PARSER, and until
        `instance._HOTKEY_RE` it was type-checked as a `str` and nothing else.

        The hostile charter.toml goes through `instance.frame_of`, the way `config.FRAME`
        builds it, and the resolved hotkey must produce no canary at all. Two assertions,
        either of which fails if `_HOTKEY_RE` is removed: the resolved value must BE the
        default, and sourcing it must run nothing. See the test above for the positive
        control — the proof that an unfiltered value really does execute — which is its
        own test because not every tmux's parser lets the exploit through.
        """
        disarmed = f"/tmp/charter-c1-{os.getpid()}-disarmed"
        self.addCleanup(lambda: Path(disarmed).unlink(missing_ok=True))
        conf_dir = Path(tempfile.mkdtemp(prefix="charter-integ-hotkey-"))
        self.addCleanup(shutil.rmtree, conf_dir, True)
        self._new_pane()

        resolved = instance.frame_of(
            {"frame": {"hotkey": f"F2\nrun-shell 'touch {disarmed}'"}})["hotkey"]
        self.assertEqual(resolved, "F2", "the hostile value must degrade to the default")
        self.assertEqual(self._source_hotkey(conf_dir, resolved, "disarmed").returncode, 0)
        time.sleep(0.5)
        self.assertFalse(os.path.exists(disarmed),
                         "a charter.toml hotkey ran a command at launch, with no "
                         "keypress — see instance._HOTKEY_RE")

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
        _, pane_correct, _ = self._new_pane()
        write = commands_frame._pane_died_write_hook_argv(socket=SOCKET, harness_pane=pane_correct)
        teardown = commands_frame._pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=pane_correct)
        self.assertEqual(_run(write).returncode, 0)
        self.assertEqual(_run(teardown).returncode, 0)
        hooks = _tmux("show-hooks", "-p", "-t", pane_correct).stdout
        self.assertIn("pane-died[0]", hooks)
        self.assertIn("pane-died[1]", hooks)

        _, pane_wrong, _ = self._new_pane()
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

    def test_arming_a_panel_pane_leaves_the_harness_panes_hook_array_alone(self):
        """#382's own warning, checked against a real server rather than argued.

        A panel pane now gets its OWN `pane-died` hook (`_panel_died_hook_argv`), and
        `pane-died` is the same event whose array the harness pane uses for the exit
        code — where an unindexed `set-hook` REPLACES THE WHOLE ARRAY, as the test above
        measures. A third writer of that array would delete `[1]` and bring back the
        hang. It is safe only because pane options are PER PANE, which is exactly the
        thing to verify here: the harness pane's array is read back before and after the
        panel pane is armed and must be byte-identical, and the panel's own array must
        hold only its own hook."""
        _, harness_pane, _ = self._new_pane()
        _run(commands_frame._pane_died_write_hook_argv(socket=SOCKET,
                                                       harness_pane=harness_pane))
        _run(commands_frame._pane_died_teardown_hook_argv(socket=SOCKET,
                                                          harness_pane=harness_pane))
        before = _tmux("show-hooks", "-p", "-t", harness_pane).stdout

        panel_pane = _tmux("split-window", "-t", harness_pane, "-v", "-l", "1",
                           "-P", "-F", "#{pane_id}", "--",
                           *_gate_argv(os.path.join(self._gate_dir, "never"), "exit 0")
                           ).stdout.strip()
        self.assertTrue(panel_pane.startswith("%"), panel_pane)
        argv = commands_frame._panel_died_hook_argv(
            socket=SOCKET, panel_pane=panel_pane, slot="bottom", fid="demo-1")
        self.assertIsNotNone(argv, "this machine's own interpreter path cannot be "
                                   "written into a hook action — see "
                                   "`_action_word_is_safe`")
        armed = _run(argv)
        self.assertEqual(armed.returncode, 0, armed.stderr)

        after = _tmux("show-hooks", "-p", "-t", harness_pane).stdout
        self.assertEqual(before, after,
                         "arming a PANEL pane for respawn changed the HARNESS pane's "
                         "own pane-died array — the exit code and the teardown both "
                         "live there")
        self.assertIn("pane-died[0]", after)
        self.assertIn("pane-died[1]", after)
        panel_hooks = _tmux("show-hooks", "-p", "-t", panel_pane).stdout
        self.assertIn("frame-respawn", panel_hooks)
        self.assertNotIn("kill-session", panel_hooks,
                         "a panel's own hook must never be able to end the frame")

    def test_a_dead_panels_hook_runs_charter_with_the_slot_and_pane_it_must_revive(self):
        """The whole respawn mechanism end to end, minus the respawn itself: does the
        action string `_panel_died_hook_argv` builds actually FIRE, reach a shell, and
        deliver the right argv?

        Four separate things have to hold at once and none of them can be checked by
        reading the string: the interpreter path has to survive tmux's own parse of the
        action (the failure `_pane_died_write_hook_argv`'s docstring measures for double
        quotes, one layer over), `run-shell -b` must still run the command at all, the
        slot and pane id must arrive as separate argv words, and — since #408 — the
        frame id has to travel WITH them rather than out of band, because
        `set-environment` is unavailable on the operator's own server.

        The interpreter is a script that records its own argv, at a path holding a SPACE:
        a bare interpolation would split it into two words and the shim would never run
        at all."""
        self._require_pane_died_fires()
        _, pane, gate = self._new_pane("exit 3")
        seen = self._hook_reaches_a_shim(
            socket=SOCKET, pane=pane, gate=gate,
            interpreter_dir=os.path.join(self._gate_dir, "interp dir"))
        self.assertIsNotNone(
            seen, "the panel's own pane-died hook never reached a shell — "
                  f"hooks: {_tmux('show-hooks', '-p', '-t', pane).stdout!r}")
        self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                "--pane", pane, "--frame", "demo-1"])

    def test_an_awkward_interpreter_path_arrives_byte_for_byte(self):
        """The property `_ACTION_QUOTE_BREAKERS` is derived from, measured rather than
        argued: every ASCII punctuation character EXCEPT the six that mean something to
        one of the three parsers involved is literal on the way through, so an
        interpreter living behind one is armed rather than refused — and the argv that
        comes out the far side is the one charter meant.

        Without this the guard could be tightened to a paranoid allowlist and nothing
        would notice; with it, a tightening that costs a real path its respawn fails
        here."""
        self._require_pane_died_fires()
        _, pane, gate = self._new_pane("exit 3")
        awkward = "a b;c&d(e)f*g-h,i=j+k@l:m[n]o{p}q!r%s^t~u"
        seen = self._hook_reaches_a_shim(
            socket=SOCKET, pane=pane, gate=gate,
            interpreter_dir=os.path.join(self._gate_dir, awkward))
        self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                "--pane", pane, "--frame", "demo-1"])

    def test_a_hook_action_really_is_format_expanded_before_any_shell_sees_it(self):
        """The POSITIVE CONTROL for `_action_word_is_safe` refusing `#`, and the reason
        that refusal exists at all rather than being tidiness about a harmless character.

        tmux expands `#{…}` in a hook action before parsing it as a command — that is the
        mechanism `_pane_died_write_hook_argv` relies on to get `#{pane_dead_status}` to a
        shell, and it applies to every other character of the action equally. Measured
        here in the shape an interpreter path would have: the literal text
        `/opt/py#{pane_id}/x` reaches `/bin/sh` rewritten.

        A first version of charter's guard looked only for quote characters and let this
        through entirely. Without this control, tightening the guard would look like
        caution rather than like the fix for a measured rewrite — and `#{pane_title}`
        expands to text the program in that pane sets for itself."""
        self._require_pane_died_fires()
        _, pane, gate = self._new_pane("exit 3")
        out = os.path.join(self._gate_dir, "expanded")
        self.assertEqual(
            _tmux("set-hook", "-p", "-t", pane, "pane-died",
                  f"""run-shell -b 'echo "/opt/py#{{pane_id}}/x" > "{out}"'""").returncode,
            0)
        self._release(gate)
        self.assertTrue(_await_file(out), "the hook never reached a shell")
        seen = Path(out).read_text().strip()
        self.assertNotEqual(
            seen, "/opt/py#{pane_id}/x",
            "tmux no longer expands formats inside a hook action — if that is really "
            "true, `_pane_died_write_hook_argv`'s `#{pane_dead_status}` has stopped "
            "working too, which is a much larger thing than this test")
        self.assertEqual(seen, f"/opt/py{pane}/x",
                         "the text was rewritten, but not into the pane id — read what "
                         "this tmux actually does before trusting the guard's reasoning")

    #: How many deaths one exit-status test may spend before it gives up on this
    #: machine — #409.
    #:
    #: **Not a retry for flakiness: a retry for a trial that measured nothing.** The
    #: signal-death test was failing about one CI job in eight on tmux 3.4, and the
    #: diagnosis in the ticket ("tmux marks the pane dead and THEN runs the hook; the
    #: read lands in that window") predicts a longer wait would fix it. This module had
    #: already measured otherwise, in `PanelIntegration`'s own docstring: pinned to one
    #: cpu against four spin loops, 11 of 120 deaths on tmux 3.4 ended `#{pane_dead}` `1`
    #: with an EMPTY status and the `pane-died` hook never firing — **permanently**,
    #: polling a further 8 seconds never filled it in. tmux 3.4's `server_destroy_pane`
    #: does not have the child's status when the pane's fd closes, and nothing arrives
    #: later. So the deadline is not the knob for THIS one, and a bounded poll alone
    #: would have re-shipped the same flake with a longer wait attached to it.
    #:
    #: What a death like that produces is not a wrong answer, it is no answer: the pane
    #: is gone and tmux never ran charter's hook, so there is nothing for the test to be
    #: about. :meth:`_a_death_the_hook_saw` detects exactly that — with a probe hook
    #: beside charter's own, not by reading the empty status, which the healthy signal
    #: death also has — and spends another pane on it.
    _HOOK_TRIALS = 3

    def _a_death_the_hook_saw(self, dies_by: str, status_path: str):
        """One trial of "a pane dies, charter's write hook records its status".

        Returns the status file's CONTENT, or ``None`` when tmux never ran this pane's
        `pane-died` array at all — the tmux 3.4 window :data:`_HOOK_TRIALS` describes,
        which is a trial that measured nothing rather than a result.

        **The probe hook is what tells those apart, and it is a different question from
        the one the test asks.** It is installed at `pane-died[1]` with a CONSTANT action
        — `touch <marker>`, no `#{pane_dead_status}`, no `$CHARTER_FRAME_EXIT`, nothing
        charter builds — so the only thing its marker can report is "tmux reached this
        pane's hook array". tmux runs an array in index order, so the marker cannot
        appear unless charter's own `[0]` was reached first. An INDEXED install beside an
        existing `[0]` is safe: that is exactly what
        `test_the_write_hook_must_be_installed_before_the_teardown_hook` measures, and it
        is the same shape `cmd_launch` itself uses.

        **A fired array with no file is a FAILURE, not a void trial**, and that is the
        half that keeps this test able to fail. Delete the `${v:-N}` fallback and the
        hook still fires and still writes — an empty line — so the file exists and its
        content is asserted. Break the hook so nothing is written at all and the probe
        still fires, so this says so rather than skipping.
        """
        session, pane, gate = self._new_pane(dies_by)
        self.assertEqual(_run(commands_frame._exit_path_env_argv(
            socket=SOCKET, session=session, status_path=status_path)).returncode, 0)
        self.assertEqual(_run(commands_frame._pane_died_write_hook_argv(
            socket=SOCKET, harness_pane=pane)).returncode, 0)
        fired = os.path.join(self._gate_dir, f"array-ran-{pane.lstrip('%')}")
        self.assertEqual(_tmux("set-hook", "-p", "-t", pane, "pane-died[1]",
                               f'run-shell "touch {fired}"').returncode, 0)
        self._release(gate)
        if not _await_file(fired):
            return None, pane
        self.assertTrue(
            _await_file(status_path),
            "tmux ran this pane's `pane-died` array — a constant-action probe hook "
            "beside charter's own fired — and charter's write hook produced no file at "
            "all. That is charter, not the runner.")
        return Path(status_path).read_text().strip(), pane

    def _no_death_was_delivered(self):
        self.skipTest(
            f"none of {self._HOOK_TRIALS} deaths on this machine reached a `pane-died` "
            "hook at all — tmux never had the child's status when the pane's fd closed "
            "(measured on tmux 3.4 under load; see `_HOOK_TRIALS`). The capability this "
            "test measures was not present to measure, and nothing here is widened to "
            "pass without it.")

    # -- 2. Path delivery and injection ---------------------------------------------- #

    def test_the_exit_status_path_round_trips_a_hostile_plane_path(self):
        """`set-environment` (a single argv value, no shell involved) is how the
        exit-status path reaches the write hook's shell — verified here against a path
        containing a space, a literal `'`, and a `$(touch ...)` injection attempt all at
        once: the file at that exact path must hold the harness's real exit code, and
        nothing embedded in the path may execute.

        Tried up to `_HOOK_TRIALS` times, for #409's reason and not for flakiness'
        sake: a death tmux never ran a hook for measured nothing about this path."""
        self._require_pane_died_fires()
        tmp = tempfile.mkdtemp(prefix="charter-integ-inj-")
        self.addCleanup(shutil.rmtree, tmp, True)
        canary = os.path.join(tmp, "canary")
        hostile_dir = os.path.join(tmp, "it's a $(touch " + canary + ") dir")
        os.makedirs(hostile_dir, exist_ok=True)
        status_path = os.path.join(hostile_dir, "exit")

        for _ in range(self._HOOK_TRIALS):
            content, _pane = self._a_death_the_hook_saw("exit 42", status_path)
            if content is None:
                continue   # tmux never ran the array — see `_HOOK_TRIALS`
            self.assertFalse(os.path.exists(canary),
                             "the $(touch ...) inside the plane path must never execute")
            self.assertEqual(content, "42")
            return
        self._no_death_was_delivered()

    # -- 3. Signal death -------------------------------------------------------------- #

    def test_a_signal_death_writes_the_unknown_death_sentinel_not_an_empty_line(self):
        """Measured against tmux 3.7c and re-measured against tmux 3.4: a pane killed by
        SIGKILL reports `#{pane_dead}` `1` with `#{pane_dead_status}` EMPTY, not a number
        — `display-message` is checked directly to confirm that is still what THIS tmux
        does, not merely assumed. The write hook's own `${v:-N}` fallback must turn that
        empty value into `_UNKNOWN_DEATH_CODE` at the point of writing, so the file it
        produces is always a parseable integer; `state.exit_code` cannot parse an empty
        line and would silently read it back as "nothing was ever recorded".

        The pane's own program signals ITSELF once its gate opens, rather than the test
        typing the same thing at an interactive shell — see `_gate_argv` for the 30-trial
        measurement that made the difference between the two shapes.

        **#409, and what it is NOT.** This failed about one CI job in eight on tmux 3.4
        and cost a re-run on five PRs. Two fixes were rejected on the way to this one.
        Accepting an empty `#{pane_dead_status}` as a pass would make the assertion
        unfailable — the sentinel is the whole subject. And a longer wait would not have
        worked either: the ticket reads the failure as a race against the hook, but this
        module had already measured the opposite (`PanelIntegration`'s docstring, 11 of
        120 deaths on a one-cpu 3.4), that in that window the hook never fires AT ALL and
        never will. So the fix is to notice a death that carried no measurement and spend
        another pane, while asserting every delivered death exactly as strictly as before
        — see :meth:`_a_death_the_hook_saw` and :data:`_HOOK_TRIALS`."""
        self._require_pane_died_fires()
        tmp = tempfile.mkdtemp(prefix="charter-integ-sig-")
        self.addCleanup(shutil.rmtree, tmp, True)

        for attempt in range(self._HOOK_TRIALS):
            # A fresh path per trial, so a file left by an earlier trial can never be
            # what a later one reads back.
            status_path = os.path.join(tmp, f"exit-{attempt}")
            content, pane = self._a_death_the_hook_saw("kill -9 $$", status_path)
            if content is None:
                continue   # tmux never ran the array — see `_HOOK_TRIALS`

            dead, _, status = _tmux(
                "display-message", "-p", "-t", pane,
                "#{pane_dead}:#{pane_dead_status}").stdout.strip().partition(":")
            self.assertEqual(dead, "1", "the pane must be confirmed dead before this "
                                        "test means anything")
            self.assertEqual(status, "",
                             "this pins that tmux itself still reports an EMPTY status "
                             "for a signal death, not a negative number — the wrong "
                             "premise an earlier version of this suite assumed")
            self.assertEqual(content, str(commands_frame._UNKNOWN_DEATH_CODE),
                             f"expected the sentinel, got {content!r} (an empty string "
                             f"here is exactly the bug: state.exit_code cannot parse it)")
            return
        self._no_death_was_delivered()

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
        # kill-server THEN unlink, never two separately-registered `addCleanup`
        # calls in that order — `addCleanup` runs LIFO, so `addCleanup(kill-server)`
        # followed by `addCleanup(unlink)` actually runs unlink FIRST — see
        # `_teardown_socket`'s own docstring below for why that is backwards and
        # when it stops being harmless.
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)  # let it derive under CHARTER_ROOT, like this
                                            # process's own PersonaIso-isolated config
        self.env = _importable_env(self.env)  # the panel argv carries -P — see the helper

    def _teardown_socket(self) -> None:
        """Kill this class's own tmux server, THEN unlink its socket file — see
        `TmuxIntegration._teardown_socket` for why the file survives `kill-server`
        on its own. Order matters here specifically, not just tidily: two separate
        `addCleanup` calls (`kill-server`, then `unlink`) run LIFO — unlink FIRST,
        kill-server SECOND — which reconnects to nothing once the socket's own
        directory entry is already gone. It was harmless when this class was
        written (no session of its own armed `remain-on-exit`, so `_kill_pid` on
        each pane's own process already ended the session, then the server, on
        tmux's own `exit-empty` default before `kill-server` was ever asked to do
        anything) and was fixed anyway, so this class could not silently start
        leaking the day a test added to it armed one. That day has arrived: the two
        failure tests below arm `remain-on-exit` for their own window, exactly as a
        real frame has it armed, and with it on a killed pane leaves the session
        standing — only this `kill-server`, running FIRST, takes it down."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _spawn_panel(self, session: str, fid: str, *,
                     slot: str = "bottom", rows: int = 5) -> None:
        """One `charter panel <slot> --session <fid>` pane, in a fresh session named
        *session* on this class's socket. Registers `_kill_pid` as a cleanup — see its
        own docstring for why `kill-server` alone leaves this process orphaned — using
        the pane's OWN pid (`#{pane_pid}`), captured immediately, rather than the
        session or pane id: those name tmux objects, not the OS process underneath
        them, and killing the process is the actual guarantee this method exists to
        give every caller.

        The argv comes from `layout.panel_command` — the launcher's own — rather than
        being retyped here. Retyped, it drifted once already and in the direction that
        matters: it kept a bare `[sys.executable, "-m", "charter"]` after production had
        moved to `util.self_relaunch_argv()`, which is exactly the difference #390 is
        about, so this class was spawning a shape production no longer spawns.

        *rows* defaults to the five this class's original three tests use; the failure
        tests below pass `1`, because ONE row is the size `layout.SLOT_SIZE` gives `top`
        and `bottom` and the only size at which tmux's own dead-pane message costs the
        whole pane rather than one line of it."""
        argv = layout.panel_command(slot=slot, session=fid)
        r = _tmux("new-session", "-d", "-s", session, "-x", "40", "-y", str(rows),
                  "-c", str(_REPO_ROOT), "--", *argv, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = _tmux("display-message", "-p", "-t", session, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

    # -- A panel that cannot start, and what the operator is left looking at ---------- #

    #: The slot no renderer exists for, so `panel.run` refuses it before reaching any
    #: state at all — the cheapest genuine startup failure charter's own Python can see,
    #: and one of the two the issue names.
    _BAD_SLOT = "nosuchslot"

    #: What that refusal writes to stderr, byte for byte (`frame/panel.py`'s `run`).
    #: Shared by the two tests below so the control below fails for the same REASON, and
    #: differs only in what the process does after printing it.
    _BAD_SLOT_STDERR = (f"charter panel: unknown slot '{_BAD_SLOT}' "
                        f"(known: bottom, left, right, top)")

    #: A pane program that waits for a gate file, prints *stderr text* and exits 2 — the
    #: pre-#382 panel, reduced to the only two things about it that mattered. The gate
    #: is what makes `remain-on-exit` arm-able in time: `set -w` cannot be issued before
    #: the session exists, and without the gate the process would already be gone.
    _DYING_PROGRAM = ("import os, sys, time\n"
                      "while not os.path.exists(sys.argv[1]): time.sleep(0.02)\n"
                      "print(sys.argv[2], file=sys.stderr)\n"
                      "sys.exit(2)\n")

    def _capture(self, target: str, *history: str) -> str:
        """What `capture-pane` reports for *target* — the VISIBLE screen by default,
        which is the whole point: it is what an operator sees without knowing to scroll
        a one-row pane's history. Pass `"-S", "-3"` to look into that history instead."""
        return _tmux("capture-pane", "-p", *history, "-t", target).stdout

    def _dead(self, target: str) -> str:
        return _tmux("display-message", "-p", "-t", target, "#{pane_dead}").stdout.strip()

    def _remain_on_exit(self, session: str) -> None:
        """Arm `remain-on-exit` for *session*'s own window, the way a real frame has it
        armed from its very first moment (`commands_frame._PLACEHOLDER_CONF`).

        Window-scoped (`set -w -t`), not `set -g`: this class runs several sessions on
        one socket in sequence, and a global option would outlive the test that set it
        and change how every LATER test's pane behaves when its process ends — the exact
        cross-test leak the module docstring gives each test its own session to avoid."""
        r = _tmux("set", "-w", "-t", session, "remain-on-exit", "on")
        self.assertEqual(r.returncode, 0, r.stderr)

    def _wait_for(self, target: str, needle: str, timeout: float = _DEADLINE) -> str:
        """Poll `capture-pane` until *needle* is on the visible screen, returning the
        last capture either way — the same "poll for content, never sleep a guess"
        shape `_await_file` uses, so a slow runner is slow rather than wrong."""
        deadline = time.monotonic() + timeout
        seen = self._capture(target)
        while time.monotonic() < deadline and needle not in seen:
            time.sleep(0.1)
            seen = self._capture(target)
        return seen

    def test_a_panel_that_cannot_start_paints_the_reason_into_a_one_row_pane(self):
        """#382's FIRST half, against a real tmux at the real size — the half that cost
        a debugging session, and the half nothing until now actually observed.

        Every other test of `_hold` in this suite calls `panel.run(..., once=True)`
        in-process and reads the return value, which proves the refusal happens and
        proves nothing at all about what an operator sees: the failure being fixed is
        specifically that the reason DID reach the pane and was then scrolled out of it
        by tmux's own dead-pane message. So this spawns the real `charter panel` argv
        into a real ONE-ROW pane (`layout.SLOT_SIZE["bottom"]`) with `remain-on-exit`
        armed exactly as a real frame arms it, and asserts on `capture-pane` — what is
        on the screen — not on a return code.

        Two assertions, and both are load-bearing. The reason must be READABLE, and the
        pane must still be ALIVE (`#{pane_dead}` `0`): a panel that painted the reason
        and then exited would satisfy the first for a few milliseconds and lose it to
        `Pane is dead (status 2)` immediately after, which is precisely the pre-#382
        behaviour. The companion test below is the control for the second half of that
        sentence — it measures what this pane WOULD have shown had `_hold` returned.
        """
        fid = state.frame_id("panel-integ-badslot", os.getpid())
        self._spawn_panel("panel-badslot", fid, slot=self._BAD_SLOT, rows=1)
        self._remain_on_exit("panel-badslot")

        needle = f"unknown slot '{self._BAD_SLOT}'"
        seen = self._wait_for("panel-badslot", needle)
        self.assertIn(needle, seen,
                      "a panel that cannot start left nothing readable in its own "
                      f"one-row pane — all the operator has is: {seen!r}")
        self.assertEqual(self._dead("panel-badslot"), "0",
                         "the panel painted its reason and then EXITED — tmux is about "
                         "to write `Pane is dead (status 2)` over the only row this "
                         "pane has, which is the whole failure #382 is about")

    def test_the_same_reason_on_stderr_alone_is_scrolled_out_of_a_one_row_pane(self):
        """The control, and the measurement `_hold` exists because of.

        Without this, the test above proves only "text can appear in a pane" — it does
        not establish that the OLD shape (print the reason, exit) genuinely fails, so a
        reader has no way to tell whether `_hold` bought anything. Here the identical
        reason string is printed to stderr by a process that then exits 2, in a pane of
        the identical size, on the same tmux:

        * the VISIBLE screen does NOT hold the reason — the operator's whole view, the
          `Pane is dead (status 2)` of the field report;
        * the reason IS in the pane's history one line up. That second assertion is what
          makes this a measurement rather than a guess: it rules out the other
          explanation for an empty screen — that a panel's stderr never reaches its pane
          at all, which is what the issue originally reported and which would call for a
          completely different fix. It reaches the pane, and the pane cannot hold it.

        **Neither tmux's dead-pane MESSAGE nor the exit status tmux records is asserted
        here, and that is a measurement rather than a concession.** An earlier version
        of this test asserted `Pane is dead` was on the visible screen, guarded by a
        probe of `remain-on-exit-format` — which is an option, and says only what tmux
        WOULD write. It went green on tmux 3.7c and on three of the four `ubuntu-latest`
        jobs, and failed the fourth with `capture-pane` reporting a single blank line
        for a pane tmux had already answered `#{pane_dead}` `1` for. Asserting
        `#{pane_dead_status}` instead — the structural half of the same sentence — would
        have failed the same job for the same reason.

        Re-measured against tmux 3.4 in an Ubuntu 24.04 container, running this exact
        fixture. On an idle box it behaves exactly like 3.7c: `#{pane_dead_status}` `2`,
        the message in the pane, every time. Pinned to ONE cpu against four spin loops —
        a busy shared runner — 11 of 120 deaths, and 5 of a further 60, ended
        `#{pane_dead}` `1` with an EMPTY status and NOTHING written into the pane. And
        permanently, not briefly: polling the status for a further 8 seconds never
        filled it in. It is the same shape `_gate_argv`'s own docstring already records
        for 3.4 — dead pane, empty status, `pane-died` never fires — from tmux 3.4's
        `server_destroy_pane` not having the child's status when the pane's fd closes.
        tmux 3.7c writes both, every time.

        So on the two tmuxes this suite must pass on, the fd closing (`#{pane_dead}`) is
        the only thing tmux reports about a death that can be relied on, and everything
        else below is read from the pane's own CONTENT. Nothing is widened to accept two
        outcomes: the version-dependent facts are not asserted weakly, they are not
        asserted at all, because they are not what this test measures. (It is also why
        `TmuxIntegration`'s exit-status tests wait on the FILE a `pane-died` hook wrote
        rather than on `#{pane_dead}` — that file cannot appear until tmux has the
        child's status, so it is a synchronisation point where `#{pane_dead}` is not.)
        The three assertions this test does keep were then run 60 times under that same
        one-cpu load: 3 runs landed in the window above, and none of the three
        assertions moved.

        **One row is the fixture's load-bearing half, and for a sharper reason than
        "small".** Measured against tmux 3.7c with the process still ALIVE and no death
        to write a message about yet: this 74-column reason, `print`ed to a 40-column
        ONE-row pane, leaves the visible screen ALREADY blank — the wrap and the print's
        own trailing newline each scroll the pane's only row into history. So at the
        size `top` and `bottom` actually are (`layout.SLOT_SIZE`: 1) the operator's
        nothing does not wait for the death, and does not depend on which tmux is
        running: the dead-pane message, where a tmux writes one, lands on a row that was
        already empty. It is also why `panel._write` — the one path `_hold` paints
        through — ends its write with NO trailing newline: at this geometry a newline is
        indistinguishable from never having painted.

        Bigger panes are where tmux's own behaviour starts to matter, which is exactly
        why this fixture does not use one, and the mutation check for this assertion is
        that boundary: at `-y 6` with one filler line printed AHEAD of the reason, the
        reason's first wrapped line — the one carrying the slot name — survives on the
        visible screen and this assertion goes red. (`-y 6` alone does NOT flip it on
        3.7c: measured, tmux's one-line scroll evicts precisely that first wrapped line
        and leaves the second, so the assertion still passes for a reason that has
        nothing to do with what it claims. The filler line is what moves the slot name
        clear of that scroll.)
        """
        gate_dir = tempfile.mkdtemp(prefix="charter-integ-gate-")
        self.addCleanup(shutil.rmtree, gate_dir, True)
        gate = os.path.join(gate_dir, "die")

        r = _tmux("new-session", "-d", "-s", "panel-oldshape", "-x", "40", "-y", "1",
                  "--", sys.executable, "-c", self._DYING_PROGRAM, gate,
                  self._BAD_SLOT_STDERR, env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.addCleanup(_kill_pid, _tmux("display-message", "-p", "-t",
                                         "panel-oldshape", "#{pane_pid}").stdout.strip())
        self._remain_on_exit("panel-oldshape")
        Path(gate).write_text("x")

        deadline = time.monotonic() + _DEADLINE
        while time.monotonic() < deadline and self._dead("panel-oldshape") != "1":
            time.sleep(0.1)
        self.assertEqual(self._dead("panel-oldshape"), "1",
                         "the control process never died — nothing is being measured")

        visible = self._capture("panel-oldshape")
        self.assertNotIn(self._BAD_SLOT, visible,
                         "the reason survived on the visible screen of a ONE-row pane — "
                         "a pane that small no longer loses a newline-terminated write, "
                         "so `panel._hold` is solving a problem that no longer exists: "
                         f"{visible!r}")
        with_history = self._capture("panel-oldshape", "-S", "-3")
        self.assertIn(self._BAD_SLOT, with_history,
                      "the reason never reached the pane AT ALL — that is a different "
                      "failure from the one #382 fixes, and `_hold` would not cure it: "
                      f"{with_history!r}")

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

        deadline = time.monotonic() + _DEADLINE
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

        deadline = time.monotonic() + _DEADLINE
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

    Substitutes `sys.executable` for the `"$CHARTER_PY"` `menu_argv` embeds, rather
    than tying it to the session — this class fires the action through `run-shell -t
    fid` directly instead of through a rendered menu, so it is writing the command
    itself and can simply name the interpreter. (`MenuFormatIntegration` below, which
    does drive a real rendered menu, sets `CHARTER_PY` on the session instead and
    leaves `menu_argv`'s own text untouched; `MenuClientIntegration` goes further and
    proves it with no `charter` on `$PATH` at all.) Either way, never a bare `charter`:
    this developer machine's own `$PATH` may resolve one to an entirely different
    install than this checkout (confirmed on the machine this test was written on), and
    `PanelIntegration` above avoids the identical trap the same way for `charter
    panel`."""

    def setUp(self) -> None:
        super().setUp()
        # kill-server THEN unlink — see `PanelIntegration._teardown_socket`'s own
        # docstring for why two separately-registered `addCleanup` calls in that
        # order run backwards (`addCleanup` is LIFO).
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)

    def _teardown_socket(self) -> None:
        """Kill this class's own tmux server, THEN unlink its socket file — see
        `PanelIntegration._teardown_socket` for the full reasoning, identical here."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

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

        # The exact template `menu_argv` embeds for this item, `"$CHARTER_PY"` swapped
        # for `sys.executable` (see the class docstring) — everything else, including
        # the absence of any label text, matches byte for byte. `client` (this test's
        # own no-op stand-in) only affects `-c`, never the per-item action text this
        # line inspects.
        real_command = menu.menu_argv(fid, SOCKET, client="")[-1]
        self.assertNotIn("pwned", real_command,
                         "sanity: the hostile label must not have leaked into the "
                         "action text this integration test is about to run for real")
        action = f"{sys.executable} -m charter frame-action {action_id}"
        r = _tmux("run-shell", "-t", fid, action)
        self.assertEqual(r.returncode, 0, r.stderr)

        deadline = time.monotonic() + _DEADLINE
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
class MenuFormatIntegration(_NeedsAttachedClient, PersonaIso, unittest.TestCase):
    """The CRITICAL finding from the first review round, proved against a REAL,
    RENDERED `display-menu` — not only the argv shape `tests/test_frame_menu.py`'s
    `LabelSafety` checks. `display-menu`'s own docs: "The name and command are formats"
    — a menu item's NAME is a tmux FORMAT, not inert text, and an unescaped `#(shell
    command)` label runs the command the INSTANT tmux DRAWS the menu, no selection
    needed (confirmed by hand: the hostile row was invisible in the rendered menu —
    nothing about the menu LOOKED wrong, only the canary gave it away).

    A REAL, ATTACHED client is unavoidable here — `display-menu` refuses outright ("no
    current client") without one — which is why every OTHER test in this file avoids
    attaching one at all. `_NeedsAttachedClient._attach_pty` is the one place in the
    suite that does, and the one place that decides this machine cannot.

    **A THROWAWAY plane, `PanelIntegration`'s shape exactly, and it was not one until
    now.** Every test here calls `menu.record`, which goes `menu._table(fid,
    create=True)` -> `state.frame_dir(fid, create=True)` -> `config.STATE_DIR` — so
    without `PersonaIso` this class MINTED `menu-fmt-integ*-<pid>` directories in the
    developer's REAL `.charter/frame/` and left them there, three per run, measured by
    listing that directory before and after. Nothing reaps them either: since #383 a
    directory whose name ends in a live pid is kept, and while the suite is running that
    pid is the suite's own.

    `PersonaIso` alone only covers what a Python IMPORT of `charter` reads in THIS
    process, and one test below (`test_an_escaped_label_still_lets_the_real_action_run_
    when_selected`) fires a REAL `charter frame-action` SUBPROCESS that has to find the
    very table `menu.record` just wrote. `$CHARTER_ROOT` carries the same throwaway plane
    across that boundary — passed to the `new-session` that STARTS this class's server,
    because a `run-shell` with no `-t` inherits the SERVER's own starting process
    environment (`commands_frame._session_id_env_argv`'s own docstring measures this) —
    and `charter.toml` has to exist at that path or `root.find_root` refuses it outright.
    """

    def setUp(self) -> None:
        super().setUp()
        # kill-server THEN unlink — see `PanelIntegration._teardown_socket`'s own
        # docstring for why two separately-registered `addCleanup` calls in that
        # order run backwards (`addCleanup` is LIFO). Registered AFTER
        # `PersonaIso.setUp`'s own plane cleanup, so LIFO runs it FIRST: the tmux
        # server dies while the plane it was pointed at still exists.
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)  # let it derive under CHARTER_ROOT, like this
                                            # process's own PersonaIso-isolated config

    def _teardown_socket(self) -> None:
        """Kill this class's own tmux server, THEN unlink its socket file — see
        `PanelIntegration._teardown_socket` for the full reasoning, identical here."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _new_session(self, fid: str) -> None:
        """A fresh `cat`-backed session named *fid*, with its pane's OWN pid registered
        for cleanup — `kill-server` alone (already in `setUp`) is documented elsewhere in
        this file (`_kill_pid`'s own docstring) as unreliable at reaping a still-running
        pane's process in time; `PanelIntegration`/`MenuIntegration` above both work
        around it the same way, and this class needs the identical fix (confirmed by
        hand: without this, three "cat" processes were still alive under `pgrep -f
        'tmux -L charter-integration-test'` after this class's own tests finished and
        `kill-server` had already run).

        The one call in this class that carries `self.env`: it is the call that STARTS
        the server, and a `run-shell` with no `-t` reads the server's own starting
        environment, which is how the `charter frame-action` subprocess a menu selection
        spawns lands on this test's throwaway plane rather than the developer's."""
        r = _tmux("new-session", "-d", "-s", fid, "-x", "80", "-y", "24", "cat",
                  env=self.env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = _tmux("display-message", "-p", "-t", fid, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

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

        `\x1bOQ` (application-mode F2) is the ONLY sequence written — a second one
        (`\x1b[OQ`, normal-mode F2) was tried "for safety" in an earlier version of
        this helper and is exactly the mistake it looks like: its own leading ESC can
        dismiss the menu the first sequence just opened, and the trailing `[OQ` then
        leaks into the pane as literal text instead of being interpreted as a key at
        all. It passed five runs in a row before this was caught — a single, correct
        sequence is what actually works, not a second one "just in case".
        """
        bind_cmd = ["tmux", "-L", SOCKET, "bind", "-n", "F2"] + cmd[3:]
        r = subprocess.run(bind_cmd, capture_output=True, text=True, timeout=5)
        self.assertEqual(r.returncode, 0, r.stderr)
        os.write(fd, b"\x1bOQ")
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
        # The item's own action reads `"$CHARTER_PY" -m charter frame-action a0` — no
        # string substitution needed here any more (an earlier version rewrote a bare
        # `charter`, which may not be this checkout on `$PATH`). Setting the session's
        # own `CHARTER_PY` is what production does, so this drives the real mechanism
        # rather than a rewrite of it: `display-menu -t <session>` scopes the item's
        # `run-shell` to this session's environment.
        self.assertEqual(_tmux("set-environment", "-t", fid, "CHARTER_PY",
                              sys.executable).returncode, 0)
        cmd = menu.menu_argv(fid, SOCKET, client=client)
        self.assertIn("$CHARTER_PY", cmd[-1])
        self._drive_menu(fd, cmd)

        self.assertFalse(os.path.exists(format_canary),
                         "the hostile label must not have executed merely by being "
                         "rendered")

        os.write(fd, b"1")
        deadline = time.monotonic() + _DEADLINE
        while time.monotonic() < deadline and not os.path.exists(real_canary):
            time.sleep(0.2)
        self.assertTrue(os.path.exists(real_canary),
                        "selecting the (escaped, still-usable) item must still run "
                        "its real, opaque-id-resolved action")
        self.assertFalse(os.path.exists(format_canary),
                         "selecting the item must not retroactively execute the label")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class MenuClientIntegration(_NeedsAttachedClient, PersonaIso, unittest.TestCase):
    """Fix round 2, IMPORTANT-1, proved with two real ptys attached to ONE frame at
    once: the hotkey must open the menu on the PRESSER's own screen, never a
    different attached client's — the defect an earlier version of this mechanism
    introduced by picking `list-clients`'s first-reported client instead of asking
    the bind itself who pressed. See `commands_frame.cmd_menu`'s own docstring for
    what was, and was not, directly observed before the fix.

    Drives the REAL production chain end to end — `conf_text`'s own bind text,
    `source-file`'d for real, firing a REAL `charter frame-menu` subprocess via
    `run-shell`, which calls the REAL `cmd_menu` -> `menu.menu_argv` ->
    `display-menu`. Two things a hand-rolled stand-in could not exercise:

    1. `commands_frame.SOCKET` is a hardcoded module constant ("charter", the real
       production socket) — this suite never uses that socket (every OTHER class
       here runs on its own pid-scoped one, precisely to avoid `kill-server` ever
       reaching a real, unrelated frame). A subprocess spawned bare cannot be told
       a different socket via any argv this module controls. `_charter_shim` is a
       tiny executable Python script, standing in for `charter` on `$PATH`, that
       monkeypatches `commands_frame.SOCKET` to this class's own socket before
       dispatching to the real `cli.main` — the one substitution that has to happen
       BEFORE `cmd_menu`/`cmd_action` run, not on a command string built inside
       them (unlike `MenuIntegration`/`MenuFormatIntegration` above, which construct
       the command themselves and can rewrite `charter` -> `sys.executable -m
       charter` directly; `cmd_menu` builds ITS OWN `display-menu` call internally,
       leaving no string here to rewrite after the fact).
    2. `tmux display-menu`, issued directly rather than from a binding, does not
       reliably wire itself to a target client's own key input the same way a
       key-bound one does — confirmed by hand while building this: calling
       `cmd_menu` in-process, in a background thread, with `SOCKET` mocked and no
       subprocess involved at all, returned immediately, and no keypress from
       either client could select anything afterward. The same "direct invocation
       behaves differently from a bind" property `MenuFormatIntegration` above
       already documents for label rendering, evidently not limited to format
       expansion. The REAL bind is not a nicety here; it is the only path observed
       to work at all.

    A THROWAWAY plane, for the same reason and by the same mechanism as
    `MenuFormatIntegration` above: `menu.record` reaches `config.STATE_DIR`, so without
    `PersonaIso` this class minted a `menu-client-integ-<pid>` directory in the
    developer's REAL `.charter/frame/` on every run. This class already builds its own
    environment for the `new-session` that starts its server (`_charter_py_env`, for the
    scrubbed `$PATH`), so `$CHARTER_ROOT` is carried there rather than in a second one —
    and it has to reach the server's STARTING environment specifically, because that is
    what the real bind's own `run-shell` inherits.
    """

    def setUp(self) -> None:
        super().setUp()
        # kill-server THEN unlink — see `_teardown_socket`'s own docstring below:
        # THIS class, unlike its siblings above, is not merely defending against a
        # hypothetical here. Registered AFTER `PersonaIso.setUp`'s own plane cleanup,
        # so LIFO runs it FIRST — the server dies while the plane it reads still exists.
        self.addCleanup(self._teardown_socket)
        # `$CHARTER_ROOT` is refused outright without this marker (`root.find_root`
        # raises rather than falling back), which would leave the real bind's own
        # `charter frame-menu` subprocess failing instead of reading this test's plane.
        (self.tmp / "charter.toml").write_text("")

    def _teardown_socket(self) -> None:
        """Kill this class's own tmux server, THEN unlink its socket file.

        Two separately-registered `addCleanup` calls (`kill-server`, then `unlink`)
        run LIFO — unlink FIRST, kill-server SECOND — and a `kill-server` issued
        after the socket's own directory entry is already gone cannot reconnect to
        the still-live server at all (see `FourEdgeIntegration.setUp`'s own
        docstring for the measured leak this produces). That reversed order is
        harmless for `PanelIntegration`/`MenuIntegration`/`MenuFormatIntegration`
        above only because none of THEIR sessions arm `remain-on-exit`. **This
        class is the one exception, and arms it globally, not incidentally**:
        `_install_real_bind` below sources `commands_frame.conf_text(...)` for
        real, which emits `set -g remain-on-exit on` (`commands_frame.py`) —
        server-wide, not scoped to one session. It survives today with the two-call
        ordering bug anyway (confirmed by removing only `_new_session`'s own
        `addCleanup(_kill_pid, server_pid)`, which alone still leaves a PPID-1
        orphan) because `_new_session` ALSO SIGKILLs the server's own process
        directly by pid — a second, independent teardown path that has nothing to
        do with `kill-server` succeeding or failing. This fix removes the
        dependency on that second path rather than continuing to rely on it being
        present on every test added to this class in the future.
        """
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _charter_py_env(self) -> dict:
        """The environment this class's tmux server starts with: an INTERPRETER shim
        for `$CHARTER_PY`, and a `$PATH` with no `charter` on it at all.

        Both halves matter, and the second is why this replaced an earlier `$PATH` shim
        that stood in for a bare `charter` executable. That shim papered over the very
        requirement this class is now the end-to-end proof of: the production bind ran
        a bare `charter` off `$PATH`, and the suite supplied one, so nothing ever
        observed what happens on a machine where `$PATH` has no charter — `run-shell`
        prints `'charter frame-menu "…"' returned 127` INTO THE HARNESS PANE and drops
        it into copy-mode, charter drawing in the one rectangle ADR 0018 says it never
        draws. Scrubbing `$PATH` to a single empty directory means a bare `charter`
        cannot possibly resolve here, so this class fails if the fix is ever reverted
        rather than passing on the old shim's charity.

        The shim itself still exists for the reason the old one did, unchanged:
        `commands_frame.SOCKET` is a hardcoded module constant ("charter", the real
        production socket) and a subprocess spawned by tmux cannot be told a different
        one through any argv this module controls. It accepts exactly the `-m charter
        …` argv the real interpreter would, monkeypatches `SOCKET`, and dispatches to
        the real `cli.main`. `#!` uses `sys.executable` directly and sets `sys.path`
        explicitly, because a script executed by its own shebang gets ITS OWN directory
        as `sys.path[0]`, not this checkout's.
        """
        repo_root = str(Path(__file__).resolve().parent.parent)
        shim_dir = Path(tempfile.mkdtemp(prefix="charter-integ-shim-"))
        self.addCleanup(shutil.rmtree, shim_dir, True)
        shim = shim_dir / "charter-py"
        shim.write_text(
            f"#!{sys.executable}\n"
            "import sys\n"
            f"sys.path.insert(0, {repo_root!r})\n"
            "argv = sys.argv[1:]\n"
            "assert argv[:2] == ['-m', 'charter'], argv\n"
            "import charter.commands_frame\n"
            f"charter.commands_frame.SOCKET = {SOCKET!r}\n"
            "from charter.cli import main\n"
            "sys.exit(main(argv[2:]))\n"
        )
        shim.chmod(0o755)
        # A `$PATH` holding exactly one thing: tmux. Not an EMPTY directory — this
        # environment is also what the `_tmux("new-session", …)` call that starts the
        # server runs under, and it has to be able to find the binary it is starting.
        # Symlinked into a throwaway directory rather than reusing tmux's own (which on
        # this machine is a package manager's `bin`, and may well hold a real `charter`
        # — the exact thing this must not silently supply).
        bare = Path(tempfile.mkdtemp(prefix="charter-integ-nopath-"))
        self.addCleanup(shutil.rmtree, bare, True)
        (bare / "tmux").symlink_to(shutil.which("tmux"))
        self.assertIsNone(shutil.which("charter", path=str(bare)),
                          "this class proves a bare `charter` is never needed — its "
                          "own $PATH must not contain one")
        self.shim = shim
        # `$CHARTER_ROOT` rides along here rather than in a second environment: this is
        # the env the server STARTS under, and the real bind's `run-shell` (no `-t`)
        # inherits exactly that. `$CHARTER_HOME` is dropped so `STATE_DIR` derives under
        # the throwaway root the way this process's own PersonaIso-isolated config does —
        # left in place it would win outright (`config._migrate_state_dir` returns it
        # verbatim) and put the shim right back on the developer's real plane.
        env = dict(os.environ, PATH=str(bare), CHARTER_ROOT=str(self.tmp),
                   CHARTER_WORKSPACE="demo")
        env.pop("CHARTER_HOME", None)
        return env

    def _new_session(self, fid: str) -> None:
        """The one `new-session` call that starts this class's fresh server —
        carries the scrubbed `$PATH` (see `_charter_py_env`). `/bin/cat`, absolute,
        because that `$PATH` has nothing on it. `run-shell` (no explicit `-t`, matching
        both the bind's own action and every per-item action) falls back to the SERVER's
        own starting process environment (see `_session_id_env_argv`'s own docstring),
        so this is the only call in this class that needs the special environment at
        all — `$CHARTER_PY` itself is tied to the SESSION, the way `cmd_launch` ties
        it.

        Kills both the pane's own process AND the server's own process directly
        (`#{pid}`, tmux's own server-PID format) rather than relying on `setUp`'s
        `kill-server` alone: confirmed by hand that this class's own heavier setup
        (two attached ptys, a `charter` shim subprocess per keypress) left a bare,
        childless, PPID-1 server behind `kill-server` more than once — the same
        "kill-server does not reliably reap in time" property `_kill_pid`'s own
        docstring already names for a session's PANE process, evidently reachable
        for the SERVER process too under this class's own load. `kill-server`
        stays in `setUp` as a harmless, already-a-no-op-by-then fallback."""
        r = _tmux("new-session", "-d", "-s", fid, "-x", "80", "-y", "24", "/bin/cat",
                 env=self._charter_py_env())
        self.assertEqual(r.returncode, 0, r.stderr)
        server_pid = _tmux("display-message", "-p", "-t", fid, "#{pid}").stdout.strip()
        self.addCleanup(_kill_pid, server_pid)
        pid = _tmux("display-message", "-p", "-t", fid, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

    def _install_real_bind(self, fid: str) -> None:
        """`conf_text`'s own bind line, `source-file`'d for real — byte for byte, no
        substitution of any kind. The bind reads `"$CHARTER_PY" -m charter frame-menu
        "#{client_name}"`, and `$CHARTER_PY` is tied to this session by the
        `set-environment` call in the test below, exactly as `cmd_launch` does it."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session=fid)
        conf = Path(tempfile.mkdtemp(prefix="charter-integ-clientconf-")) / "tmux.conf"
        self.addCleanup(shutil.rmtree, conf.parent, True)
        conf.write_text(text)
        r = _tmux("source-file", str(conf))
        self.assertEqual(r.returncode, 0, r.stderr)

    def _short_canary(self, name: str) -> str:
        path = f"/tmp/chci-{os.getpid()}-{name}"

        def _cleanup():
            try:
                os.remove(path)
            except OSError:
                pass
        self.addCleanup(_cleanup)
        return path

    def test_each_presser_gets_their_own_menu_not_the_others(self):
        fid = f"menu-client-integ-{os.getpid()}"
        self._new_session(fid)
        clientA, fdA = self._attach_pty(fid)
        clientB, fdB = self._attach_pty(fid, exclude=frozenset({clientA}))
        self.assertNotEqual(clientA, clientB)
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session=fid)).returncode, 0)
        # `$CHARTER_PY`, carried the same out-of-band way `cmd_launch` carries it —
        # the shim standing in for `sys.executable` so `commands_frame.SOCKET` lands on
        # THIS class's own socket rather than the real one (see `_charter_py_env`).
        self.assertEqual(_tmux("set-environment", "-t", fid, "CHARTER_PY",
                              str(self.shim)).returncode, 0)
        self._install_real_bind(fid)

        # -- A presses: only A's own keystream may select what A's own press opened --
        canary_a = self._short_canary("a")
        menu.record(fid=fid, entries=[
            ("Detach", [sys.executable, "-c", f"open({canary_a!r}, 'w').close()"]),
        ])
        os.write(fdA, b"\x1bOQ")
        time.sleep(0.8)
        os.write(fdB, b"1")
        time.sleep(0.8)
        self.assertFalse(os.path.exists(canary_a),
                         "client B's own keystream selected an item in the menu A's "
                         "own hotkey opened — the menu did not open specifically on "
                         "A's screen (the exact regression this round fixes)")
        os.write(fdA, b"1")
        deadline = time.monotonic() + _DEADLINE
        while time.monotonic() < deadline and not os.path.exists(canary_a):
            time.sleep(0.2)
        self.assertTrue(os.path.exists(canary_a),
                        "A's own hotkey press must open A's own, selectable menu")

        # -- Now B presses: the reverse must hold too, not just one direction --
        canary_b = self._short_canary("b")
        menu.record(fid=fid, entries=[
            ("Detach", [sys.executable, "-c", f"open({canary_b!r}, 'w').close()"]),
        ])
        os.write(fdB, b"\x1bOQ")
        time.sleep(0.8)
        os.write(fdA, b"1")
        time.sleep(0.8)
        self.assertFalse(os.path.exists(canary_b),
                         "client A's own keystream selected an item in the menu B's "
                         "own hotkey opened")
        os.write(fdB, b"1")
        deadline = time.monotonic() + _DEADLINE
        while time.monotonic() < deadline and not os.path.exists(canary_b):
            time.sleep(0.2)
        self.assertTrue(os.path.exists(canary_b),
                        "B's own hotkey press must open B's own, selectable menu")


def _git(repo: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)


def _init_repo(path: Path, branch: str) -> Path:
    """A real git repo with one commit, on *branch* — the same fixture shape
    `tests/test_frame_gather.py`'s own `_init_repo` uses, duplicated here (rather
    than imported across test modules) so this module stays as self-contained as
    every other fixture in it already is. `FourEdgeIntegration` below is the only
    caller: it needs a repo a real `charter panel left --session <fid>` subprocess
    can gather for itself, through `gather.scan`, exactly the way an operator's own
    clone would be gathered."""
    path.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    _git(path, "config", "user.email", "t@example.com")
    _git(path, "config", "user.name", "t")
    _git(path, "config", "gc.auto", "0")
    _git(path, "config", "maintenance.auto", "false")
    (path / "README.md").write_text("hello\n")
    _git(path, "add", "README.md")
    _git(path, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
    return path


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class FourEdgeIntegration(PersonaIso, unittest.TestCase):
    """Task 5 (#385), the closing proof for this whole plan: a frame configured with
    ALL FOUR slots comes up with all four panes alive and drawing REAL content, and
    repaints after a real `state.bump`. Tasks 1-4 were each unit-tested — a mocked
    `scan()`, an in-process cache read, a renderer called directly against a fixed
    `width`. Nothing before this class has ever run `gather.scan`,
    `notify.plane_changed`, `slots.render` and a real tmux pane together, in the
    same process tree, against a real git repo — this is the one place the whole
    composition (gather -> cache -> hook -> panel -> renderer -> a pane an operator
    can actually read) is proven at once.

    `PanelIntegration` above already proves ONE panel end to end (`bottom`, driven
    by a direct `charter panel bottom --session <fid>` new-session). This class
    proves the COMPOSITION `layout.panel_argvs` exists for instead: four real
    splits off the SAME harness pane id, in the same launch — the exact multi-split
    scenario `layout.py`'s own module docstring names as the index-churn hazard
    pane ids were built to close (tmux renumbers pane INDICES on every split;
    `PanelIntegration` only ever creates one pane total, so it can never exercise a
    second or third split landing on the wrong rectangle). `_spawn_frame` below
    calls `layout.session_argv` then `layout.panel_argvs` — the same two real,
    production argv-building functions `commands_frame.cmd_launch` calls, run here
    without the parts of `cmd_launch` this task is not about (the hotkey menu, the
    exit-code hooks) — those are already proven end to end by `TmuxIntegration`/
    `MenuIntegration`/`MenuFormatIntegration`/`MenuClientIntegration` above, and
    duplicating them here would only be a second, weaker copy.

    **Only `left` is asserted through content that could only have come from the
    cache Tasks 1/2 built.** `top` (workspace/persona/version), `right` (persona
    chips) and `bottom` (todo count/alerts) all read live at render time —
    `slots.py`'s own docstrings for `_top`/`_right`/`_bottom` each say so — real
    subprocess, real data, but none of it through `gather.py`'s cache. `left` is
    the one slot `slots._left` reads EXCLUSIVELY from `gather.read(fid)` (see that
    function's own docstring: "never a repo directory listing, a `git status`, or a
    `glstate.read_for` of its own"), so a real repo's real branch name showing up
    in a captured `left` pane is the one assertion in this whole file that proves
    the actual thing this plan is for: a real `git` sweep, gathered ONCE, cached to
    disk by a hook, and read back by a panel process that never itself calls git —
    a panel showing "no repos" would be ALIVE and would tell you nothing about
    whether any of that chain actually works (see this module's own task brief).

    **Fix round 1 closed three links this proof left unproven, all the same shape
    (green even with the actual mechanism disabled):** a real, uncommitted file in
    the fixture repo (mutation testing found `left`'s row shows a clean repo as
    clean whether or not `gather.scan`'s own `_repo_states` sweep — the real `git
    status --porcelain --branch` half — ever ran at all, since name/branch alone
    come from `_repo_trees`/`_branch`, neither of which touches it); a direct read
    of the cache FILE (`gather._cache_file`), not only the pane's own text, both
    right after priming and right after the real hook fires (mutation testing
    found `gather.save` could be neutered outright and every content assertion
    still passed, because `gather.read`'s own missing-cache fallback recomputes a
    live scan whenever there is nothing to read).

    The repaint proof mutates the repo's BRANCH, not its dirty bit, deliberately:
    `gather.py`'s `_branch` reads `.git/HEAD` straight, with no cache of its own,
    while dirty/ahead/behind ride `statusline._repo_states`' 5-second TTL
    (`_STATE_TTL`), keyed by directory path and blind to a file appearing between
    two gathers taken seconds apart — a dirty-bit mutation immediately after the
    frame's first (TTL-warming) gather would be flaky against the very caching
    behaviour this plan depends on, for a reason that has nothing to do with
    whether the repaint actually worked. A branch switch has no such cache to race
    — which is exactly why the ALIVE test above needs its OWN, separate dirty file
    (created before the frame ever launches, so no TTL race is possible) to prove
    the `_repo_states` half at all.

    **Capability, not presence — this module's own opening promise, checked for
    this class specifically.** Every tmux primitive `_spawn_frame`/the tests above
    use — `new-session -x/-y -P -F`, `split-window -t <pane-id> -l N`,
    `select-pane`, `#{pane_dead}`/`#{pane_active}`/`#{pane_pid}`,
    `capture-pane -p`, `set -g remain-on-exit` — predates charter's own declared
    floor (`tmuxctl.FLOOR`, tmux 3.2), which CI's tmux 3.4 already clears; none of
    it is gated behind a version this class would need to probe. Unlike
    `MenuFormatIntegration`/`MenuClientIntegration` above, nothing here attaches a
    real client (`display-menu`'s "no current client" refusal, and `TERM=dumb`'s
    inability to give it one, are why those two classes need
    `_NeedsAttachedClient` at all) — `capture-pane`/`display-message` read a
    pane's state and content directly, needing no client attached to it, so
    `TERM=dumb`'s specific failure mode (a pty-attached CLIENT that cannot
    register) never applies to this class. Confirmed directly, not merely
    inferred: this whole module, `FourEdgeIntegration` included, passes 17/17
    under `TERM=dumb` with nothing skipped. `_HAS_TMUX` (presence) is therefore
    the right and only gate for this class — no narrower capability probe is
    missing here the way one was for the classes this module's own opening
    docstring describes.
    """

    def setUp(self) -> None:
        super().setUp()
        # ONE combined cleanup, kill-server THEN unlink — never two separately
        # registered `addCleanup` calls in that order (`addCleanup` runs LIFO, so
        # that shape runs unlink FIRST, kill-server SECOND — backwards). Every
        # class in this module now uses this same combined shape (see
        # `MenuClientIntegration._teardown_socket` for the measured leak the
        # reversed order produces once a session's `remain-on-exit` is armed,
        # which is this class's own situation too: `_spawn_frame` writes
        # `commands_frame._PLACEHOLDER_CONF` — `set -g remain-on-exit on` — into
        # its `-f` config, the same placeholder `cmd_launch` itself loads, so a
        # crashed panel leaves an inspectable pane rather than vanishing. With it
        # on, killing every pane's own process (`_kill_pid`, in `_spawn_frame`)
        # does NOT end the session; only an explicit `kill-session`/`kill-server`
        # does, and unlike `MenuClientIntegration` this class has no second,
        # independent teardown path (a directly-SIGKILLed server pid) to fall
        # back on if the ordering is wrong — getting it right here is the only
        # thing standing between a `sleep 600` harness pane and an orphan).
        self.addCleanup(self._teardown_socket)
        (self.tmp / "charter.toml").write_text("")
        self.env = dict(os.environ, CHARTER_ROOT=str(self.tmp), CHARTER_WORKSPACE="demo")
        self.env.pop("CHARTER_HOME", None)  # derive STATE_DIR under CHARTER_ROOT, like
                                            # this process's own PersonaIso-isolated config
        self.env = _importable_env(self.env)  # the panel argv carries -P — see the helper

    def _teardown_socket(self) -> None:
        """Kill this class's own tmux server, THEN unlink its socket file — see
        `setUp`'s own comment for why the order is load-bearing here specifically."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _run_env(self, cmd: list[str]) -> subprocess.CompletedProcess:
        """Run a full tmux argv — already built by `layout.py`'s own functions, never
        hand-retyped — under this class's own throwaway plane. Mirrors
        `commands_frame.cmd_launch`'s own behaviour exactly: it passes `env=env` to
        EVERY tmux call it makes on a launch, not only the first (`new-session`
        alone is not enough — a split's own local tmux-CLIENT invocation does not
        need `$CHARTER_ROOT` itself, since a spawned pane's environment comes from
        the SESSION's tracked environment set at `new-session` time, not from
        whatever invoked the later `split-window` — but this matches production
        rather than relying on that distinction holding forever). `cwd` is pinned
        to the checkout root for the same reason `PanelIntegration._spawn_panel`
        pins it: `python3 -m charter` needs `charter` importable off the CURRENT
        DIRECTORY when the package is not installed, and nothing here should depend
        on wherever `unittest discover` happens to have been invoked from.
        """
        return subprocess.run(cmd, capture_output=True, text=True, timeout=10,
                              env=self.env, cwd=str(_REPO_ROOT))

    def _pane_pid(self, pane: str) -> str:
        return _tmux("display-message", "-p", "-t", pane, "#{pane_pid}").stdout.strip()

    def _capture(self, pane: str) -> str:
        return _tmux("capture-pane", "-p", "-t", pane).stdout

    def _alive(self, pane: str) -> str:
        return _tmux("display-message", "-p", "-t", pane, "#{pane_dead}").stdout.strip()

    def _wait_for(self, pane: str, needle: str, timeout: float = _DEADLINE) -> str:
        """Poll `capture-pane` for *pane* until *needle* appears or *timeout*
        elapses, returning whatever was last captured either way.

        Fix round 1: a fixed `time.sleep(1.5)` used to stand where this is called
        from — a guess about how long a cold `charter` import plus a full
        `gather.scan` (subprocess git calls included) takes on THIS machine,
        wrong in both directions: measured content landing in ~0.08s on a normal
        run (so the sleep was mostly wasted time, four such waits per test in the
        heaviest class in this module), and a hard, unrecoverable failure on any
        runner slow enough to still be starting up at 1.5s — exactly the
        "polling makes a slow machine slow rather than wrong" property
        `_await_file` above already established for this file's other classes,
        applied here to a first paint rather than a repaint. Content, not mere
        pane existence: a pane can be alive and blank (or alive and still on its
        LAST paint) well before it has the real content a caller wants to assert
        on, so this waits for the actual needle, not a fixed delay standing in
        for "probably done by now".
        """
        deadline = time.monotonic() + timeout
        content = self._capture(pane)
        while time.monotonic() < deadline and needle not in content:
            time.sleep(0.1)
            content = self._capture(pane)
        return content

    def _spawn_frame(self, fid: str) -> tuple[str, dict[str, str]]:
        """Launch a real four-slot frame: `layout.session_argv` for the harness pane,
        then `layout.panel_argvs` for all four splits off its id — the same two
        calls `cmd_launch` makes. Returns the harness pane id and a `slot -> pane
        id` map; every pane's own pid (`#{pane_pid}`, captured immediately — never
        the session or pane id, which name tmux objects, not the OS process
        underneath) is registered for `_kill_pid` cleanup as it is created, exactly
        the way `PanelIntegration`/`MenuIntegration` above already have to (see
        `_kill_pid`'s own docstring for why `kill-server` alone leaves orphans).
        """
        conf_dir = Path(tempfile.mkdtemp(prefix="charter-integ-4edge-"))
        self.addCleanup(shutil.rmtree, conf_dir, True)
        conf_path = conf_dir / "tmux.conf"
        # The same placeholder `cmd_launch` writes ahead of its own `new-session`
        # call — `commands_frame._PLACEHOLDER_CONF` arms `remain-on-exit` from the
        # very first moment, so a panel that crashes at startup leaves a pane this
        # test can still inspect rather than one that simply vanishes.
        conf_path.write_text(commands_frame._PLACEHOLDER_CONF)

        session_cmd = layout.session_argv(session=fid, conf=str(conf_path), socket=SOCKET,
                                          cols=120, rows=40, harness_argv=["sleep", "600"])
        r = self._run_env(session_cmd)
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        self.assertTrue(harness_pane, "tmux did not report the harness pane's id")
        self.addCleanup(_kill_pid, self._pane_pid(harness_pane))

        slots = ["top", "bottom", "left", "right"]
        panel_cmds = layout.panel_argvs(slots=slots, session=fid, socket=SOCKET,
                                        harness_pane=harness_pane)
        panes: dict[str, str] = {}
        for slot, cmd in zip(slots, panel_cmds):
            p = self._run_env(cmd)
            self.assertEqual(p.returncode, 0, f"splitting {slot!r}: {p.stderr}")
            pane_id = p.stdout.strip()
            self.assertTrue(pane_id, f"tmux did not report {slot!r}'s pane id")
            panes[slot] = pane_id
            self.addCleanup(_kill_pid, self._pane_pid(pane_id))

        # The same refocus `cmd_launch` performs once every panel is drawn (its own
        # comment: `split-window` makes the newly created pane active by default,
        # so after four splits the LAST panel drawn — never the harness — has
        # focus unless something puts it back). A literal `select-pane`, not one of
        # the argv-building functions above: it targets a tmux-ASSIGNED pane id,
        # carrying nothing an operator or a config file ever supplied, so there is
        # no injection surface here for a helper to guard against.
        self._run_env(["tmux", "-L", SOCKET, "select-pane", "-t", harness_pane])
        return harness_pane, panes

    def test_all_four_panels_come_up_alive_with_real_content_and_the_harness_keeps_focus(self):
        """Launch composition, proven end to end: a frame with `top`/`left`/`right`/
        `bottom` all configured comes up with all four panes alive, each showing
        real content a broken renderer (or an empty, never-gathered cache) could
        not have produced, and the harness pane — not the last panel
        `split-window` happened to draw — holds keyboard focus once the launch
        finishes.

        **Fix round 1: the repo is made DIRTY before the frame ever launches, not
        left pristine.** `_init_repo`'s own repo has nothing uncommitted, so
        without this, `left`'s row shows no marker regardless of whether
        `gather.scan`'s `_repo_states` half (a real `git status --porcelain
        --branch` subprocess) ran at all — a mutation that replaced that whole
        sweep with `{}` left every assertion in this test green, because
        everything asserted before this fix (repo name, branch) comes from
        `_repo_trees`/`_branch` alone, neither of which touches `_repo_states`.
        The `*` dirty marker asserted below is the one thing in this test that
        can only come from that sweep actually running and actually landing in
        the cache `left` reads back.
        """
        repo_name = f"cnry{os.getpid() % 10000}"
        branch = f"br{os.getpid() % 10000}a"
        repo = config.WORKSPACES_DIR / "demo" / repo_name
        _init_repo(repo, branch)
        (repo / "scratch.txt").write_text("uncommitted\n")
        todos.add("demo", "a todo the bottom panel should count")
        persona_name = self.make_persona(f"p{os.getpid() % 100}")

        fid = state.frame_id("four-edge-alive", os.getpid())
        harness_pane, panes = self._spawn_frame(fid)
        self.assertEqual(set(panes), {"top", "bottom", "left", "right"})

        # Poll for real content (`_wait_for`'s own docstring — fix round 1:
        # replaces a fixed `time.sleep(1.5)` that guessed at how long four cold
        # `charter` imports plus four `gather.scan` git sweeps take). This also
        # does the flat sleep's OTHER job — giving a genuine startup crash time
        # to register — for free: a dead pane never satisfies its needle, so
        # `_wait_for` spends its own timeout before falling through to the alive
        # check below, same as a deliberate wait would have.
        top = self._wait_for(panes["top"], "demo")
        left = self._wait_for(panes["left"], repo_name)
        right = self._wait_for(panes["right"], persona_name)
        bottom = self._wait_for(panes["bottom"], "1 todo")

        for slot, pane in panes.items():
            self.assertEqual(self._alive(pane), "0",
                             f"the {slot!r} panel died at startup — a hole in the "
                             f"frame, not a degraded row (the launcher-days-of-a-"
                             f"green-suite gap `PanelIntegration`'s own docstring "
                             f"names, now for all four slots at once)")

        self.assertIn("demo", top, f"top never showed the real workspace name:\n{top!r}")

        self.assertIn(repo_name, left, f"left never showed the real repo:\n{left!r}")
        self.assertIn(branch, left, f"left never showed the real branch:\n{left!r}")
        self.assertIn("*", left,
                      f"left never showed the dirty marker for a real uncommitted "
                      f"file — either gather.scan's own git-status sweep never "
                      f"ran, or nothing carried its result into the cached "
                      f"row:\n{left!r}")

        self.assertIn(persona_name, right,
                      f"right never showed the real persona:\n{right!r}")

        self.assertIn("1 todo", bottom,
                      f"bottom never showed the real todo count:\n{bottom!r}")

        focus = _tmux("display-message", "-p", "-t", harness_pane,
                      "#{pane_active}").stdout.strip()
        self.assertEqual(focus, "1",
                         "the harness pane lost focus to the last panel drawn — an "
                         "operator's harness must be able to receive a keystroke "
                         "the instant the frame comes up")

    def test_a_state_bump_through_the_real_hook_repaints_left_and_bottom_with_new_facts(self):
        """Closes the gap `PanelIntegration`'s own hook test
        (`test_a_real_hook_call_repaints_a_live_panel_without_a_direct_state_bump`)
        leaves open for THIS plan: that test drives `hooks.posttooluse` against
        `bottom` alone, which never touches `gather.py` at all. This drives the
        SAME real hook — never a direct `state.bump` or `gather.refresh` call — and
        watches `left` repaint with a NEW branch name that only exists because
        `notify.plane_changed` ran `gather.refresh` BEFORE `state.bump` (Task 2's
        own contract, and its own docstring's reason: refresh-then-bump closes the
        window where a poller sees the new version and still reads the stale
        cache). A version bump into a stale or never-refreshed cache would leave
        `left` showing the OLD branch forever — this is the one test in the file
        that would catch that. `bottom` is watched in the same pass, from the SAME
        single hook call, to pin that one refresh/bump serves every slot that asks,
        not only the one `PanelIntegration` already covers.

        **The cache is warmed with one direct `gather.refresh` call before any
        assertion runs — this is load-bearing, not incidental.** Caught by
        mutation: with no cache file on disk yet, `gather.read`'s OWN fallback
        (`_left` calls it, this file's `left` panel does not) recomputes a FRESH
        scan on every call regardless of whether the cache was ever written — so
        with `notify.plane_changed`'s `gather.refresh` call deleted outright (a
        real mutation tried while writing this test), `left` still showed the new
        branch every time, because it was never reading a cache at all, only ever
        falling through to a live scan. That passed for the wrong reason and would
        have shipped a vacuous proof of Task 2's whole contract. Priming the cache
        first (real production `gather.refresh`, called directly — the same
        function `notify.plane_changed` is supposed to call) means the SECOND
        gather event below can only show the new branch by successfully
        OVERWRITING an already-valid cache file — the one behaviour that
        distinguishes "the hook refreshed the cache" from "the panel quietly
        recovered on its own."
        """
        repo_name = f"cnry{os.getpid() % 10000}"
        branch_a = f"br{os.getpid() % 10000}a"
        branch_b = f"br{os.getpid() % 10000}b"
        repo = config.WORKSPACES_DIR / "demo" / repo_name
        _init_repo(repo, branch_a)
        todos.add("demo", "the first todo")

        fid = state.frame_id("four-edge-repaint", os.getpid())
        _harness_pane, panes = self._spawn_frame(fid)

        # Prime the cache for real, mirroring an already-fired prior hook (see the
        # docstring above for why this is load-bearing) — `workspace="demo"`
        # explicit rather than relying on `$CHARTER_WORKSPACE`, since this call
        # runs IN this test process, not a subprocess that inherited the tmux
        # session's own environment the way every panel below does.
        gather.refresh(fid, workspace="demo")

        # Fix round 1: prove the CACHE FILE itself exists and holds *branch_a* —
        # not only that `left` shows the right text, which `gather.read`'s own
        # missing-cache fallback (a fresh, live scan) can produce with `gather.
        # save` neutered outright and no cache ever written at all. This is the
        # direct check that `gather.refresh` did the WRITE half of its job, not
        # only the read a panel happens to see regardless.
        cache = gather._cache_file(fid, create=False)
        self.assertTrue(cache is not None and cache.exists(),
                        "gather.refresh's own save() never produced a cache file "
                        "for this frame — priming succeeded at reading but not "
                        "at writing")
        self.assertIn(branch_a, cache.read_text(),
                      f"the primed cache file exists but does not hold the "
                      f"starting branch:\n{cache.read_text()!r}")

        left_before = self._wait_for(panes["left"], branch_a)
        bottom_before = self._wait_for(panes["bottom"], "1 todo")
        self.assertIn(branch_a, left_before,
                      f"left never showed the starting branch:\n{left_before!r}")
        self.assertIn("1 todo", bottom_before,
                      f"bottom never showed the starting todo count:\n{bottom_before!r}")

        # The mutation: a real branch switch (see the class docstring for why this,
        # not a dirty-bit change, is what a repaint proof here mutates) and a
        # second todo — both real plane-state changes, the kind `notify.
        # plane_changed` exists to notice.
        _git(repo, "checkout", "-q", "-b", branch_b)
        todos.add("demo", "the second todo")

        # Reset the debounce (see `PanelIntegration`'s own identical line): a
        # window left over from another test in this same process must not mask a
        # broken hook wiring by silently no-op'ing this call.
        notify._last["at"] = 0.0
        # `CHARTER_WORKSPACE`, not only `CHARTER_SESSION_ID`: this hook call runs
        # IN-PROCESS (`run_hook`, not a subprocess), so `gather.refresh` inside it
        # resolves the active workspace through THIS process's real `os.environ` —
        # unlike every panel subprocess in this class, which inherits `demo` from
        # the tmux SESSION environment `_spawn_frame` set at `new-session` time
        # (via `self.env`). Without this, `workspace.resolve()` falls through to
        # this throwaway plane's own `DEFAULT_WORKSPACE` instead (this process's
        # cwd is the checkout root, not inside `demo`'s own tree, so the cwd rung
        # cannot rescue it either) — `gather.refresh` would then cache a scan of
        # the WRONG, repo-less workspace, and `left` would repaint to "no repos"
        # rather than the new branch, for a reason that has nothing to do with
        # whether the refresh/bump wiring itself works.
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": fid,
                                          "CHARTER_WORKSPACE": "demo"}):
            out = run_hook(hooks.posttooluse, {
                "tool_name": "Read",
                "tool_input": {"file_path": "/nonexistent"},
                "session_id": "four-edge-hook-session",
            })
        self.assertIsNone(out, "posttooluse should emit nothing for a plain Read")

        # `self._wait_for` — the SAME poll `_wait_for` already uses for the FIRST
        # paint (fix round 1) — waits for the specific new fact each panel is
        # expected to show, rather than a fixed sleep or an undifferentiated
        # "content changed" check.
        left_after = self._wait_for(panes["left"], branch_b)
        bottom_after = self._wait_for(panes["bottom"], "2 todos")

        self.assertNotEqual(left_after, left_before,
                            f"left never repainted after a real hooks.posttooluse() "
                            f"call; still showing:\n{left_before!r}")
        self.assertIn(branch_b, left_after,
                      f"left repainted but not with the NEW branch:\n{left_after!r}")
        self.assertNotIn(branch_a, left_after,
                         f"left kept showing the OLD branch after the switch — a "
                         f"stale cache surviving its own refresh:\n{left_after!r}")

        # Fix round 1: the cache FILE on disk, not only the pane's own capture,
        # now holds the new branch — proving the hook's `gather.refresh` call did
        # the WRITE `notify.plane_changed` is supposed to perform, the same
        # direct check made against `branch_a` above, repeated here against the
        # value only a SECOND, successful refresh could have produced.
        self.assertIn(branch_b, cache.read_text(),
                      f"left repainted (or the panel's own live fallback masked "
                      f"a broken refresh) but the cache file itself was never "
                      f"updated with the new branch:\n{cache.read_text()!r}")

        self.assertNotEqual(bottom_after, bottom_before,
                            f"bottom never repainted after the same real hook call; "
                            f"still showing:\n{bottom_before!r}")
        self.assertIn("2 todos", bottom_after,
                      f"bottom repainted but not with the NEW todo count:\n{bottom_after!r}")

        for slot, pane in panes.items():
            self.assertEqual(self._alive(pane), "0",
                             f"the {slot!r} panel died sometime after repainting")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class EarlyDeathIntegration(unittest.TestCase):
    """#384: what a command that dies before the frame is drawn actually leaves behind.

    `commands_frame.early_death_message`'s whole design rests on facts about TMUX, not
    about charter — which of `sh -c` and `execvp` runs a given `charter frame -- <cmd>`,
    and what each leaves in the pane when the command is not there. Those were measured
    by hand against tmux 3.7c while deciding not to add a `shutil.which()` pre-check;
    this class is that hand-measurement turned into something that reruns.

    **The acceptance contract is the first two tests, and they are the reason the
    pre-check was refused.** tmux's own rule: ONE argument is handed to a shell, TWO OR
    MORE are `execvp`'d directly. So `charter frame -- 'exit 7'` is a shell command line
    — `shutil.which` over that text would be asking the wrong question of the wrong
    string — while `charter frame -- exit 7` is a direct exec of a program called `exit`
    that does not exist. Neither test names a version; both read what this tmux does.

    Assertions are deliberately about the exit code and about what reaches the MESSAGE,
    never about a shell's exact wording (`sh`, `bash`, `dash` and `zsh` all phrase
    "command not found" differently, and tmux's `default-shell` is whatever the machine
    says) and never about a tmux version string.
    """

    #: A word no `$PATH` will resolve and no file is named — the same shape
    #: `tests/test_frame_launcher.py` uses, spelled long enough that a machine that
    #: somehow HAS it would be telling us something worth knowing.
    MISSING = "charter-definitely-not-a-real-binary-xyz"

    def setUp(self) -> None:
        self.addCleanup(self._teardown_socket)
        self._conf_dir = Path(tempfile.mkdtemp(prefix="charter-integ-death-"))
        self.addCleanup(shutil.rmtree, self._conf_dir, True)
        self._n = 0

    def _teardown_socket(self) -> None:
        """Kill this class's own server, THEN unlink its socket file — see
        `PanelIntegration._teardown_socket` for the full reasoning, identical here.
        Load-bearing rather than tidy for THIS class specifically: its sessions arm
        `remain-on-exit`, so a dead pane's session does not end on its own and tmux's
        `exit-empty` default never gets the chance to retire the server."""
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _die(self, *harness_argv: str) -> tuple[str, int | None]:
        """Run *harness_argv* the way `cmd_launch` runs it, and wait for it to die.

        Built from `layout.session_argv` and `commands_frame._PLACEHOLDER_CONF` rather
        than a hand-retyped `new-session`, so this measures the exact bytes the launcher
        sends — including `--`, which is what stops tmux reading a leading `-` in an
        operator's own command as one of its own flags.

        `remain-on-exit` is armed BOTH ways for the same reason `cmd_launch` arms it both
        ways: the placeholder's `-f` is honoured only by the call that STARTS the server,
        so the second and later sessions in one test would otherwise let their pane
        vanish the instant it died — and a pane that is gone answers nothing, which would
        make every assertion below vacuous rather than red.
        """
        self._n += 1
        name = f"d{self._n}"
        conf = self._conf_dir / f"{name}.conf"
        conf.write_text(commands_frame._PLACEHOLDER_CONF)
        _tmux("set", "-g", "remain-on-exit", "on")  # no-op (and an error) before the
                                                    # first session; `-f` covers that one
        r = _run(layout.session_argv(session=name, conf=str(conf), socket=SOCKET,
                                     cols=80, rows=24, harness_argv=list(harness_argv)))
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self.assertTrue(pane, "tmux reported no pane id for the new session")
        return pane, _await_dead(pane)

    def test_a_lone_argument_reaches_a_shell_not_execvp(self):
        """Half of the contract `charter frame --` is not allowed to narrow. `exit 7` is
        not a program on any machine — it is shell syntax, and it comes back as 7 only if
        tmux ran it through a shell. A `shutil.which(argv[0])` pre-check would refuse
        this text outright (it is not even one word), which is why #384's fix reports
        after the fact instead of predicting beforehand."""
        pane, code = self._die("exit 7")
        self.assertEqual(code, 7,
                         "a single argument must reach a shell — `charter frame -- "
                         "'ulimit -n; exit 3'` and every other one-liner depend on it")

    def test_two_or_more_arguments_are_exec_d_directly_with_no_shell(self):
        """The other half, and what makes charter's own `$PATH` note SOUND rather than a
        guess: split into two arguments, the same text is `execvp`'d, so `exit` is looked
        up as a program, is not found, and cannot possibly have run. A word that resolves
        to nothing in THIS form provably never executed — which is the one condition
        under which `_could_not_have_run` is allowed to speak."""
        pane, code = self._die("exit", "7")
        self.assertIsNotNone(code, "the pane should have died at once")
        self.assertNotEqual(code, 7,
                            "`exit 7` must NOT be interpreted by a shell once it is two "
                            "arguments — charter's own diagnosis depends on the split")
        self.assertNotEqual(code, 0)

    def test_a_missing_commands_own_words_survive_into_the_report(self):
        """The fix's load-bearing measurement: the shell's `command not found` is still
        readable in the dead pane, so charter never has to invent a worse sentence.

        This is also what fails if `-S -` is ever dropped from `_pane_last_words`.
        Measured against tmux 3.7c: `remain-on-exit` appends its own `Pane is dead (…)`
        line, which scrolls the visible screen by one — so a plain `capture-pane -p` of a
        command that printed exactly ONE line comes back with blanks where that line was.
        The whole message lives in the line the default form loses.

        No shell's exact phrasing is asserted (`sh`, `dash`, `bash` and `zsh` all differ,
        and tmux uses the machine's own `default-shell`) — only that the missing word
        itself comes back, which every one of them names."""
        pane, code = self._die(self.MISSING)
        self.assertIsNotNone(code, "the pane should have died at once")
        self.assertNotEqual(code, 0)
        words = commands_frame._pane_last_words(SOCKET, pane)
        self.assertTrue(any(self.MISSING in ln for ln in words),
                        f"the shell said what was wrong and charter could not read it "
                        f"back: {words!r}")
        self.assertFalse(any("Pane is dead" in ln for ln in words),
                         f"tmux's own trailer must not be quoted back: {words!r}")
        msg = commands_frame.early_death_message([self.MISSING], code, words)
        self.assertIn(self.MISSING, msg)

    def test_the_operator_is_named_the_command_whichever_residue_tmux_left(self):
        """End to end for the form that leaves charter nothing to quote. Measured
        against tmux 3.7c: a failed `execvp` exits tmux's own child with 1 and an
        entirely EMPTY pane — no shell ran, so nothing said `command not found`, and a
        bare 1 is indistinguishable from a program that ran and failed.

        Asserted on the MESSAGE rather than on the pane being empty, deliberately. Which
        residue a given tmux leaves is that tmux's business, and charter is correct
        either way — it quotes the pane when there is one and answers for itself when
        there is not. What must never differ is that the operator is told which command
        died, which is the whole of #384. The branch-specific wording is pinned where it
        can be pinned exactly, in `tests/test_frame_launcher.py::EarlyDeathIsLegible`."""
        pane, code = self._die(self.MISSING, "--flag")
        self.assertIsNotNone(code, "the pane should have died at once")
        self.assertNotEqual(code, 0)
        words = commands_frame._pane_last_words(SOCKET, pane)
        msg = commands_frame.early_death_message([self.MISSING, "--flag"], code, words)
        self.assertIn(self.MISSING, msg)
        self.assertIn(str(code), msg)


#: The socket FILE tmux computes for `-L SOCKET` — the same path `_teardown_socket`
#: already has to know. Needed as a path (not a name) by `WindowInsideAnOperatorsTmux`,
#: because the whole point of that class is exercising the `-S <socket path>` half of
#: `tmuxctl.server_argv`, which is how charter reaches a server it did not start.
SOCKET_PATH = str(Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET)


class WindowInsideAnOperatorsTmux(_TmuxServerFixture, PersonaIso):
    """The frame built as a WINDOW in a tmux charter did not start (#381).

    Stands a real server up and treats it as the operator's: charter reaches it by
    socket PATH, opens one window in one of its sessions, and must leave everything else
    exactly as it found it. The properties here are the ones a mock cannot check —
    whether tmux keeps a dead pane at all when `remain-on-exit` is set PANE-scoped,
    what it answers for a pane that no longer exists, whether `respawn-pane -e` reaches
    the process, and whether `kill-window` takes the operator's session with it.

    No attached client anywhere in this class, deliberately: every command is issued
    against a detached session, so nothing here needs the terminal capability
    `_NeedsAttachedClient` probes for and none of it is skipped on a headless CI step.

    `PersonaIso`, and it is load-bearing rather than tidy: the last test below calls the
    real `cmd_launch`, which reaps and writes frame state under `config.STATE_DIR`. The
    tmux server this class stands up is a throwaway; without `PersonaIso` the PLANE it
    writes to would not be. See `_TmuxServerFixture`'s docstring for what that cost.
    """

    def _operator_server(self, harness_dies_by="exit 0"):
        """A session standing in for one the operator already had open.

        Returns its name, its `$N` session id, its own pane id, and the gate the frame's
        harness will die by. The session's own pane runs a program that never exits on
        its own, so anything that quietly takes the operator's session down shows up as
        a missing session rather than as a race.
        """
        name, pane, gate = self._new_pane("exit 0")
        sid = _tmux("display-message", "-p", "-t", pane, "#{session_id}").stdout.strip()
        self.assertTrue(sid.startswith("$"), sid)
        return name, sid, pane, gate

    def _open_frame_window(self, sid, fid="charter-demo-1"):
        """`layout.window_argv`'s own bytes, run for real. Returns (window id, pane id)."""
        r = _run(layout.window_argv(socket=SOCKET_PATH, session=sid, window=fid,
                                    cwd=self._gate_dir))
        self.assertEqual(r.returncode, 0, r.stderr)
        window_id, _, pane_id = r.stdout.strip().partition(" ")
        self.assertTrue(window_id.startswith("@"), r.stdout)
        self.assertTrue(pane_id.startswith("%"), r.stdout)
        return window_id, pane_id

    def _wait_until(self, predicate, timeout=_DEADLINE):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return True
            time.sleep(0.05)
        return False

    def _require_pane_options(self, pane_id):
        """Skip unless THIS tmux accepts a pane-scoped `remain-on-exit`.

        `set-option -p` is what keeps a dead harness pane askable on a server charter is
        a guest on, and it is the only scope that does not reach past charter's own
        window (see `commands_frame._remain_on_exit_argv`). A tmux without pane options
        cannot do the thing these tests measure, so they skip naming that rather than
        failing with tmux's own argument-parsing text."""
        r = _run(commands_frame._remain_on_exit_argv(socket=SOCKET_PATH,
                                                     harness_pane=pane_id))
        if r.returncode != 0:
            self.skipTest("this tmux does not accept a pane-scoped `remain-on-exit` "
                          f"({r.stderr.strip()}), so it cannot hold a dead harness pane "
                          "for its exit status to be read out of")

    def test_the_harnesss_real_exit_code_survives_in_someone_elses_server(self):
        """End to end for the one property the whole module exists for, on the path
        where there is no `attach` to read a code out of: charter opens the window,
        arms the pane, respawns the harness into it, and reads the status back through
        `_pane_state` — the same function the launcher's wait loop calls."""
        _, sid, _, _ = self._operator_server()
        window_id, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        gate = os.path.join(self._gate_dir, "harness-gate")
        r = _run(layout.respawn_argv(socket=SOCKET_PATH, harness_pane=pane_id, env={},
                                     cwd=self._gate_dir,
                                     harness_argv=_gate_argv(gate, "exit 33")))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(commands_frame._pane_state(SOCKET_PATH, pane_id),
                         (commands_frame._ALIVE, None))
        self._release(gate)
        self.assertTrue(self._wait_until(
            lambda: commands_frame._pane_state(SOCKET_PATH, pane_id)[0]
            == commands_frame._DEAD), "the pane never came back dead")
        self.assertEqual(commands_frame._pane_state(SOCKET_PATH, pane_id),
                         (commands_frame._DEAD, 33))
        del window_id

    def test_the_placeholder_does_not_exit_on_its_own(self):
        """`layout.PLACEHOLDER`'s only required property, and the one an argv assertion
        cannot make: if it could exit, the window would be gone before `remain-on-exit`
        was ever set on its pane and the ordering the whole path rests on would buy
        nothing."""
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        time.sleep(0.4)
        self.assertEqual(commands_frame._pane_state(SOCKET_PATH, pane_id)[0],
                         commands_frame._ALIVE,
                         "the placeholder exited on its own")

    def test_a_pane_that_is_gone_answers_empty_rather_than_failing(self):
        """The measured fact `_pane_state` — and therefore the launcher's wait loop —
        rests on: `display-message -p -t <a pane that no longer exists>` does NOT fail.
        It returns 0 and prints an empty line. A loop that only stopped on
        `#{pane_dead}` being `1` would poll a window nobody can bring back, forever."""
        _, sid, _, _ = self._operator_server()
        window_id, pane_id = self._open_frame_window(sid)
        raw = _run(tmuxctl.server_argv(SOCKET_PATH, "display-message", "-p", "-t",
                                       pane_id, commands_frame._DEAD_FORMAT))
        self.assertEqual(raw.returncode, 0)
        self.assertEqual(raw.stdout.strip(), "0:")
        _run(tmuxctl.server_argv(SOCKET_PATH, "kill-window", "-t", window_id))
        gone = _run(tmuxctl.server_argv(SOCKET_PATH, "display-message", "-p", "-t",
                                        pane_id, commands_frame._DEAD_FORMAT))
        self.assertEqual(gone.returncode, 0,
                         "tmux is expected to answer, not to refuse")
        self.assertEqual(gone.stdout.strip(), ":",
                         "both variables expand to nothing, and the format's own "
                         "literal `:` is all that is left — NOT an empty line, which "
                         "is what a guard written from memory assumed")
        self.assertEqual(commands_frame._pane_state(SOCKET_PATH, pane_id),
                         (commands_frame._GONE, None))

    def test_closing_the_frames_window_leaves_the_operators_session_alone(self):
        """`kill-window`, never `kill-session` — the difference between charter tidying
        up after itself and charter ending every window the operator had open."""
        name, sid, op_pane, _ = self._operator_server()
        window_id, _ = self._open_frame_window(sid)
        before = _tmux("list-windows", "-a", "-F", "#{window_name}").stdout.split()
        self.assertIn("charter-demo-1", before)
        _run(tmuxctl.server_argv(SOCKET_PATH, "kill-window", "-t", window_id))
        after = _tmux("list-windows", "-a", "-F", "#{window_name}").stdout.split()
        self.assertNotIn("charter-demo-1", after)
        self.assertIn(name, _tmux("list-sessions", "-F", "#{session_name}").stdout.split(),
                      "the operator's own session went with charter's window")
        self.assertEqual(_tmux("display-message", "-p", "-t", op_pane,
                               "#{pane_dead}").stdout.strip(), "0",
                         "the operator's own pane died with it")

    def test_charters_environment_reaches_the_harness_and_the_launchers_pane_does_not(self):
        """`respawn-pane -e` is the only channel charter has here, and the launching
        pane's own `$TMUX_PANE` must not travel down it: tmux sets that variable itself
        for the pane it creates, and overwriting it would tell the harness — and
        `session.terminal()`, which reads it — that it is somewhere it is not."""
        _, sid, op_pane, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        out = os.path.join(self._gate_dir, "harness-env")
        env = commands_frame._frame_env("charter-demo-1", None)
        env["CHARTER_HARNESS"] = "claude-code"
        env = commands_frame._guest_harness_env(env)
        r = _run(layout.respawn_argv(
            socket=SOCKET_PATH, harness_pane=pane_id, env=env, cwd=self._gate_dir,
            harness_argv=["/bin/sh", "-c",
                          f'printf "%s\\n%s\\n%s\\n" "$CHARTER_SESSION_ID" '
                          f'"$CHARTER_HARNESS" "$TMUX_PANE" > "{out}"']))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(_await_file(out), "the harness never ran")
        self.assertTrue(self._wait_until(lambda: Path(out).read_text().count("\n") >= 3))
        sid_seen, harness_seen, pane_seen = Path(out).read_text().splitlines()[:3]
        self.assertEqual(sid_seen, "charter-demo-1")
        self.assertEqual(harness_seen, "claude-code")
        self.assertEqual(pane_seen, pane_id,
                         "the harness must see its OWN pane, not the launcher's")
        self.assertNotEqual(pane_seen, op_pane)

    def test_a_pane_here_gets_the_invoking_clients_path_not_the_servers(self):
        """The measurement `_guest_harness_env` rests on, pinned against a real server so
        charter is TOLD if tmux ever changes it.

        `cmd_launch` resolves the harness binary with `shutil.which` against charter's own
        `$PATH`; the pane it then respawns into lives on a server the operator started.
        Measured against tmux 3.7c: the pane's `$PATH` is the one the CLIENT that issued
        `respawn-pane` had — charter's — and an explicit `-e PATH=…` does not even
        survive, tmux overwrites it after applying the `-e` set. So on this tmux carrying
        `PATH` is redundant; if that ever stops being true, `_guest_harness_env` becomes
        the only thing standing between an operator and a harness that cannot be executed,
        and this test is where the change shows up.

        `python3` reads the value, not a shell, so no shell's own `$PATH` normalisation
        can be what is being measured.
        """
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        out = os.path.join(self._gate_dir, "harness-path")
        mine = os.environ.get("PATH", "")
        r = _run(layout.respawn_argv(
            socket=SOCKET_PATH, harness_pane=pane_id,
            env={"PATH": "/charter/said/this", "CHARTER_SESSION_ID": "charter-demo-1"},
            cwd=self._gate_dir,
            harness_argv=[sys.executable, "-c",
                          "import os,sys;open(sys.argv[1],'w')"
                          ".write(os.environ.get('PATH',''))", out]))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(_await_file(out), "the harness never ran")
        seen = Path(out).read_text()
        self.assertNotEqual(seen, "", "the pane was handed no PATH at all")
        self.assertEqual(seen, mine,
                         "a pane on somebody else's server no longer gets the invoking "
                         "client's PATH — `_guest_harness_env`'s `-e PATH` is now the "
                         "only thing carrying it, and its 'redundant here' note is stale")
        self.assertNotEqual(seen, "/charter/said/this",
                            "`-e PATH=` now survives; the same note is stale the other "
                            "way, and charter could state a PATH deliberately")

    def test_a_panels_respawn_hook_is_armed_against_this_server_and_fires(self):
        """#408, end to end on the path it was broken on. `_arm_panel_respawn` refused
        here outright, because `_panel_died_hook_argv` hand-built `["tmux", "-L", …]` and
        would have aimed a `run-shell` at charter's private server — or started an empty
        one named after a socket path.

        Two things a mock cannot check: that `set-hook -p` is accepted on a pane charter
        created inside somebody else's server at all, and that the action reaches a real
        shell with the frame id ON IT — there is no `set-environment` here to read
        `$CHARTER_SESSION_ID` back out of, which is the reason `--frame` exists."""
        self._require_pane_died_fires()
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        gate = os.path.join(self._gate_dir, "panel-gate")
        panel = _run(tmuxctl.server_argv(
            SOCKET_PATH, "split-window", "-t", pane_id, "-v", "-l", "1",
            "-P", "-F", "#{pane_id}", "--", *_gate_argv(gate, "exit 4"))).stdout.strip()
        self.assertTrue(panel.startswith("%"), panel)
        _run(tmuxctl.server_argv(SOCKET_PATH, "set-option", "-p", "-t", panel,
                                 "remain-on-exit", "on"))
        seen = self._hook_reaches_a_shim(
            socket=SOCKET_PATH, pane=panel, gate=gate,
            interpreter_dir=os.path.join(self._gate_dir, "op interp"))
        self.assertIsNotNone(
            seen, "a panel that died inside the operator's own tmux reached nothing — "
                  f"hooks: {_run(tmuxctl.server_argv(SOCKET_PATH, 'show-hooks', '-p', '-t', panel)).stdout!r}")
        self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                "--pane", panel, "--frame", "demo-1"])

    def test_a_frame_here_is_live_by_its_window_never_by_a_session(self):
        """`cmd_respawn` asked `_live_sessions(SOCKET)` unconditionally, which on this
        server is a question about somebody else's sessions: it answers "gone" for a
        frame that is on screen, so a panel could never have been brought back even once
        the hook reached charter. `_frame_is_live` asks the right question per server.

        Both directions against the same real server, so neither can pass by the answer
        always being the same one."""
        _, sid, _, _ = self._operator_server()
        self._open_frame_window(sid, fid="charter-demo-1")
        self.assertTrue(commands_frame._frame_is_live(SOCKET_PATH, "charter-demo-1"))
        self.assertFalse(commands_frame._frame_is_live(SOCKET_PATH, "a-frame-that-ended"))
        # And the operator's OWN session name is not a frame: a `list-sessions`-shaped
        # answer here would report it live and respawn a panel into a window charter
        # never opened.
        sessions = _tmux("list-sessions", "-F", "#{session_name}").stdout.split()
        self.assertTrue(sessions, "the fixture server reported no sessions at all")
        for name in sessions:
            self.assertFalse(commands_frame._frame_is_live(SOCKET_PATH, name), name)

    def test_the_harness_starts_where_charter_was_typed(self):
        """A pane in a server charter did not start otherwise inherits the SESSION's
        working directory — wherever the operator was when they first ran `tmux`."""
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        out = os.path.join(self._gate_dir, "harness-cwd")
        r = _run(layout.respawn_argv(
            socket=SOCKET_PATH, harness_pane=pane_id, env={}, cwd=self._gate_dir,
            harness_argv=["/bin/sh", "-c", f'pwd > "{out}"']))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertTrue(_await_file(out), "the harness never ran")
        self.assertEqual(Path(Path(out).read_text().strip()).resolve(),
                         Path(self._gate_dir).resolve())

    def test_nothing_of_the_operators_tmux_is_written_by_a_whole_launch(self):
        """The boundary, checked against tmux's own reported state rather than against
        charter's argv: every server option, every option of the operator's own session,
        and the entire key table are read before and after a REAL `cmd_launch` runs
        inside this server, and must come back byte for byte.

        `cmd_launch` itself, not the argv builders — this is the one test that can catch
        a command the launcher issues that nobody thought to look for.

        **Their TMUX, which is not the same claim as "nothing at all is written"** — the
        name says `tmux` for that reason. A launch writes charter's own frame state, and
        the first thing it writes is a DELETION: `state.reap` rmtrees every frame
        directory the named server does not report, and a directory carrying no `server`
        marker (the migration case `state.reap` documents) is not reported by any server
        at all. The decoy below is exactly that shape — no marker, and named after a pid
        that has genuinely exited, so neither of `reap`'s two keep-rules covers it — and
        asserting it is gone afterward pins the blast radius of one launch — which is also why this class cannot run
        without `PersonaIso`: the same rmtree against an unisolated `config.STATE_DIR`
        lands on the developer's live frames. The decoy is checked to be inside this
        test's throwaway plane BEFORE the launch, so the assertion that it was deleted
        can never be evidence about the real one.
        """
        name, sid, op_pane, _ = self._operator_server()
        gate = os.path.join(self._gate_dir, "e2e-gate")

        # Named after a pid that has genuinely exited: since #383 `reap` keeps any
        # directory whose launcher is still alive, and the `-1` this once carried reads
        # as `launchd`/`init` — the decoy would have survived for that reason and this
        # assertion would have been unfailable.
        decoy = state.frame_dir(
            f"a-frame-from-a-charter-that-had-no-server-marker-{_a_dead_pid()}",
            create=True)
        self.assertIsNotNone(decoy)
        (decoy / "version").write_text("1\n")
        self.assertTrue(decoy.is_relative_to(self.tmp),
                        f"the frame state this test is about to have charter delete is "
                        f"at {decoy} — outside this test's own throwaway plane "
                        f"({self.tmp}), so it belongs to somebody real")

        def _snapshot():
            return tuple(_tmux(*args).stdout for args in (
                ("show-options", "-g"),
                ("show-options", "-t", name),
                ("show-options", "-g", "-w"),
                ("list-keys",)))

        before = _snapshot()
        server_pid = _tmux("display-message", "-p", "#{pid}").stdout.strip()
        args = SimpleNamespace(harness="frame", rest=["--", *_gate_argv(gate, "exit 21")],
                               no_frame=False)
        rc: list[int] = []

        def _run_launch():
            env = dict(os.environ, TMUX=f"{SOCKET_PATH},{server_pid},{sid[1:]}",
                       TMUX_PANE=op_pane)
            with mock.patch.dict(os.environ, env, clear=True), \
                 mock.patch("sys.stdout.isatty", return_value=True), \
                 mock.patch("charter.workspace.resolve", return_value="demo"), \
                 mock.patch.dict(config.FRAME, {"slots": []}):
                rc.append(commands_frame.cmd_launch(args))

        worker = threading.Thread(target=_run_launch, daemon=True)
        worker.start()
        self.assertTrue(self._wait_until(
            lambda: "demo-" in _tmux("list-windows", "-a", "-F",
                                     "#{window_name}").stdout),
            "the frame's own window never appeared in the operator's server")
        during = _snapshot()
        self._release(gate)
        worker.join(timeout=25)
        self.assertFalse(worker.is_alive(), "cmd_launch never returned")
        self.assertEqual(rc, [21], "the harness's own exit code did not come back")
        self.assertEqual(before, during,
                         "charter wrote something on a server it is only a guest on")
        self.assertEqual(before, _snapshot())
        self.assertIn(name, _tmux("list-sessions", "-F", "#{session_name}").stdout.split())
        self.assertNotIn("demo-", _tmux("list-windows", "-a", "-F",
                                        "#{window_name}").stdout,
                         "the frame's window was left behind")
        self.assertFalse(decoy.exists(),
                         "a launch is expected to reap a frame directory with no "
                         "`server` marker — if it has stopped doing that, `state.reap`'s "
                         "migration case changed and the isolation this class rests on "
                         "is no longer being exercised by anything")


class ASecondFrameOnTheSharedServer(_TmuxServerFixture, PersonaIso):
    """#411 — what a `new-session` inherits when the server is ALREADY running.

    Every frame on charter's own server is a session on ONE shared server
    (`commands_frame`'s module docstring), so exactly one launch per machine is the one
    whose `new-session` actually starts it. That distinction was invisible in the code
    until it cost a frame: `layout.respawn_argv`'s docstring said charter's own variables
    reach the harness "because `new-session` starts the server and the server inherits
    the launcher's environment", which is true of the first launch and of no other.

    No mock can observe this. It is tmux's own rule about where a new pane's environment
    comes from, and the whole `-e` on `layout.session_argv` exists because of it — so it
    is measured here, against a real server with a real second session on it, and will
    fail loudly if tmux ever changes it (at which point the flag stops being necessary,
    which is worth being told rather than left carrying forever).
    """

    #: Not `CHARTER_SESSION_ID`: this class is measuring TMUX's inheritance rule, and
    #: charter's own variable may be present in the test runner's environment already
    #: (the suite runs inside a frame on this project), which would leave the "inherited"
    #: assertion able to pass on a value neither session set.
    _VAR = "CHARTER_INTEG_PROBE"

    def _session_reading_the_var(self, name: str, *, client_value: str,
                                 carry: str | None = None) -> str:
        """A session on `SOCKET` whose pane writes `$_VAR` out; returns what it wrote.

        *client_value* is what the tmux CLIENT process is started with — the thing a
        launcher controls without `-e`. *carry* is `-e`, or `None` for a call that
        passes none. The pane sleeps afterwards so the SERVER stays up for the next
        session: a server that empties shuts itself down, and a second `new-session`
        against a dead server would start a fresh one and quietly measure nothing.
        """
        out = os.path.join(self._gate_dir, f"env-{name}")
        args = ["new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                "-P", "-F", "#{pane_pid}"]
        if carry is not None:
            args += ["-e", f"{self._VAR}={carry}"]
        args += ["--", "sh", "-c",
                 f'printf "%s" "${self._VAR}" > {out}; exec sleep 60']
        r = _tmux(*args, env=dict(os.environ, **{self._VAR: client_value}))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.addCleanup(_kill_pid, r.stdout.strip())
        self.assertTrue(_await_file(out),
                        f"the pane for session {name} never wrote its environment out")
        return Path(out).read_text()

    def _require_new_session_env(self) -> None:
        """Skip unless `new-session -e` is a flag this tmux knows.

        Capability, never a version string (this module's own rule). `-e` arrived in
        tmux 3.2 and `commands_frame.cmd_launch` withholds the environment below
        `tmuxctl.SESSION_ENV_FLOOR` precisely because an older tmux does not degrade on
        it — it refuses the command outright. A machine there cannot measure the fix,
        and says which flag it lacked rather than failing as though charter were wrong.
        """
        if not _NEW_SESSION_TAKES_ENV:
            r = _tmux("new-session", "-d", "-s", "envprobe", "-e", "X=1", "--", "true")
            _NEW_SESSION_TAKES_ENV.append(r.returncode == 0)
            _tmux("kill-session", "-t", "envprobe")
        if not _NEW_SESSION_TAKES_ENV[0]:
            self.skipTest("this tmux's `new-session` does not accept `-e`, so the "
                          "environment charter carries with it cannot be measured here")

    def test_a_later_session_inherits_the_servers_environment_not_its_own_clients(self):
        """The defect, reproduced as tmux's own behaviour. The second launcher exports a
        different value and tmux hands its pane the FIRST one — which is why a second
        frame's harness read another frame's id, wrote another frame's workspace pointer,
        and bumped another frame's version while its own panels waited for a change that
        was being recorded somewhere else."""
        self.assertEqual(self._session_reading_the_var("first", client_value="one"),
                         "one", "the launch that STARTS the server does set it")
        self.assertEqual(
            self._session_reading_the_var("second", client_value="two"), "one",
            "if tmux now gives a later session its own client's environment, the `-e` "
            "in `layout.session_argv` is no longer load-bearing — say so rather than "
            "carrying a flag nothing needs")

    def test_the_e_flag_is_what_gets_a_later_session_its_own_value(self):
        """The fix, measured against the same server in the same state. Same two
        sessions, same client environments; the only difference is the `-e` that
        `layout.session_argv` now carries."""
        self._require_new_session_env()
        self.assertEqual(self._session_reading_the_var("first", client_value="one"),
                         "one")
        self.assertEqual(
            self._session_reading_the_var("second", client_value="two", carry="two"),
            "two", "`-e` did not reach the pane `new-session` itself creates")


if __name__ == "__main__":
    unittest.main()
