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

**A capability can also go missing for ONE DEATH, and that is #487.** On a loaded tmux
3.4 a pane's fd closes before tmux has the child's status, and for some deaths it never
gets it: `#{pane_dead}` `1`, `#{pane_dead_status}` empty, the pane's `pane-died` array
never run, permanently. A trial like that measured NOTHING about charter — it is not a
result to assert on and not a failure to report — and every death-dependent test here now
detects it with a constant-action probe hook and spends another pane (`_VoidDeaths`:
`_HOOK_TRIALS`, `_arm_array_probe`, `_the_array_ran`, `_the_array_never_ran`). The
detector asks whether tmux ran the array, never whether the status came back empty: a
harness genuinely killed by a signal also has an empty status, and classifying on that
would make the signal-death test unfailable. Nothing is widened; a machine that produces
`_HOOK_TRIALS` such deaths in a row skips, naming what it could not measure.

**#507 is the same mechanism reaching a class that had no defence against it**, and #494
is the last readiness condition this module assumed rather than waited for. The probe's
evidence now lives in tmux's OWN state rather than in a file it asked a shell to touch
(`_VoidDeaths._PROBE_OPTION`), `EarlyDeathIntegration` — which reads a dead pane's exit
STATUS and is exactly what a void destroys — retries a trial that measured nothing like
everything else here (`_a_measured_death`), and a menu is now WAITED for on the terminal
it is drawn on rather than slept at (:class:`_Screen`).

**The last test that could not carry a probe now carries one — #609, and it took a
measurement to find out that the obstacle written here was not this test's.**
`WindowInsideAnOperatorsTmux.test_nothing_of_the_operators_tmux_is_written_by_a_whole_launch`
reads the code a whole real `cmd_launch` returns, and this paragraph used to say that pane
carried charter's production `pane-died` PAIR — `[0]` the write hook, `[1]` `kill-session`
— leaving no free index. That pair is the PRIVATE-SERVER path's. This test drives
`_launch_in_operator_tmux`, whose own docstring says it installs no hooks on the harness
pane at all ("the harness's exit code travels without hooks here"), and `show-hooks -p` on
that pane during a real launch confirms it: **empty**. `_PROBE_INDEX` was free the whole
time, and the test was asserting where its siblings skip for a reason that was never true
of it.

What IS true of it is the other half, and it is why the probe needed something new: this
launcher runs `kill-window` the instant it has read the code, so the dying pane — and any
option written on it — is gone microseconds later. So the probe's evidence moves to the
SERVER for this one caller (`_VoidDeaths._ON_THE_SERVER`), which outlives the window and
appears in none of the five readings the test snapshots. A void death there is now a
retried trial and then a skip, exactly like everywhere else.

Every test gets its own tmux SESSION (hooks are per-pane, so a shared session would let
one test's hook leak into another's pane) on the ONE socket this module owns, and every
test kills that socket's server on the way out via `addCleanup` — so a failing test
can't leak a stale socket file into `/private/tmp/tmux-<uid>/` any more than a passing
one does.
"""

from __future__ import annotations

import fcntl
import os
import pty
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import threading
import time
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, config, hooks, instance, statusline, todos
from charter.frame import chats, gather, layout, notify, overlay
from charter.frame import slots as frame_slots
from charter.frame import state, tmuxctl

from tests import _tmuxreap
from tests._isolation import PersonaIso, make_plane, run_hook
from tests._planeguard import allow_background_children
# Imported rather than re-declared: a second copy of the stub that keeps a launch's
# detached `frame-gather` child off the developer's real plane is a copy that can drift
# out of step with the production call it stands in for, and `tests/_planeguard.py`
# cannot see a subprocess to tell anyone it drifted. Cross-module test imports are this
# suite's ordinary way of sharing a fixture (`tests.test_hooks.InAControlPlane` has four
# importers).
from tests.test_frame_launcher import _no_real_detached_child

_HAS_TMUX = shutil.which("tmux") is not None

#: Unique per test PROCESS, not merely per class — a socket left behind by an earlier,
#: interrupted run must never collide with (or be mistaken for) this one's.
#:
#: Built by `tests._tmuxreap.name` rather than spelled here, and that is the other half of
#: the same sentence: an interrupted run's socket is not merely something this one must not
#: collide with, it is something this one must CLEAN UP — 14 live servers and 497 stale
#: files had accumulated before anything did (#564). The reaper recognises a socket by the
#: shape that function produces, so a module spelling its own name is a module whose leaks
#: nobody reaps.
SOCKET = _tmuxreap.name("integration-test")

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


def _tmux_on(socket: str, *args: str,
             env: dict | None = None) -> subprocess.CompletedProcess:
    """`tmux -L <socket>`, for a test that has to say WHICH server it means.

    There is more than one server in this module now: `SOCKET` is charter's own, set up
    the way charter sets its own up, and `OP_SOCKET` stands in for a tmux the operator
    already had open, left at tmux's own defaults. A helper hard-wired to one of them is
    how the operator's server came to be charter's server under another name — see
    `OP_SOCKET`'s own comment for what that cost.
    """
    return subprocess.run(["tmux", "-L", socket, *args], capture_output=True, text=True,
                          timeout=10, env=env)


def _tmux(*args: str, env: dict | None = None) -> subprocess.CompletedProcess:
    """`_tmux_on(SOCKET, …)` — charter's own server, the default for this module.

    *env* is `None` by every existing call site (inherit this process's own
    environment, unchanged) — `PanelIntegration` is the one caller that needs a
    DIFFERENT environment for a `new-session` call, so tmux hands its spawned pane a
    throwaway plane's `$CHARTER_ROOT` rather than this test process's real one."""
    return _tmux_on(SOCKET, *args, env=env)


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

#: What a trial helper answers for a death tmux never had the child's status for: a
#: trial that measured NOTHING, which is neither a pass nor a failure.
#:
#: A distinct object rather than `None`, because `None` already means "nothing was
#: delivered" to `_hook_reaches_a_shim`'s own callers — including the negative control,
#: whose whole subject is that nothing was delivered. Collapsing the two would let a void
#: trial satisfy that control, which is the shape of "a test that cannot fail" this
#: module has now shipped several ways.
_VOID = SimpleNamespace(
    __doc__="a death tmux never ran the pane's `pane-died` array for — see `_HOOK_TRIALS`")


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


def _await_text(path: str, timeout: float = _DEADLINE) -> str:
    """Wait for *path* to hold NON-EMPTY text and return it stripped — `""` if it never
    does.

    **`_await_file` is not enough wherever the assertion is about CONTENT.** A shell
    redirection (`echo "$v" > "$path"`, which is exactly what
    `commands_frame._pane_died_write_hook_argv` builds) CREATES the file and then writes
    it, so a poll that stops at `os.path.exists` can land in the gap between the two and
    read a blank where the hook wrote a number. Nothing is widened by waiting for the
    text: a hook that really writes an empty line — delete the `${v:-N}` fallback and it
    does — still spends the deadline and still returns `""`, and the caller's assertion
    on the content fails exactly as it did before, saying the same thing.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            text = Path(path).read_text().strip()
        except OSError:
            text = ""
        if text:
            return text
        time.sleep(0.1)
    try:
        return Path(path).read_text().strip()
    except OSError:
        return ""


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


#: Anything tmux drew that is not TEXT: a CSI/OSC escape sequence, a charset selection,
#: a single-character escape. Stripped from a copy of a client's screen bytes before the
#: search in `_Screen.saw`, so a row tmux styled part-way through (the SELECTED row of a
#: menu carries its own highlight) still reads as the contiguous text it renders as. The
#: raw bytes are searched first and this is only the fallback — nothing here can hide a
#: label, it can only reveal one the styling had split.
_ESCAPES = re.compile(rb"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;?]*[ -/]*[@-~]"
                      rb"|\x1b[()][A-Za-z0-9]|\x1b[@-Z\\-_]")


class _Screen:
    """Everything tmux has written to ONE attached client's terminal since it attached.

    **This is the observable #494 needed and could not find in a format.** A tmux menu is
    a client OVERLAY, not pane content, so `capture-pane` cannot see it and — probed
    exhaustively against tmux 3.7c with two real ptys on one session, all 168 formats
    `display-message -a` names, before and after a `display-menu` — not one client, pane,
    window or session format reports it either. Only `client_written`, a byte counter,
    moved, and it moved for the client with NO menu too. But the menu is drawn, and these
    tests own the master fd of the terminal it is drawn on: the menu's own rows arrive on
    the presser's pty and on nobody else's. Measured: the label appeared on the presser's
    fd 0.05 s after `display-menu`, and never on the other client's.

    So a test can WAIT for the menu instead of sleeping 0.8 s and hoping — which is the
    whole of #494, because a keystroke sent before the menu exists is spent on the pane
    and no later deadline can un-spend it.

    **A THREAD per client, not a drain at the point of asking**, for two reasons. A pty's
    buffer is finite: with nobody reading, tmux eventually blocks writing to the client,
    which is a hang the test would read as "the menu never opened". And `saw()` has to be
    able to answer about output that arrived while the test was looking somewhere else —
    a single accumulating buffer is what makes the answer independent of when it is asked.

    The reader is stopped and JOINED before `_reap_pty` closes the fd (`_attach_pty`
    registers this cleanup last, and `addCleanup` is LIFO). Left running, it would sit in
    a read on a closed fd number that the next `pty.fork` in the same process may well be
    handed — a thread stealing another client's bytes is exactly the kind of cross-test
    coincidence this module keeps finding.
    """

    def __init__(self, fd: int) -> None:
        self._fd = fd
        self._buf = bytearray()
        self._lock = threading.Lock()
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._pump, daemon=True,
                                        name=f"charter-test-screen-{fd}")
        self._thread.start()

    def _pump(self) -> None:
        while not self._stop.is_set():
            try:
                ready, _, _ = select.select([self._fd], [], [], 0.05)
            except (OSError, ValueError):
                return                    # the fd went away under us — nothing to read
            if not ready:
                continue
            try:
                chunk = os.read(self._fd, 65536)
            except OSError:
                return                    # EIO: the pty's child is gone
            if not chunk:
                return
            with self._lock:
                self._buf += chunk

    def stop(self) -> None:
        self._stop.set()
        self._thread.join(timeout=5)

    def saw(self, needle: str) -> bool:
        """Has *needle* been drawn on this client's terminal at any point?"""
        want = needle.encode()
        with self._lock:
            raw = bytes(self._buf)
        return want in raw or want in _ESCAPES.sub(b"", raw)

    def drawn_so_far(self) -> int:
        """How many bytes tmux has drawn on this client. A MARK, never an assertion."""
        with self._lock:
            return len(self._buf)

    def await_more_drawn(self, mark: int, timeout: float = _DEADLINE) -> bool:
        """Wait until tmux has drawn anything at all on this client past *mark*.

        For the one readiness condition that has no text to wait for: a key that moves a
        menu's SELECTION repaints rows whose labels are already on screen, so there is no
        new string to look for — only that tmux painted again. Weaker than
        :meth:`await_drawn` on its own, and used only where a pty's own FIFO ordering
        carries the rest of the argument: tmux cannot paint a response to a key without
        having first read every key written before it.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.drawn_so_far() > mark:
                return True
            time.sleep(0.05)
        return self.drawn_so_far() > mark

    def await_drawn(self, needle: str, timeout: float = _DEADLINE) -> bool:
        """Wait until *needle* has been drawn on this client, up to *timeout*.

        A POSITIVE wait, so it spends `_DEADLINE` — see that constant. It returns the
        instant the text is on screen, so the number is what a loaded runner may take and
        never what a healthy one does; the caller asserts on the answer.
        """
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.saw(needle):
                return True
            time.sleep(0.05)
        return self.saw(needle)


def _await_pane_changed(session: str, before: str, timeout: float = _DEADLINE) -> str:
    """Wait until `capture-pane` of *session* differs from *before*, and return it.

    The readiness condition for the NEGATIVE half of #494's two-client tests. "Client B's
    keystroke must not select in A's menu" says nothing at all unless B's keystroke was
    really delivered and really spent — a keystroke still in flight satisfies it for free,
    which is what the fixed sleep it replaces was quietly relying on. A client with no
    menu open sends its keys to the pane, whose `cat` echoes them, so the pane's own text
    changing is tmux itself reporting that B's key landed on B's pane. Returns *before*
    unchanged if it never does, and the caller fails on that.
    """
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        now = _tmux("capture-pane", "-p", "-t", session).stdout
        if now != before:
            return now
        time.sleep(0.05)
    return _tmux("capture-pane", "-p", "-t", session).stdout


class _NeedsAttachedClient:
    """`_attach_pty` for the two classes that need a REAL, ATTACHED client, and the
    capability probe that decides whether this machine can give them one.

    `display-menu` refuses outright ("no current client") without an attached client, so
    both classes below are unrunnable — not passable, not failable — where tmux will not
    attach one. They skip, naming what they could not get and quoting tmux's own refusal,
    rather than asserting anything about a menu nothing drew.
    """

    def _attach_pty(self, session: str,
                    exclude: frozenset = frozenset()) -> tuple[str, int, _Screen]:
        """Forks a real `tmux attach -t session` under a pty — the one way to hand
        `display-menu` a client it will actually accept for `-c`/`-t` targeting.

        *exclude* names clients the caller already knows about: `list-clients -t session`
        lists every client attached to it, not only the one just forked, so a SECOND pty
        on the same session has to pick its own name out from among several.

        Registers the fork's own cleanup (SIGKILL, then reap — see `_reap_pty`) before
        returning the attached client's name (`#{client_name}`, read back from tmux
        itself once the attachment has had time to register), the pty's own master fd
        — writing a KEY to the fd (not `tmux send-keys`, confirmed by hand: `send-keys`
        feeds a PANE's own input queue, which an active menu overlay never reads from) is
        the only way found to actually select a menu item from here — and a :class:`_Screen`
        watching everything tmux draws on that client, which is what lets a caller wait
        for a menu rather than sleep and assume one (#494).

        The screen's cleanup is registered AFTER `_reap_pty`'s, so LIFO stops the reader
        thread FIRST and the fd is never closed out from under it.
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
                screen = _Screen(fd)
                self.addCleanup(screen.stop)
                return name, fd, screen
            refusals.append(f"TERM={term}: {_refusal(fd) or '(tmux printed nothing)'}")
            _reap_pty(pid, fd)
        self.skipTest(
            "no tmux client can attach on this machine, and a rendered `display-menu` "
            "needs one — tmux refused every terminal type tried: " + " | ".join(refusals))


class _VoidDeaths:
    """Telling a RESULT from a trial that measured nothing — #409, #487, #507.

    On a loaded tmux 3.4 a pane's fd can close before tmux has the child's status, and
    for some deaths it never gets it: `#{pane_dead}` `1`, `#{pane_dead_status}` EMPTY,
    the pane's `pane-died` array never run, permanently. A trial like that says nothing
    about charter — it is not a result to assert on and not a failure to report.

    **A mixin of its own rather than part of `_TmuxServerFixture`, and #507 is why.**
    `EarlyDeathIntegration` is a plain `unittest.TestCase` that builds its own sessions
    from `layout.session_argv`, so it inherited none of this — and three of its tests
    read a dead pane's STATUS, which is precisely what a void destroys.
    `test_a_lone_argument_reaches_a_shell_not_execvp` failing on two Python versions in
    one CI run and passing on a re-run of identical bytes is that: `_query_pane_dead_status`
    answers `commands_frame._UNKNOWN_DEATH_CODE` (1) for a pane tmux holds no status for,
    and `assertEqual(code, 7)` cannot tell that from a shell that really exited 1.

    Everything here is expressed against `self._srv`, so a class brings its own server.
    """

    #: Which server this class's tests stand up. `SOCKET` is charter's own; the class
    #: that is about the tmux charter is a GUEST on overrides it, because a guest server
    #: charter's own fixture has already configured is not a guest server (`OP_SOCKET`).
    SOCKET_NAME = SOCKET

    def _srv(self, *args: str, env: dict | None = None) -> subprocess.CompletedProcess:
        """`tmux` against THIS class's own server — never the module-level `_tmux`, which
        is `SOCKET` and `SOCKET` only."""
        return _tmux_on(self.SOCKET_NAME, *args, env=env)
    #: How many deaths one death-dependent test may spend before it gives up on this
    #: machine — #409, and #487 for every test in the module rather than two of them.
    #:
    #: **Not a retry for flakiness: a retry for a trial that measured nothing.** Measured
    #: on tmux 3.4 in an Ubuntu 24.04 container pinned to one cpu against twelve spin
    #: loops, running the real `respawn-pane`/`_pane_state` path 118 times: 111 deaths
    #: reported `#{pane_dead}` `1` with `#{pane_dead_status}` `33` and ran the pane's
    #: `pane-died` array; the other 7 reported `1` with an EMPTY status and never ran the
    #: array — and PERMANENTLY, polling a further 8 seconds never filled either in. tmux
    #: 3.4's `server_destroy_pane` returns without notifying while `PANE_STATUSREADY` is
    #: unset, and for those deaths it is never set. A longer deadline is therefore not
    #: the knob: it would re-ship the same flake with a longer wait attached to it.
    #:
    #: What a death like that produces is not a wrong answer, it is NO answer — there is
    #: nothing for the test to be about — so the trial is spent again on a fresh pane and
    #: the assertions stay exactly as strict as they were.
    _HOOK_TRIALS = 3

    #: Where this module's constant-action probe hook goes in a pane's `pane-died` array.
    #:
    #: `1` everywhere, whether or not charter armed a `[0]`: tmux runs an array in index
    #: order, so a probe here cannot fire before charter's own hook, and a SPARSE array
    #: holding only `[1]` still runs (measured on both tmux 3.4 and 3.7c).
    #:
    #: **Which is also where `commands_frame._pane_died_teardown_hook_argv` lands**, so a
    #: probe must never be armed on a pane carrying the production harness PAIR — it
    #: would replace the teardown hook and the test would be measuring its own probe.
    #: No caller here does: `_a_death_the_hook_saw` installs the write hook only, and
    #: `test_the_write_hook_must_be_installed_before_the_teardown_hook`, which installs
    #: both, asserts on the array itself and needs no probe.
    _PROBE_INDEX = 1

    #: The tmux PANE OPTION the probe hook sets, and reads back, as its whole evidence.
    #:
    #: **It used to be a FILE, and that is the weakness #495's own attacker flagged and
    #: #507 folded in here.** The action was `run-shell "touch <marker>"`, so "tmux never
    #: ran the pane's hook array" and "the marker could not be written" were the same
    #: reading: anything that made the path unwritable — a full disk, a `_gate_dir` gone,
    #: a `run-shell` that could not fork — was indistinguishable from a void and got the
    #: trial spent again. Sabotaging the probe's own action (`touch` -> `true`) turned a
    #: real failure into a SKIP, which is a suite reporting green about a machine it
    #: never measured.
    #:
    #: A tmux option is set BY TMUX, in the same act as running the array, with no shell,
    #: no fork and no filesystem in between — so "the array ran" and "a marker got
    #: written" stop being one question. Measured against tmux 3.7c: a `pane-died` hook
    #: whose action is `set-option -p @charter-probe ran` sets it on the DYING pane
    #: (`-p` resolves to the hook's own pane), it survives on a dead-but-kept pane, and
    #: `show-options -p -v` on a pane that never ran the hook exits 1 with `invalid
    #: option` and prints nothing — three distinguishable answers where the file had two.
    _PROBE_OPTION = "@charter-probe"

    #: The two scopes the probe's evidence can be kept at, spelled as the flag both
    #: `set-option` and `show-options` take for it.
    #:
    #: **`-p` — the dying pane's own option — is the default and was the only one until
    #: #609.** The pane is kept by `remain-on-exit`, so it is still there to be asked
    #: afterwards and the evidence sits on the very thing it is evidence about.
    #:
    #: **`-s` — the server's — is for a caller whose pane is torn down before it can
    #: ask.** `WindowInsideAnOperatorsTmux
    #: .test_nothing_of_the_operators_tmux_is_written_by_a_whole_launch` drives a REAL
    #: `cmd_launch`, and `_launch_in_operator_tmux` runs `kill-window` the instant it has
    #: read the code — so a pane-scoped answer is destroyed by the very path under test,
    #: microseconds after it is written. Measured against tmux 3.7c: a `pane-died` hook
    #: whose action is `set-option -s @charter-probe ran` fires on the dying pane, and the
    #: option survives that `kill-window` and the frame's whole session. Everything #507
    #: says about an option over a file holds unchanged at either scope — it is set BY
    #: TMUX, in the same act as running the array, with no shell, no fork and no
    #: filesystem in between.
    #:
    #: **And it cannot answer that test's own question for it**, which is the thing to
    #: check before a test writes anything on a server it is asserting nobody wrote to.
    #: Measured, on the same server, after the hook had fired: the option appears in NONE
    #: of the five readings that test snapshots — `show-options -g`, `show-options -t
    #: <their session>`, `show-options -g -w`, `show-options -w -t <their pane>`, and
    #: `list-keys` all come back without it, because a SERVER option is none of those
    #: scopes.
    _ON_THE_PANE, _ON_THE_SERVER = "-p", "-s"

    def _probe_where(self, scope: str, pane: str) -> tuple[str, ...]:
        """The scope flag and the target that goes with it: a pane for `-p`, and for `-s`
        nothing at all, because a server is not a thing there can be two of."""
        return (scope, "-t", pane) if scope == self._ON_THE_PANE else (scope,)

    def _probe_action(self, scope: str) -> str:
        """What the probe hook's action IS, at *scope*. A tmux command, not a shell command
        line: nothing charter builds appears in it, no `#{pane_dead_status}`, no
        `set-environment` value — so the only thing it can ever report is "tmux reached
        this pane's hook array", which on both tmuxes this suite must pass on is the same
        question as "did tmux have the child's status"."""
        return f"set-option {scope} {self._PROBE_OPTION} ran"

    def _the_array_ran(self, pane: str, timeout: float = _DEADLINE, *,
                       scope: str = _ON_THE_PANE) -> bool:
        """Did tmux run *pane*'s `pane-died` array? Polled, up to *timeout*.

        Reads the option back off tmux itself. An unarmed or unrun pane answers rc 1 and
        an empty value; a pane whose array ran answers `ran`. Anything else is treated as
        not-run, and `_the_array_never_ran` is what then refuses to accept that reading
        unless the pane really is in the one state it is allowed to mean.
        """
        deadline = time.monotonic() + timeout
        while True:
            got = self._srv("show-options", "-v", *self._probe_where(scope, pane),
                            self._PROBE_OPTION)
            if got.returncode == 0 and got.stdout.strip() == "ran":
                return True
            if time.monotonic() >= deadline:
                return False
            time.sleep(0.1)

    def _arm_array_probe(self, pane: str, *, scope: str = _ON_THE_PANE) -> None:
        """Install the CONSTANT-action `pane-died` probe hook on *pane*. **The one
        observable that tells a result from a trial that measured nothing.**

        **Call this AFTER whatever hook the test is about, never before.** `set-hook -p
        -t <pane> pane-died <action>` with no index REPLACES the whole array, so a probe
        installed first is silently wiped by the very hook it exists to watch — and the
        trial then looks like a void that also, impossibly, has a status. That is not
        theoretical: it is what the first version of `_a_hook_delivery` did, and
        `_the_array_never_ran` is what turned it into three loud failures rather than
        three silent skips.

        **The option is written and read back HERE, before the hook is armed**, and that
        round trip is not ceremony. Every caller reads a missing option as "tmux never
        ran the array" — so a tmux that cannot carry a pane-scoped user option at all, or
        cannot report one back, would turn every death in this module into a void and
        every death-dependent test into a skip, on a machine where nothing was wrong. The
        probe proves it can record and recall before anything depends on it recalling,
        and says so by name if it cannot.

        **Reading the pane's STATUS instead is a SPELLING, and #487 is the bill for it.**
        An empty `#{pane_dead_status}` on a dead pane is produced by two different
        realities, and `commands_frame._pane_state` folds both to the same
        `_UNKNOWN_DEATH_CODE`. Measured on tmux 3.4 and re-measured on 3.7c: a harness
        genuinely killed by a signal answers `1:` and the array RUNS (40/40 on 3.4, 15/15
        on 3.7c); a death tmux never got the status of answers `1:` and the array never
        runs (7/7 of the voids in `_HOOK_TRIALS`'s 118-trial run). A test that classified
        on the empty status alone would have to treat a real signal death as a void
        trial — which is how a suite stops being able to fail.

        **The HOOK is pane-scoped whatever *scope* says; only its evidence moves.**
        `set-hook -p -t <pane>` is what makes this one pane's array the subject, and
        widening that to `-g` would arm every pane on the server — including ones another
        test in the same class is about. *scope* decides only where the option the action
        writes LIVES: on the dying pane, or on the server that outlives it. See
        :data:`_ON_THE_PANE`.
        """
        where = self._probe_where(scope, pane)
        self.assertEqual(
            self._srv("set-option", *where, self._PROBE_OPTION, "arming").returncode, 0,
            f"this tmux will not set the {scope} option ({self._PROBE_OPTION}) every void "
            "check in this module reads — a missing option would then mean 'tmux ran no "
            "hook' everywhere and skip every death-dependent test on a healthy machine")
        back = self._srv("show-options", "-v", *where, self._PROBE_OPTION)
        self.assertEqual(
            (back.returncode, back.stdout.strip()), (0, "arming"),
            f"this tmux set {self._PROBE_OPTION} and would not report it back "
            f"({back.returncode}, {back.stdout!r}) — see above")
        self.assertEqual(
            self._srv("set-option", "-u", *where, self._PROBE_OPTION).returncode, 0)
        self.assertEqual(
            self._srv("set-hook", "-p", "-t", pane, f"pane-died[{self._PROBE_INDEX}]",
                      self._probe_action(scope)).returncode, 0)

    def _the_array_never_ran(self, pane: str, what: str) -> None:
        """Assert *pane* is in the ONE state an unset probe option is allowed to mean:
        dead, with tmux holding no status for it at all.

        The void is asserted here rather than assumed, so the only way a trial can be
        retried instead of failing is by actually being the measured condition. A pane
        tmux has DESTROYED (nothing armed `remain-on-exit`, or charter stopped arming it)
        and a pane tmux has a status for but ran no hook for are both real defects, and
        both say which one they are instead of being spent as another trial.
        """
        dead, _, status = self._srv(
            "display-message", "-p", "-t", pane,
            commands_frame._DEAD_FORMAT).stdout.strip().partition(":")
        self.assertEqual(
            dead, "1",
            f"{what}, and tmux does not report the pane dead either (`{dead}:{status}`) "
            "— the pane was destroyed rather than kept, which is a failure of whatever "
            "was supposed to arm `remain-on-exit` and NOT the tmux 3.4 window "
            "`_HOOK_TRIALS` describes")
        self.assertEqual(
            status.strip(), "",
            f"{what}, yet tmux is holding a status ({status.strip()!r}) for it. tmux "
            "runs a pane's `pane-died` array once it has the child's status, so a status "
            "with no hook run is charter's hook having been lost, not a trial that "
            "measured nothing")

    def _no_death_was_delivered(self):
        self.skipTest(
            f"none of {self._HOOK_TRIALS} deaths on this machine reached a `pane-died` "
            "hook at all — tmux never had the child's status when the pane's fd closed "
            "(measured on tmux 3.4 under load; see `_HOOK_TRIALS`). The capability this "
            "test measures was not present to measure, and nothing here is widened to "
            "pass without it.")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class _TmuxServerFixture(_VoidDeaths):
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

    #: Whether `_new_pane` arms `remain-on-exit` server-globally, the way
    #: `commands_frame._PLACEHOLDER_CONF` does on charter's own private server.
    #:
    #: **A class that is about somebody else's tmux must turn this OFF, and the whole of
    #: #408's second half is why.** With it on, every pane on the server keeps its corpse
    #: and therefore fires `pane-died` — including panes charter did nothing to. A test
    #: for "charter makes a dying panel reachable" then passes on the FIXTURE's option
    #: and cannot see charter failing to set one; measured, by deleting the production
    #: call and watching the test stay green.
    ARMS_REMAIN_ON_EXIT = True

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
        fix. tmux computes this path itself from `-L <socket>`; matched here rather than
        asked of tmux because there is no query command for it, only observed behaviour
        (`/tmp/tmux-<getuid()>/<socket name>` on every platform this repo runs on)."""
        self._srv("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / self.SOCKET_NAME).unlink(missing_ok=True)

    def _new_pane(self, dies_by: str = "exit 0") -> tuple[str, str, str]:
        """A fresh session on this class's server; its name, pane id and GATE.

        The pane runs a program that WAITS for its gate file and then dies by *dies_by* —
        the caller opens the gate (`_release`) when it wants the death. See `_gate_argv`
        for why the pane is never driven with `send-keys` instead, which is the shape this
        module used to use and the reason it failed roughly one CI run in ten.

        `remain-on-exit` is armed globally here only when `ARMS_REMAIN_ON_EXIT` says so —
        see that attribute for why a class about somebody else's tmux must not.
        """
        self._pane_counter += 1
        name = f"p{self._pane_counter}"
        gate = os.path.join(self._gate_dir, f"gate-{name}")
        r = self._srv("new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", *_gate_argv(gate, dies_by))
        self.assertEqual(r.returncode, 0, r.stderr)
        if self.ARMS_REMAIN_ON_EXIT:
            # Global on this socket's server — cheap to repeat per pane, and every frame
            # on CHARTER'S OWN server wants it regardless (see
            # `commands_frame._PLACEHOLDER_CONF`'s own docstring).
            self._srv("set", "-g", "remain-on-exit", "on")
        return name, r.stdout.strip(), gate

    @staticmethod
    def _release(gate: str) -> None:
        """Open a pane's gate: its program stops waiting and dies the way it was built to."""
        Path(gate).touch()

    def _hook_reaches_a_shim(self, *, socket, pane, gate, interpreter_dir,
                             timeout=_DEADLINE, probe=False):
        """Arm *pane*'s respawn hook with a shim as charter's interpreter, open the gate,
        and return whatever argv the shim recorded (``None`` if it never ran).

        The shim stands in for `sys.executable`, so the argv it records is what charter
        would REALLY have been invoked with — and *interpreter_dir* is how a caller
        chooses what shape of path that is.

        *timeout* is the full `_DEADLINE` for a caller expecting the hook to fire — a
        slow machine must come back slow, never wrong. A caller expecting NOTHING passes
        a shorter one and earns the right to by establishing separately that the pane is
        GONE: once tmux has destroyed a pane, `pane-died` for it can no longer fire at
        any deadline, so waiting longer proves nothing that the pane's absence does not.

        *probe* asks for `_arm_array_probe`'s constant-action hook to be armed BESIDE
        charter's own — after it, since an unindexed `set-hook` replaces the array — for
        a caller that intends to tell a void trial from a real one (`_a_hook_delivery`
        does the telling). Its option is waited on FIRST, so a void costs one deadline
        rather than two, and it is the caller, never this method, that decides what an
        unset option means.
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
        if probe:
            self._arm_array_probe(pane)
        self._release(gate)
        if probe and not self._the_array_ran(pane, timeout):
            return None
        # The shim's own `printf … > marker` creates before it writes, so the readiness
        # condition is TEXT rather than existence — see `_await_text`.
        recorded = _await_text(marker, timeout)
        if not recorded:
            return None
        return recorded.split()

    def _a_hook_delivery(self, *, socket, pane, gate, interpreter_dir):
        """One trial of "this pane dies and charter's own `pane-died` hook reaches a
        shell carrying the argv charter meant".

        Returns the recorded argv, or :data:`_VOID` for a death tmux never ran the pane's
        hook array for at all — see `_HOOK_TRIALS`.

        **An array that RAN and delivered nothing is a failure, not a void trial**, and
        that is the half that keeps the callers able to fail: break `_panel_died_hook_argv`
        so the action reaches no shell and the probe beside it still fires, so this says
        so rather than spending another pane and skipping.
        """
        seen = self._hook_reaches_a_shim(socket=socket, pane=pane, gate=gate,
                                         interpreter_dir=interpreter_dir, probe=True)
        if seen is not None:
            return seen
        self.assertFalse(
            self._the_array_ran(pane, timeout=0),
            "tmux ran this pane's `pane-died` array — a constant-action probe hook "
            "beside charter's own fired — and charter's own hook delivered no argv at "
            "all. That is charter, not the runner: "
            f"{self._srv('show-hooks', '-p', '-t', pane).stdout!r}")
        self._the_array_never_ran(pane, "tmux never ran this pane's `pane-died` array")
        return _VOID

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

        **`_HOOK_TRIALS` deaths, not one, and #487 is why.** A SINGLE death answering
        "does not fire" is exactly the void this module measures: on tmux 3.4 under load
        roughly one death in seventeen never reaches a hook at all, however healthy the
        machine's tmux is. Cached process-wide off one such death, this probe would skip
        every hook test in the module for the rest of the run — and a skip reads as
        green, so the module would be at its most silent precisely on the loaded runners
        it is meant to be honest about. The answer cached here is "not one of
        `_HOOK_TRIALS` deaths fired a hook", which no healthy tmux produces.

        **The probe sets `remain-on-exit` itself rather than inheriting it from the
        fixture**, which it used to do and which made it a probe of the fixture as much
        as of tmux: on a class with `ARMS_REMAIN_ON_EXIT` off it would answer "does not
        fire" everywhere and skip every test that asks. Window-scoped, because that is
        the scope with no version floor under it — `set-option -p` is tmux 3.0+, and a
        probe that reported a capability missing because the SCOPE was unsupported would
        skip tests for the wrong reason.
        """
        if not _PANE_DIED_FIRES:
            tmp = tempfile.mkdtemp(prefix="charter-integ-probe-")
            self.addCleanup(shutil.rmtree, tmp, True)
            for attempt in range(self._HOOK_TRIALS):
                _, pane, gate = self._new_pane("exit 7")
                marker = os.path.join(tmp, f"fired-{attempt}")
                self._srv("set-option", "-w", "-t", pane, "remain-on-exit", "on")
                self._srv("set-hook", "-p", "-t", pane, "pane-died",
                          f'run-shell "touch {marker}"')
                self._release(gate)
                if _await_file(marker):
                    _PANE_DIED_FIRES.append(True)
                    break
            else:
                _PANE_DIED_FIRES.append(False)
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
        self.assertTrue(_await_file(armed),
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
        at all.

        Tried up to `_HOOK_TRIALS` times, for #487's reason and not for flakiness' sake:
        a death tmux never ran a hook for measured nothing about this argv, and
        `_a_hook_delivery` fails rather than retries for every other way of delivering
        nothing."""
        self._require_pane_died_fires()
        for _ in range(self._HOOK_TRIALS):
            _, pane, gate = self._new_pane("exit 3")
            seen = self._a_hook_delivery(
                socket=SOCKET, pane=pane, gate=gate,
                interpreter_dir=os.path.join(self._gate_dir, "interp dir"))
            if seen is _VOID:
                continue
            self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                    "--pane", pane, "--frame", "demo-1"])
            return
        self._no_death_was_delivered()

    def test_an_awkward_interpreter_path_arrives_byte_for_byte(self):
        """The property `_ACTION_QUOTE_BREAKERS` is derived from, measured rather than
        argued: every ASCII punctuation character EXCEPT the six that mean something to
        one of the three parsers involved is literal on the way through, so an
        interpreter living behind one is armed rather than refused — and the argv that
        comes out the far side is the one charter meant.

        Without this the guard could be tightened to a paranoid allowlist and nothing
        would notice; with it, a tightening that costs a real path its respawn fails
        here.

        Tried up to `_HOOK_TRIALS` times — see `_a_hook_delivery`, and #487."""
        self._require_pane_died_fires()
        awkward = "a b;c&d(e)f*g-h,i=j+k@l:m[n]o{p}q!r%s^t~u"
        for _ in range(self._HOOK_TRIALS):
            _, pane, gate = self._new_pane("exit 3")
            seen = self._a_hook_delivery(
                socket=SOCKET, pane=pane, gate=gate,
                interpreter_dir=os.path.join(self._gate_dir, awkward))
            if seen is _VOID:
                continue
            self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                    "--pane", pane, "--frame", "demo-1"])
            return
        self._no_death_was_delivered()

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
        expands to text the program in that pane sets for itself.

        Tried up to `_HOOK_TRIALS` times, with the constant-action probe beside the hook
        under test: a missing file here used to be reported as "the hook never reached a
        shell", which on a loaded tmux 3.4 was true and meant nothing — tmux had run no
        hook for that pane at all (#487). The probe firing and this file still missing is
        the case that keeps its failure."""
        self._require_pane_died_fires()
        for attempt in range(self._HOOK_TRIALS):
            _, pane, gate = self._new_pane("exit 3")
            # A fresh path per trial, so a file left by an earlier trial can never be
            # what a later one reads back.
            out = os.path.join(self._gate_dir, f"expanded-{attempt}")
            self.assertEqual(
                _tmux("set-hook", "-p", "-t", pane, "pane-died",
                      f"""run-shell -b 'echo "/opt/py#{{pane_id}}/x" > "{out}"'"""
                      ).returncode, 0)
            self._arm_array_probe(pane)
            self._release(gate)
            if not self._the_array_ran(pane):
                self._the_array_never_ran(
                    pane, "tmux never ran this pane's `pane-died` array")
                continue
            seen = _await_text(out)
            wrote = "an empty file" if os.path.exists(out) else "no file at all"
            self.assertTrue(
                seen,
                "tmux ran this pane's `pane-died` array — a constant-action probe hook "
                "beside the one under test fired — and the hook under test left "
                f"{wrote}. That is not the runner.")
            self.assertNotEqual(
                seen, "/opt/py#{pane_id}/x",
                "tmux no longer expands formats inside a hook action — if that is really "
                "true, `_pane_died_write_hook_argv`'s `#{pane_dead_status}` has stopped "
                "working too, which is a much larger thing than this test")
            self.assertEqual(seen, f"/opt/py{pane}/x",
                             "the text was rewritten, but not into the pane id — read "
                             "what this tmux actually does before trusting the guard's "
                             "reasoning")
            return
        self._no_death_was_delivered()

    def _a_death_the_hook_saw(self, dies_by: str, status_path: str):
        """One trial of "a pane dies, charter's write hook records its status".

        *status_path* must be spelled ``<root>/<chat>/exit``: the write hook builds it
        from ``$CHARTER_FRAME_EXIT`` (the frame root, carried by `set-environment`) and
        ``#{@charter_chat}`` (the chat, read off this window's own option), so this helper
        splits it back into the two halves and delivers each the way production does.

        Returns the status file's CONTENT, or ``None`` when tmux never ran this pane's
        `pane-died` array at all — the tmux 3.4 window :data:`_HOOK_TRIALS` describes,
        which is a trial that measured nothing rather than a result. The probe hook that
        tells those apart is `_arm_array_probe`'s; see it for why the empty status is not
        the thing to classify on.

        **A fired array with no file is a FAILURE, not a void trial**, and that is the
        half that keeps this test able to fail. Delete the `${v:-N}` fallback and the
        hook still fires and still writes — an empty line — so the file exists and its
        content is asserted. Break the hook so nothing is written at all and the probe
        still fires, so this says so rather than skipping.
        """
        session, pane, gate = self._new_pane(dies_by)
        # The variable carries the frame ROOT and the hook appends this window's own
        # chat, so the caller's *status_path* is `<root>/<chat>/exit` and BOTH halves are
        # exercised: a hostile root through `set-environment`, and the chat through a
        # tmux format expanded in the dying pane's own context.
        root, chat = os.path.split(os.path.dirname(status_path))
        self.assertEqual(_run(commands_frame._exit_path_env_argv(
            socket=SOCKET, session=session, frame_root=root)).returncode, 0)
        self.assertEqual(_run(commands_frame._chat_option_argv(
            socket=SOCKET, harness_pane=pane, chat=chat)).returncode, 0)
        self.assertEqual(_run(commands_frame._pane_died_write_hook_argv(
            socket=SOCKET, harness_pane=pane)).returncode, 0)
        self._arm_array_probe(pane)
        self._release(gate)
        if not self._the_array_ran(pane):
            self._the_array_never_ran(
                pane, "tmux never ran this pane's `pane-died` array")
            return None, pane
        content = _await_text(status_path)
        self.assertTrue(
            os.path.exists(status_path),
            "tmux ran this pane's `pane-died` array — a constant-action probe hook "
            "beside charter's own fired — and charter's write hook produced no file at "
            "all. That is charter, not the runner.")
        # `_await_text`, not `_await_file` then read: `echo … > path` creates the file
        # before it writes it, so existence alone can hand back the blank in between.
        # An empty line is still returned as `""` here, which is what the caller's
        # assertion about the sentinel is about.
        return content, pane

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
        # `<hostile root>/<chat>/exit`: the root is what `set-environment` carries and the
        # chat is what the hook's own `#{@charter_chat}` expands to, so the injection
        # attempt is still in the half that travels out of band.
        os.makedirs(os.path.join(hostile_dir, "hostile.1"), exist_ok=True)
        status_path = os.path.join(hostile_dir, "hostile.1", "exit")

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
            # what a later one reads back. `<root>/<chat>/exit` is the shape the write
            # hook produces now — the root out of band, the chat expanded from the
            # window's own `@charter_chat`.
            status_path = os.path.join(tmp, f"sig.{attempt}", "exit")
            os.makedirs(os.path.dirname(status_path), exist_ok=True)
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

    def test_recomputed_sizes_really_hold_across_real_window_resizes(self):
        """Cross-task fix round, item 3: tmux's own layout engine redistributes EVERY
        pane proportionally on ANY resize, `-l size` notwithstanding — hand-verified
        against this exact tmux binary during development: a 120x30 frame grown to
        200x50 stretched a one-row panel to 8 rows, and only snapped back to 1 row on
        the way down because that particular shrink happened to be an exact round trip
        of the same grow. Three resizes here (grow, shrink smaller than the original,
        grow past the first grow) rule out "only works for a round trip".

        **What is driven changed with #488.** The `window-resized` hook used to carry
        `resize-pane -t %N -y 1` as literal text and this test installed it. `bottom` is
        content-and-window sized now, so the correction has to be RECOMPUTED against the
        window that just changed — `commands_frame._reassert_sizes`, the real function
        the hook's `charter frame-resize` child calls, is driven here instead. That the
        hook actually fires and actually reaches that child is proven end to end against
        a real frame by `FourEdgeIntegration.
        test_the_resize_hook_really_fires_and_charter_really_recomputes`.

        The measured failure the CAP exists for is asserted here too, on the 90x25 pass:
        a `bottom` that wanted more rows than the window can spare must not be granted
        them, because tmux does not refuse an over-large `-y` — it takes the difference
        out of the neighbour, and the neighbour is the harness (measured on 3.7c:
        `resize-pane -y 40` in a 20-row window left the harness pane 1 row tall)."""
        fid = state.frame_id("rsz", os.getpid())
        r = _tmux("new-session", "-d", "-s", "rsz", "-x", "120", "-y", "30",
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        top = _tmux("split-window", "-t", harness_pane, "-v", "-b", "-l", "1",
                   "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(top.returncode, 0, top.stderr)
        top_pane = top.stdout.strip()
        bot = _tmux("split-window", "-t", harness_pane, "-v", "-l", "1",
                    "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(bot.returncode, 0, bot.stderr)
        bottom_pane = bot.stdout.strip()
        tab = _tmux("split-window", "-t", harness_pane, "-v", "-l", "6",
                    "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(tab.returncode, 0, tab.stderr)
        panes = {"top": top_pane, "bottom": bottom_pane, "repos": tab.stdout.strip()}

        # The last size is the one the CAP binds at: at 22 rows the window can spare
        # only `22 - top(1) - bottom(1) - 3 borders - HARNESS_MIN_ROWS` = 5, fewer than
        # the 6 the content wants — so `repos` must come out a different number there
        # than at the other three, and a `repos_rows` that ignored *window_rows*
        # entirely would still pass the first three.
        for cols, rows in ((200, 50), (90, 25), (300, 100), (120, 22)):
            r = _tmux("resize-window", "-t", "rsz", "-x", str(cols), "-y", str(rows))
            self.assertEqual(r.returncode, 0, r.stderr)
            with mock.patch("charter.frame.slots.repos_rows_wanted", return_value=6):
                commands_frame._reassert_sizes(SOCKET, fid=fid, panes=panes,
                                               harness_pane=harness_pane,
                                               window_cols=cols, window_rows=rows)
            want = layout.slot_sizes(["top", "bottom", "repos"], window_rows=rows,
                                     content_rows=6)
            if rows == 22:
                self.assertLess(want["repos"], 6,
                                "the cap never bound — this loop's last pass is the "
                                "only one that exercises it")
            for slot, pane in panes.items():
                height = _tmux("display-message", "-p", "-t", pane,
                               "#{pane_height}").stdout.strip()
                self.assertEqual(height, str(want[slot]),
                                 f"the {slot} panel is {height} rows after resizing to "
                                 f"{cols}x{rows}, not the {want[slot]} it was told")
            harness = _tmux("display-message", "-p", "-t", harness_pane,
                            "#{pane_height}").stdout.strip()
            self.assertGreaterEqual(
                int(harness), layout.HARNESS_MIN_ROWS,
                f"the harness kept only {harness} rows at {cols}x{rows} — tmux grants "
                f"an over-large -y out of the neighbour rather than refusing it")
            self.assertEqual(
                int(harness), layout.harness_rows(want, window_rows=rows),
                f"the harness is {harness} rows at {cols}x{rows}, not the "
                f"{layout.harness_rows(want, window_rows=rows)} it was told — and it is "
                f"the pane #515 has to name explicitly, because with three strips the "
                f"two below it trade rows with each other and never with it")

    def test_a_pinned_table_really_lands_on_its_committed_height(self):
        """A `size` on the repo table's `[[frame.component]]` table, driven against real
        tmux — and it is the DESIGN this proves, not only the arithmetic.

        The strip is the one pane `_reassert_sizes` never asserts a height for: in a stack
        of N panes only N-1 boundaries are free, so `top`, `bottom` and the harness are
        told their sizes and the table takes the remainder. That is the mechanism a
        content-sized table already uses, and the whole claim of a pin is that it changes
        the NUMBER the others are sized around and nothing else. Nothing inside charter
        can check that claim — the remainder is tmux's arithmetic, not charter's — so it
        is asked of tmux.

        Which is also why the obvious alternative is not what shipped. Calling a pinned
        table a FIXED row and giving it a `layout.resize_flag` axis would assert all N
        heights, and that is the measured failure `_reassert_sizes`' own docstring
        carries: at 200x50, asserting `top`, `bottom` and `repos` in split order left the
        table 1 row and the attention strip 6, the two sizes having swapped panes.

        `repos_rows_wanted` is stubbed to a number the pin must beat and one it must not
        lose to — 2 clones and 30 — because a pin that were quietly still content-sized
        would pass a single-content-count version of this test at whichever of the two
        happened to match.
        """
        fid = state.frame_id("pin", os.getpid())
        r = _tmux("new-session", "-d", "-s", "pin", "-x", "120", "-y", "50",
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        panes = {}
        for slot, args in (("top", ("-v", "-b", "-l", "1")),
                           ("bottom", ("-v", "-l", "1")),
                           ("repos", ("-v", "-l", "6"))):
            out = _tmux("split-window", "-t", harness_pane, *args,
                        "-P", "-F", "#{pane_id}", "--", "sleep", "600")
            self.assertEqual(out.returncode, 0, out.stderr)
            panes[slot] = out.stdout.strip()

        pinned = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "attention"},
            {"use": "repos", "size": 15}, {"use": "sidebar"}]}})
        self.assertEqual(
            [p["size"].n for p in pinned["components"] if p["slot"] == "repos"], [15],
            "the arrangement did not resolve the pin at all — everything below this "
            "would then be asserting the content-sized behaviour under a new name")

        for content in (2, 30):
            with self.subTest(content=content):
                with mock.patch.dict(config.FRAME, pinned), \
                        mock.patch("charter.frame.slots.repos_rows_wanted",
                                   return_value=content):
                    commands_frame._reassert_sizes(SOCKET, fid=fid, panes=panes,
                                                   harness_pane=harness_pane,
                                                   window_cols=120, window_rows=50)
                height = _tmux("display-message", "-p", "-t", panes["repos"],
                               "#{pane_height}").stdout.strip()
                self.assertEqual(
                    height, "15",
                    f"the table pane is {height} rows on a plane with {content} clones "
                    f"and a committed `size = 15` — a pinned strip is a constant, and "
                    f"tmux is the only thing that can say whether it really landed")
                harness = _tmux("display-message", "-p", "-t", harness_pane,
                                "#{pane_height}").stdout.strip()
                # 50 - top(1) - bottom(1) - repos(15) - a border for each of the three.
                self.assertEqual(harness, "30",
                                 f"the harness has {harness} rows, so the rows the pin "
                                 f"took came from somewhere the arithmetic does not "
                                 f"describe")

    def test_a_bar_this_plane_placed_does_not_take_the_tables_rows(self):
        """A `[[frame.component]]` table placing `chats`, driven against real tmux — the
        case the hand-written `{"top": "-y", "bottom": "-y", "right": "-x"}` in
        `commands_frame` could not have, because a placed component travels under its own
        id and was in no such literal.

        `layout.slot_sizes` sized the bar and `layout.harness_rows` charged the harness
        for its rows; `_apply_sizes` then issued no `resize-pane` for it at all, so the
        harness's explicit `-y` took those rows out of a neighbour and the one pane
        nothing asserts absorbed the whole error. That remainder is tmux's arithmetic and
        not charter's, which is why it is asked of tmux here rather than of the argv.

        The window is GROWN first, so every pane arrives proportionally scaled — which is
        the state a `window-resized` hook actually fires in and the state in which the
        defect was measured (200x40 -> 200x90 left the bar 7 rows and the six-repo table
        1). Both numbers are asserted, because a bar re-asserted at the wrong height and a
        bar not re-asserted at all are different defects with the same symptom.
        """
        fid = state.frame_id("placed-bar", os.getpid())
        r = _tmux("new-session", "-d", "-s", "placed-bar", "-x", "120", "-y", "40",
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        panes = {}
        for slot, args in (("top", ("-v", "-b", "-l", "1")),
                           ("chats", ("-v", "-b", "-l", "1")),
                           ("bottom", ("-v", "-l", "1")),
                           ("repos", ("-v", "-l", "6"))):
            out = _tmux("split-window", "-t", harness_pane, *args,
                        "-P", "-F", "#{pane_id}", "--", "sleep", "600")
            self.assertEqual(out.returncode, 0, out.stderr)
            panes[slot] = out.stdout.strip()

        placed = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "chats", "edge": "top", "size": 1},
            {"use": "attention"}, {"use": "repos"}, {"use": "sidebar"}]}})
        self.assertIn("chats", placed["slots"],
                      "the arrangement did not place the bar, so nothing below is about "
                      "a placed component")

        self.assertEqual(_tmux("resize-window", "-t", "placed-bar",
                               "-x", "120", "-y", "90").returncode, 0)
        with mock.patch.dict(config.FRAME, placed), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=6):
            commands_frame._reassert_sizes(SOCKET, fid=fid, panes=panes,
                                           harness_pane=harness_pane,
                                           window_cols=120, window_rows=90)

        bar = _tmux("display-message", "-p", "-t", panes["chats"],
                    "#{pane_height}").stdout.strip()
        self.assertEqual(bar, "1",
                         f"the placed bar is {bar} rows where it committed 1 — tmux was "
                         f"never told its height, so it kept whatever the window resize "
                         f"scaled it to")
        table = _tmux("display-message", "-p", "-t", panes["repos"],
                      "#{pane_height}").stdout.strip()
        self.assertEqual(table, "6",
                         f"the table pane is {table} rows on a six-clone plane — it is "
                         f"the stack's dependent pane, so it is where the rows nothing "
                         f"put back are taken from")

    def test_the_table_panes_width_is_tmuxs_answer_and_not_the_recorded_order(self):
        """#510, against the one authority there is.

        `layout.repos_cols` derives the table pane's width from the ORDER the recorded map
        is in, and that order is a JSON file in the frame's own state directory:
        `state.panes` validates the VALUES and says nothing about the order, so a truncated
        write, a hand edit, or a charter that wrote a different shape all arrive as a
        plausible-looking map whose order is fiction. Nothing about that is detectable from
        inside charter — which is the point of asking tmux instead.

        So this builds the `right`-FIRST geometry for real, where the table pane is 87
        columns in a 110-column window, and then hands `_reassert_sizes` the SHIPPED order,
        which claims the same pane is the full 110. The derivation says the table fits (110
        >= `statusline._LEFT_W`) and sizes the pane for six repos; tmux says 87, which draws
        no table at all. One row is the measurement's answer and only the measurement's, so
        this is red on `main` and red on any version that quietly keeps deriving.

        **And the order the two passes run in is what makes tmux's answer true**, which is
        the other half of the fix and is asserted here rather than argued: the window is
        resized first, leaving `right` proportionally scaled to a width it is not supposed
        to have, and `_reassert_sizes` has to put it back before it may believe anything
        about its neighbour. Measured on this binary during development: `right` came back
        62 columns wide after a 120x40 -> 200x40 grow, and the table pane read 137 where
        the truth one `resize-pane -x 22` later is 177.
        """
        fid = state.frame_id("rsz-measured", os.getpid())
        r = _tmux("new-session", "-d", "-s", "rsz-measured", "-x", "120", "-y", "40",
                  "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()

        def _split(*flags: str) -> str:
            p = _tmux("split-window", "-t", harness_pane, *flags,
                      "-P", "-F", "#{pane_id}", "--", "sleep", "600")
            self.assertEqual(p.returncode, 0, p.stderr)
            return p.stdout.strip()

        # `right` FIRST, which is the geometry an operator's own `[frame] slots` order
        # produces and which `instance.frame_of` keeps verbatim.
        right = _split("-h", "-l", "22")
        top = _split("-v", "-b", "-l", "1")
        bottom = _split("-v", "-l", "1")
        table = _split("-v", "-l", "6")
        self.assertEqual(_tmux("resize-window", "-t", "rsz-measured",
                               "-x", "110", "-y", "50").returncode, 0)

        # The map's order is a LIE about this frame: it says the shipped order, in which
        # nothing insets the table. The panes themselves are in the other one.
        lying_order = {"top": top, "bottom": bottom, "repos": table, "right": right}
        self.assertGreaterEqual(
            layout.repos_cols(list(lying_order), window_cols=110),
            statusline._LEFT_W,
            "the recorded order does not claim a table-wide pane — the derivation this "
            "test exists to disagree with is not even being exercised")

        with mock.patch("charter.frame.slots.repos_rows_wanted",
                        side_effect=lambda fid, *, pane_cols:
                            1 if pane_cols < statusline._LEFT_W else 6):
            commands_frame._reassert_sizes(SOCKET, fid=fid, panes=lying_order,
                                           harness_pane=harness_pane,
                                           window_cols=110, window_rows=50)

        self.assertEqual(
            _tmux("display-message", "-p", "-t", right, "#{pane_width}").stdout.strip(),
            "22",
            "the sidebar was not put back to its own width before the pane beside it was "
            "measured — every number after this point is about a geometry that no longer "
            "exists")
        self.assertEqual(
            _tmux("display-message", "-p", "-t", table, "#{pane_width}").stdout.strip(),
            "87",
            "tmux does not agree the table pane is inset — the fixture is not the "
            "geometry this test is about")
        height = _tmux("display-message", "-p", "-t", table,
                       "#{pane_height}").stdout.strip()
        self.assertEqual(height, "1",
                         f"the table pane is {height} rows in a window where its own pane "
                         f"is 87 columns and draws no table — sized from the recorded "
                         f"order, which tmux has just contradicted")

    def test_a_pane_that_cannot_be_measured_still_gets_the_derivations_answer(self):
        """The other side of #510, and the reason `layout.repos_cols` stays. The launcher
        cannot measure a pane that does not exist yet, and a running frame can lose one
        between the map being read and the resize being applied. Same real window, same
        real panes, and a recorded id for the table that names nothing tmux has — the
        derivation answers, and it answers 110, which draws the six-row table.
        """
        fid = state.frame_id("rsz-underived", os.getpid())
        r = _tmux("new-session", "-d", "-s", "rsz-underived", "-x", "110", "-y", "50",
                  "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        top = _tmux("split-window", "-t", harness_pane, "-v", "-b", "-l", "1",
                    "-P", "-F", "#{pane_id}", "--", "sleep", "600").stdout.strip()
        rows_seen: list[int] = []
        with mock.patch("charter.frame.slots.repos_rows_wanted",
                        side_effect=lambda fid, *, pane_cols:
                            rows_seen.append(pane_cols) or 6):
            commands_frame._reassert_sizes(
                SOCKET, fid=fid, panes={"top": top, "repos": "%999"},
                harness_pane=harness_pane, window_cols=110, window_rows=50)
        self.assertEqual(rows_seen, [110],
                         "a pane tmux would not measure did not fall through to "
                         "`layout.repos_cols` — it fell through to something else")

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
            socket=SOCKET, session="sid-one", chat="sid-one")).returncode, 0)
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=SOCKET, session="sid-two", chat="sid-two")).returncode, 0)

        _tmux("run-shell", "-t", "sid-one",
             f"env | grep CHARTER_SESSION_ID > {first_out} 2>&1 || echo NONE > {first_out}")
        _tmux("run-shell", "-t", "sid-two",
             f"env | grep CHARTER_SESSION_ID > {second_out} 2>&1 || echo NONE > {second_out}")
        # Waited for, not slept through (#487) — and for the CONTENT, because the
        # redirection creates each file before it writes it and both branches of the
        # `||` write something, so mere existence is not the readiness condition here.
        self.assertEqual(_await_text(first_out), "CHARTER_SESSION_ID=sid-one",
                         "the OLDER frame's own hotkey menu must still resolve its "
                         "own id after a second frame launches, not whichever "
                         "session tmux would pick as \"current\" by default")
        self.assertEqual(_await_text(second_out), "CHARTER_SESSION_ID=sid-two",
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
    #: Read from the registry rather than spelled out, so retiring a slot (#488 retired
    #: `left`) does not turn this control into a test that fails for the wrong reason.
    _BAD_SLOT_STDERR = (f"charter panel: unknown slot '{_BAD_SLOT}' "
                        f"(known: {', '.join(sorted(frame_slots.SLOTS))})")

    #: How wide the one-row pane below is. Named because the reason's WRAPPED height is
    #: derived from it, and a width written twice would let the two drift.
    _PANE_COLS = 40

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
        a one-row pane's history. Pass `"-S", "-<n>"` to look into that history instead."""
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
        shape `_await_file` uses, so a slow runner is slow rather than wrong.

        **Every test in this class that waits for a panel to come up goes through this,
        and #487's second half is what happens when one does not.** Three of them opened
        with a bare `time.sleep(1)` — a guess at how long a real Python interpreter takes
        to start, import charter and paint one line — and then asserted on the capture.
        Measured on tmux 3.4 pinned to one cpu against twelve spin loops: they failed
        every run, on an EMPTY capture, and raising the module's whole deadline to 90
        seconds changed nothing, because a fixed sleep does not spend a deadline. The
        same panel, given this poll, paints and passes. A sleep is not a shorter wait; it
        is a different thing from a wait."""
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
        into a real ONE-ROW pane (`layout.SLOT_SIZE["bottom"]` — `bottom`'s floor since
        #488, and still the height a plane with no clones gets) with `remain-on-exit`
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
        size `top` always is, and `bottom` is at its floor (`layout.SLOT_SIZE`: 1), the
        operator's
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

        r = _tmux("new-session", "-d", "-s", "panel-oldshape",
                  "-x", str(self._PANE_COLS), "-y", "1",
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
        # One row deeper than the reason's own wrapped height. `-3` today, exactly as it
        # was written by hand — but DERIVED, because the reason names every key of
        # `frame_slots.SLOTS` and therefore grows whenever charter ships a slot. Measured
        # while `changes` was briefly a slot: the sentence went from two wrapped rows to
        # three, this history window stopped reaching the row carrying the slot name, and
        # the case went red for the LENGTH of a message rather than for the property it
        # is named for. That is the failure this arithmetic removes.
        depth = -(-len(self._BAD_SLOT_STDERR) // self._PANE_COLS) + 1
        with_history = self._capture("panel-oldshape", "-S", f"-{depth}")
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

        first = self._wait_for("panel-live", "todo")
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

        first = self._wait_for("panel-hook-live", "todo")
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

        # Wait for the panel to have PAINTED, not for a second to pass: a corrupt
        # version file cannot be a test of a running panel until there is one, and a
        # fixed sleep is a guess at how long an interpreter takes to start (#487).
        drawn = self._wait_for("panel-corrupt", "todo")
        self.assertIn("todo", drawn, f"the panel never drew its content:\n{drawn!r}")
        alive = _tmux("display-message", "-p", "-t", "panel-corrupt", "#{pane_dead}")
        self.assertEqual(alive.returncode, 0, "the panel never even started")
        self.assertEqual(alive.stdout.strip(), "0")

        frame_dir = state.frame_dir(fid, create=True)
        (frame_dir / "version").write_bytes(b"\xff\xfe not valid utf-8 \x80\x81")

        # A NEGATIVE wait — "it did not die" — so it keeps its own short, deliberate
        # number rather than the module deadline. See `_DEADLINE`.
        time.sleep(1)
        after = _tmux("display-message", "-p", "-t", "panel-corrupt", "#{pane_dead}")
        self.assertEqual(after.returncode, 0,
                         "the session vanished — the panel crashed reading the "
                         "corrupt version file, and remain-on-exit is not armed here "
                         f"(stderr: {after.stderr!r})")
        self.assertEqual(after.stdout.strip(), "0",
                         "the panel died reading a corrupt version file")


def _git(repo: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)


def _init_repo(path: Path, branch: str) -> Path:
    """A real git repo with one commit, on *branch* — the same fixture shape
    `tests/test_frame_gather.py`'s own `_init_repo` uses, duplicated here (rather
    than imported across test modules) so this module stays as self-contained as
    every other fixture in it already is. `FourEdgeIntegration` below is the only
    caller: it needs a repo a real `charter panel bottom --session <fid>` subprocess
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
    EVERY slot comes up with every pane alive and drawing REAL content, and
    repaints after a real `state.bump`. Tasks 1-4 were each unit-tested — a mocked
    `scan()`, an in-process cache read, a renderer called directly against a fixed
    `width`. Nothing before this class has ever run `gather.scan`,
    `notify.plane_changed`, `slots.render` and a real tmux pane together, in the
    same process tree, against a real git repo — this is the one place the whole
    composition (gather -> cache -> hook -> panel -> renderer -> a pane an operator
    can actually read) is proven at once.

    `PanelIntegration` above already proves ONE panel end to end (`bottom`, driven
    by a direct `charter panel bottom --session <fid>` new-session). This class
    proves the COMPOSITION `layout.panel_argvs` exists for instead: three real
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

    **Only the repo TABLE is asserted through content that could only have come
    from the cache Tasks 1/2 built.** `top` (workspace/persona/version), `right`
    (persona chips) and `bottom`'s own attention row (todo count/alerts) all read
    live at render time — `slots.py`'s own docstrings for `_top`/`_right`/`_bottom`
    each say so — real subprocess, real data, but none of it through `gather.py`'s
    cache. The table under that row (`slots._table_lines`, #488 — it was `left`
    until then) is the one thing drawn EXCLUSIVELY from `gather.read(fid)` (see its
    own docstring: "never a repo directory listing, a `git status`, or a
    `glstate.read_for` of its own"), so a real repo's real branch name showing up
    in a captured `bottom` pane is the one assertion in this whole file that proves
    the actual thing this plan is for: a real `git` sweep, gathered ONCE, cached to
    disk by a hook, and read back by a panel process that never itself calls git —
    a panel showing "no repos" would be ALIVE and would tell you nothing about
    whether any of that chain actually works (see this module's own task brief).
    That the two halves now share one pane is itself worth the assertion: #488's
    own rule is that the table JOINS the attention row rather than evicting it.

    **Fix round 1 closed three links this proof left unproven, all the same shape
    (green even with the actual mechanism disabled):** a real, uncommitted file in
    the fixture repo (mutation testing found the repo row shows a clean repo as
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
        # This class WANTS the detached child, which is why it declares it rather than
        # stubbing it: `_spawn_frame` runs the real `commands_frame._spawn_gather`, and
        # "the detached `charter frame-gather` really starts, really gathers the workspace
        # it was told to, really writes the cache and really bumps the frame" is the
        # composition this class exists to prove. `tests._planeguard` refuses a detached
        # charter child by default (#542) precisely so the 43 cases that did NOT mean to
        # fork one say so instead of forking it.
        allow_background_children(self)
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

        # The two things `cmd_launch` does to a frame's own state directory before it
        # asks tmux for anything, and #512 is why they are here rather than left out with
        # the hotkey menu and the exit-code hooks. A panel is a PURE CACHE READER since
        # #512 (`slots._bottom` calls `gather.cached`, never `gather.read`), so a frame
        # nobody gathered for shows "gathering …" forever — which is what this test
        # observed the day the fallback went, and correctly: production fills that cache
        # from the launcher, so a test that hand-builds a frame has to do the launcher's
        # job or it is not testing a launch.
        #
        # Both are the REAL production functions, not a hand-rolled `gather.save`:
        # `_spawn_gather` IS #512's fix, and running it here is what makes the repo-row
        # assertions below a proof that the detached child really starts, really gathers
        # the workspace it was told to, really writes the cache, and really bumps the
        # frame — end to end, through a real `charter frame-gather` process, against a
        # real git repo, read back out of a real tmux pane.
        #
        # `os.environ` is patched for exactly that call, and only for it: the child is a
        # separate PROCESS, so `PersonaIso`'s in-process `config.use()` redirection cannot
        # reach it — without this it would resolve the DEVELOPER'S OWN plane and gather
        # (and bump) there. `self.env` is the same isolated environment every pane in this
        # frame already runs under, so the child lands on the same throwaway plane the
        # panels are reading.
        state.record_workspace(fid, "demo")
        with mock.patch.dict(os.environ, self.env, clear=True):
            commands_frame._spawn_gather(fid, "demo")

        session_cmd = layout.session_argv(session=fid, conf=str(conf_path), socket=SOCKET,
                                          cols=120, rows=40, harness_argv=["sleep", "600"])
        r = self._run_env(session_cmd)
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        self.assertTrue(harness_pane, "tmux did not report the harness pane's id")
        self.addCleanup(_kill_pid, self._pane_pid(harness_pane))

        slots = ["top", "bottom", "repos", "right"]
        # `repos` is content-sized since #488 (it was `bottom` until #515 split the
        # attention row off it), and the sizer is asked for the size here rather than
        # left to `SLOT_SIZE`'s floor — a one-row table pane would have no room for the
        # repo rows these tests read back, which is the whole point of the slot.
        # *content_rows* is stated rather than taken from `slots.repos_rows_wanted`: that
        # helper resolves the workspace from THIS process's environment, and every panel
        # below resolves it from the tmux session's instead, so a mismatch would size the
        # pane for a different plane than the one the panels draw.
        panel_cmds = layout.panel_argvs(
            slots=slots, session=fid, socket=SOCKET, harness_pane=harness_pane,
            sizes=layout.slot_sizes(slots, window_rows=40, content_rows=8))
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

    def test_every_panel_comes_up_alive_with_real_content_and_the_harness_keeps_focus(self):
        """Launch composition, proven end to end: a frame with `top`/`bottom`/`repos`/
        `right` all configured comes up with every pane alive, each showing real content
        a broken renderer (or an empty, never-gathered cache) could not have produced,
        and the harness pane — not the last panel `split-window` happened to draw — holds
        keyboard focus once the launch finishes.

        **Fix round 1: the repo is made DIRTY before the frame ever launches, not
        left pristine.** `_init_repo`'s own repo has nothing uncommitted, so
        without this, the repo row shows no marker regardless of whether
        `gather.scan`'s `_repo_states` half (a real `git status --porcelain
        --branch` subprocess) ran at all — a mutation that replaced that whole
        sweep with `{}` left every assertion in this test green, because
        everything asserted before this fix (repo name, branch) comes from
        `_repo_trees`/`_branch` alone, neither of which touches `_repo_states`.
        The `*` dirty marker asserted below is the one thing in this test that
        can only come from that sweep actually running and actually landing in
        the cache the table reads back.

        **#488 moved the repo table from `left` to `bottom`, and #515 moved it again to
        `repos`**, so the cache-proving assertions moved with it. The two are asserted
        against each other rather than each alone: the table pane carries the cached repo
        row and NOT the attention row, and the `bottom` pane carries the live todo count
        and NOT a repo row. Either assertion alone would pass against a renderer that
        drew both things in both panes, which is what a half-applied split looks like on
        screen — the operator's report in #515 with a rule drawn through it.
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
        self.assertEqual(set(panes), {"top", "bottom", "repos", "right"})

        # Poll for real content (`_wait_for`'s own docstring — fix round 1:
        # replaces a fixed `time.sleep(1.5)` that guessed at how long four cold
        # `charter` imports plus four `gather.scan` git sweeps take). This also
        # does the flat sleep's OTHER job — giving a genuine startup crash time
        # to register — for free: a dead pane never satisfies its needle, so
        # `_wait_for` spends its own timeout before falling through to the alive
        # check below, same as a deliberate wait would have.
        top = self._wait_for(panes["top"], "demo")
        right = self._wait_for(panes["right"], persona_name)
        table = self._wait_for(panes["repos"], repo_name)
        bottom = self._wait_for(panes["bottom"], "1 todo")

        for slot, pane in panes.items():
            self.assertEqual(self._alive(pane), "0",
                             f"the {slot!r} panel died at startup — a hole in the "
                             f"frame, not a degraded row (the launcher-days-of-a-"
                             f"green-suite gap `PanelIntegration`'s own docstring "
                             f"names, now for all four slots at once)")

        self.assertIn("demo", top, f"top never showed the real workspace name:\n{top!r}")

        self.assertIn(repo_name, table,
                      f"the table pane never showed the real repo:\n{table!r}")
        self.assertIn(branch, table,
                      f"the table pane never showed the real branch:\n{table!r}")
        self.assertIn("*", table,
                      f"the table pane never showed the dirty marker for a real "
                      f"uncommitted file — either gather.scan's own git-status sweep "
                      f"never ran, or nothing carried its result into the cached "
                      f"row:\n{table!r}")

        self.assertIn(persona_name, right,
                      f"right never showed the real persona:\n{right!r}")

        self.assertIn("1 todo", bottom,
                      f"the attention strip never showed the workspace's todo "
                      f"count:\n{bottom!r}")
        # #515's own property, and the half a membership check cannot give: the two
        # things the operator reported as one undivided pane are now in two panes, and
        # NEITHER draws the other's content. A renderer that split the panes but kept
        # composing both halves in each would satisfy every assertion above.
        self.assertNotIn("todo", table,
                         f"the table pane is still drawing the attention row:\n{table!r}")
        self.assertNotIn(repo_name, bottom,
                         f"the attention strip is still drawing the repo "
                         f"table:\n{bottom!r}")

        focus = _tmux("display-message", "-p", "-t", harness_pane,
                      "#{pane_active}").stdout.strip()
        self.assertEqual(focus, "1",
                         "the harness pane lost focus to the last panel drawn — an "
                         "operator's harness must be able to receive a keystroke "
                         "the instant the frame comes up")

    def test_the_resize_hook_really_fires_and_charter_really_recomputes(self):
        """The one link `_reassert_sizes`' own integration test (`TmuxIntegration.
        test_recomputed_sizes_really_hold_across_real_window_resizes`) cannot reach: that
        one CALLS the recompute, and proves the sizes hold. This proves tmux actually
        runs the hook charter installed, and that the `run-shell` child actually finds
        this frame and resizes its panes — the whole of #488's answer to "a content-sized
        pane must recompute its HEIGHT on `window-resized`, not just its width".

        A real `set-hook` built by `commands_frame._resize_hook_argv`, a real
        `resize-window`, and a real `charter frame-resize` subprocess started by tmux
        against this class's own throwaway plane (the frame's harness pane, pane map and
        server are recorded first, because that child reads all three off disk — which is
        exactly why no pane id needs to travel in the hook's text, closing #475).

        **Asserted on a GROW, and that is what makes it a test rather than a
        coincidence.** On a shrink tmux's own proportional redistribution already makes
        `bottom` smaller all by itself, so "it got smaller" would pass with no hook
        installed at all — the fixture-coincidence shape this suite keeps paying for.
        Growing the window is the one direction the two mechanisms disagree about: tmux
        stretches every pane proportionally (measured on 3.7c: a one-row panel became 8
        rows on a 120x30 -> 200x50 grow), while charter's recompute sizes `bottom` to its
        CONTENT, and this frame's plane has no clones to table — so a pane that came out
        SMALLER after the window grew can only be charter's answer.
        """
        fid = state.frame_id("four-edge-resize", os.getpid())
        harness_pane, panes = self._spawn_frame(fid)
        state.record_harness_pane(fid, harness_pane)
        state.record_panes(fid, panels=panes)
        state.record_server(fid, SOCKET)

        hook = commands_frame._resize_hook_argv(socket=SOCKET,
                                                harness_pane=harness_pane, fid=fid)
        self.assertIsNotNone(hook, "the frame's own id was refused by the hook builder")
        self.assertEqual(self._run_env(hook).returncode, 0,
                         "installing the resize hook failed")
        # The `run-shell` child is a `charter` of its own: it must find THIS plane, not
        # the developer's. `_spawn_frame`'s session already carries `$CHARTER_ROOT` (see
        # `setUp`), and the hook fires in that session's own environment.
        before = int(_tmux("display-message", "-p", "-t", panes["repos"],
                           "#{pane_height}").stdout.strip())
        self.assertGreater(before, 1, "the fixture never gave the table a tall pane")

        r = _tmux("resize-window", "-t", fid, "-x", "160", "-y", "80")
        self.assertEqual(r.returncode, 0, r.stderr)

        deadline = time.monotonic() + _DEADLINE
        height = before
        while time.monotonic() < deadline:
            height = int(_tmux("display-message", "-p", "-t", panes["repos"],
                               "#{pane_height}").stdout.strip() or before)
            if height < before:
                break
            time.sleep(0.1)
        self.assertLess(height, before,
                        f"the table is {height} rows after the window GREW from 40 to "
                        f"80 — tmux's own redistribution only ever stretches on a grow, "
                        f"so this is what charter's recompute would have had to undo. "
                        f"The hook never fired, or its child never reached this frame")
        # And the two one-row strips really came back to one row. #515 is what makes
        # this worth asserting here: the recompute now has three horizontal strips to
        # reconcile and tmux's `resize-pane -y` moves only one boundary, so a recompute
        # that named the strips and not the harness left two of them trading rows with
        # each other — the table one row tall and the attention strip six, measured.
        for slot in ("top", "bottom"):
            got = _tmux("display-message", "-p", "-t", panes[slot],
                        "#{pane_height}").stdout.strip()
            self.assertEqual(got, "1",
                             f"the {slot} strip is {got} rows after the recompute")
        harness = int(_tmux("display-message", "-p", "-t", harness_pane,
                            "#{pane_height}").stdout.strip())
        self.assertGreaterEqual(harness, layout.HARNESS_MIN_ROWS, harness)

    def test_a_real_drag_across_the_table_width_removes_the_pane_and_brings_it_back(self):
        """#536, end to end: a real `window-resized` hook, a real `resize-window`, a real
        `charter frame-resize` child, and a pane that actually leaves the window and
        actually comes back.

        Which slots a frame HAD was decided once, at launch — `_drawable_slots` ran against
        the terminal the frame started in and nothing re-ran it except a density change. So
        a frame launched at 120 columns and narrowed to 80 kept a `repos` pane whose
        renderer refuses to draw a table below `statusline._LEFT_W`, leaving a bordered
        rectangle saying so; and a frame narrowed and then WIDENED never got it back at
        all, because nothing could un-refuse a slot that had already been dropped.

        Asserted as a pane COUNT and a pane's presence in the window, not as a size:
        #500/#488 already prove the sizes hold, and every one of those assertions passes
        just as well against the pane that could not be un-split. The only thing that
        distinguishes this fix is that the rectangle is gone.

        **Both directions, and the second is why one is not enough.** A resize that only
        ever removed would pass "the pane is gone"; a resize that only ever added would
        pass "the pane came back". The frame has to end this test with exactly the panes a
        launch at its final size would have drawn.
        """
        fid = state.frame_id("four-edge-shape", os.getpid())
        harness_pane, panes = self._spawn_frame(fid)
        state.record_harness_pane(fid, harness_pane)
        state.record_panes(fid, panels=panes)
        state.record_server(fid, SOCKET)

        hook = commands_frame._resize_hook_argv(socket=SOCKET,
                                                harness_pane=harness_pane, fid=fid)
        self.assertIsNotNone(hook, "the frame's own id was refused by the hook builder")
        self.assertEqual(self._run_env(hook).returncode, 0,
                         "installing the resize hook failed")

        def _panes_now() -> set[str]:
            out = _tmux("list-panes", "-t", fid, "-F", "#{pane_id}").stdout.split()
            return set(out)

        def _await_shape(slots: list[str], what: str) -> set[str]:
            """Wait for the frame's own RECORD to name *slots*, then read the window.

            The record is what `cmd_resize` rewrites LAST (`_apply_arrangement`: relayout,
            record, bump), so it is the only readiness condition that means the child has
            finished rather than that its panes happen to exist yet — polling the window
            alone caught a re-layout mid-flight, with five panes up and the map still
            naming two.
            """
            deadline = time.monotonic() + _DEADLINE
            while time.monotonic() < deadline and sorted(state.panes(fid)) != sorted(slots):
                time.sleep(0.1)
            self.assertEqual(sorted(state.panes(fid)), sorted(slots),
                             f"{what}: the frame's own record names "
                             f"{sorted(state.panes(fid))}")
            seen = _panes_now()
            self.assertEqual(len(seen), len(slots) + 1,
                             f"{what}: the record says {sorted(slots)} but the window "
                             f"holds {sorted(seen)} panes counting the harness")
            return seen

        self.assertEqual(len(_panes_now()), 5,
                         "the fixture did not come up with a harness and four panels")

        # Narrow past both boundaries at once: 80 is under `[frame] min-cols` (which drops
        # `right`) and under `statusline._LEFT_W` (which drops `repos`).
        self.assertEqual(_tmux("resize-window", "-t", fid,
                               "-x", "80", "-y", "40").returncode, 0)
        left = _await_shape(["top", "bottom"],
                            "a frame narrowed to 80 columns still has the two panes a "
                            "launch at 80 columns would have refused to draw")
        self.assertNotIn(panes["repos"], left)
        self.assertNotIn(panes["right"], left)

        # And back. The panes that come back are NEW ones — `_relayout` splits rather than
        # un-kills — so this is asserted on the recorded map and the window's own count,
        # never on the old ids.
        self.assertEqual(_tmux("resize-window", "-t", fid,
                               "-x", "160", "-y", "40").returncode, 0)
        back = _await_shape(["top", "bottom", "repos", "right"],
                            "a frame widened back to 160 columns never regained the "
                            "panes it now has room for")
        for pane in back:
            self.addCleanup(_kill_pid, self._pane_pid(pane))

    def test_a_state_bump_through_the_real_hook_repaints_the_table_and_the_alert_row(self):
        """Closes the gap `PanelIntegration`'s own hook test
        (`test_a_real_hook_call_repaints_a_live_panel_without_a_direct_state_bump`)
        leaves open for THIS plan: that test drives `hooks.posttooluse` against
        `bottom` alone, which never touches `gather.py` at all. This drives the
        SAME real hook — never a direct `state.bump` or `gather.refresh` call — and
        watches the `repos` pane's table repaint with a NEW branch name that only
        exists because
        `notify.plane_changed` ran `gather.refresh` BEFORE `state.bump` (Task 2's
        own contract, and its own docstring's reason: refresh-then-bump closes the
        window where a poller sees the new version and still reads the stale
        cache). A version bump into a stale or never-refreshed cache would leave
        the table showing the OLD branch forever — this is the one test in the file
        that would catch that. `bottom` — a different PANE since #515, drawing the live
        todo count — is watched in the same pass, from the SAME single hook call, to pin
        that one refresh/bump serves every slot that asks, not only the one
        `PanelIntegration` already covers.

        **The cache is warmed with one direct `gather.refresh` call before any
        assertion runs — this is load-bearing, not incidental.** Caught by
        mutation: with no cache file on disk yet, `gather.read`'s OWN fallback
        (`_table_lines`' caller calls it) recomputes a FRESH
        scan on every call regardless of whether the cache was ever written — so
        with `notify.plane_changed`'s `gather.refresh` call deleted outright (a
        real mutation tried while writing this test), the table still showed the new
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
        # not only that the table shows the right text, which `gather.read`'s own
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

        table_before = self._wait_for(panes["repos"], branch_a)
        bottom_before = self._wait_for(panes["bottom"], "1 todo")
        self.assertIn(branch_a, table_before,
                      f"the table never showed the starting branch:\n{table_before!r}")
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
        # the WRONG, repo-less workspace, and the table would repaint to "no repos"
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
        # The two facts land on two panes since #515, and each is polled for its own
        # needle — one refresh and one bump have to serve BOTH panels, which is the
        # property this test is for and which a single-pane assertion can no longer
        # cover.
        table_after = self._wait_for(panes["repos"], branch_b)
        bottom_after = self._wait_for(panes["bottom"], "2 todos")

        self.assertNotEqual(table_after, table_before,
                            f"the table never repainted after a real "
                            f"hooks.posttooluse() call; still showing:"
                            f"\n{table_before!r}")
        self.assertIn(branch_b, table_after,
                      f"the table repainted but not with the NEW "
                      f"branch:\n{table_after!r}")
        self.assertNotIn(branch_a, table_after,
                         f"the table kept showing the OLD branch after the switch — a "
                         f"stale cache surviving its own refresh:\n{table_after!r}")
        self.assertIn("2 todos", bottom_after,
                      f"the live pane never repainted off the same bump:"
                      f"\n{bottom_after!r}")

        # Fix round 1: the cache FILE on disk, not only the pane's own capture,
        # now holds the new branch — proving the hook's `gather.refresh` call did
        # the WRITE `notify.plane_changed` is supposed to perform, the same
        # direct check made against `branch_a` above, repeated here against the
        # value only a SECOND, successful refresh could have produced.
        self.assertIn(branch_b, cache.read_text(),
                      f"bottom repainted (or the panel's own live fallback masked "
                      f"a broken refresh) but the cache file itself was never "
                      f"updated with the new branch:\n{cache.read_text()!r}")

        for slot, pane in panes.items():
            self.assertEqual(self._alive(pane), "0",
                             f"the {slot!r} panel died sometime after repainting")


class TheTablesWidthIsWhatTmuxActuallyGivesIt(_TmuxServerFixture, PersonaIso):
    """#500 round 3: `layout.repos_cols` says how wide the `repos` pane comes out.
    tmux is the only authority on that, so this asks tmux.

    The unit tests in `tests/test_frame_layout.py` pin the arithmetic against a number
    written down by a human who once ran tmux. That is exactly the shape of assertion
    this repo has been burned by — a constant that was right the day it was measured and
    is never re-checked. What is actually being claimed is that a `right` split off the
    harness pane BEFORE `repos` costs it the sidebar's columns plus one border column,
    and the only thing that can confirm it is `#{pane_width}` off a real server.

    **The splits are `layout.panel_argvs`' own commands, not hand-retyped ones** — the
    direction (`-h`/`-v`), the `-b`, and the `-l` are the production values, because
    those flags ARE the geometry under test. Only the program after `--` is swapped for
    a `sleep`, so this class needs no importable plane, no `charter panel` process and
    no repaint: it is about rectangles, and `FourEdgeIntegration` above is where real
    panels are proven.
    """

    def _table_width(self, order: list[str], cols: int) -> int:
        session = f"bw{self._pane_counter}"
        self._pane_counter += 1
        r = self._srv("new-session", "-d", "-s", session,
                      "-x", str(cols), "-y", "40", "-P", "-F", "#{pane_id}",
                      "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        self.addCleanup(self._srv, "kill-session", "-t", session)

        sizes = layout.slot_sizes(order, window_rows=40, content_rows=7)
        cmds = layout.panel_argvs(slots=order, session=session,
                                  socket=self.SOCKET_NAME,
                                  harness_pane=harness_pane, sizes=sizes)
        widths: dict[str, int] = {}
        for slot, cmd in zip(order, cmds):
            argv = cmd[:cmd.index("--") + 1] + ["sleep", "600"]
            p = subprocess.run(argv, capture_output=True, text=True, timeout=10)
            self.assertEqual(p.returncode, 0, f"splitting {slot!r}: {p.stderr}")
            pane = p.stdout.strip()
            self.addCleanup(_kill_pid, self._pane_pid_on(pane))
            widths[slot] = pane
        got = self._srv("display-message", "-p", "-t", widths["repos"],
                        "#{pane_width}").stdout.strip()
        return int(got)

    def _pane_pid_on(self, pane: str) -> str:
        return self._srv("display-message", "-p", "-t", pane,
                         "#{pane_pid}").stdout.strip()

    def test_tmux_agrees_with_the_arithmetic_in_both_slot_orders(self):
        """Both orders in one test, because the claim is a DIFFERENCE: the shipped order
        leaves the table the whole window and an operator's `right`-first order does not.
        Asserted against `layout.repos_cols`' own answer rather than against 110 and 87,
        so this is tmux checking charter's arithmetic rather than two literals agreeing
        with each other — and the second assertion is what stops "always the window's
        width" from passing.
        """
        orders = (["top", "bottom", "repos", "right"],
                  ["right", "top", "bottom", "repos"],
                  ["top", "right", "bottom", "repos"],
                  ["repos", "right"])
        for order in orders:
            with self.subTest(order=order):
                self.assertEqual(self._table_width(order, 110),
                                 layout.repos_cols(order, window_cols=110),
                                 f"tmux disagrees with layout.repos_cols for {order}")
        self.assertNotEqual(
            layout.repos_cols(orders[0], window_cols=110),
            layout.repos_cols(orders[1], window_cols=110),
            "the two orders answered the same width — the loop above proved nothing")

    def test_tmux_stacks_the_strips_in_the_order_the_shipped_slots_ask_for(self):
        """#515's own geometry, asked of tmux rather than reasoned about. Every split but
        `top`'s is a plain `-v` off the HARNESS pane, which tmux places DIRECTLY below it
        — so a slot split later sits ABOVE one split earlier, and the shipped list reads
        backwards on screen for everything under the harness.

        That is not a detail anybody can hold in their head, and getting it wrong ships
        the pre-#515 stacking order with a rule drawn through it: the status strip
        squeezed between the session and the table instead of anchored to the terminal's
        last row. So the panes' own `#{pane_top}` is read back and the reading order
        asserted: identity, harness, table, attention.
        """
        session = f"st{self._pane_counter}"
        self._pane_counter += 1
        r = self._srv("new-session", "-d", "-s", session, "-x", "120", "-y", "40",
                      "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness_pane = r.stdout.strip()
        self.addCleanup(self._srv, "kill-session", "-t", session)

        order = list(config.FRAME["slots"])
        sizes = layout.slot_sizes(order, window_rows=40, content_rows=6)
        panes = {"harness": harness_pane}
        for slot, cmd in zip(order, layout.panel_argvs(
                slots=order, session=session, socket=self.SOCKET_NAME,
                harness_pane=harness_pane, sizes=sizes)):
            argv = cmd[:cmd.index("--") + 1] + ["sleep", "600"]
            p = subprocess.run(argv, capture_output=True, text=True, timeout=10)
            self.assertEqual(p.returncode, 0, f"splitting {slot!r}: {p.stderr}")
            panes[slot] = p.stdout.strip()
            self.addCleanup(_kill_pid, self._pane_pid_on(panes[slot]))

        def _top_row(pane):
            return int(self._srv("display-message", "-p", "-t", pane,
                                 "#{pane_top}").stdout.strip())

        rows = {name: _top_row(pane) for name, pane in panes.items()}
        self.assertEqual(
            [n for n in sorted(rows, key=rows.get) if n != "right"],
            ["top", "harness", "repos", "bottom"], rows)
        # And the attention strip really is the window's last row, which is the anchor
        # #515 chose it for: the table above it grows and shrinks with the repo count
        # while this one never moves.
        self.assertEqual(rows["bottom"], 40 - 1, rows)


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class EarlyDeathIntegration(_VoidDeaths, unittest.TestCase):
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

        The conf carries ONE line the launcher's own does not: the array probe, armed
        GLOBALLY — see `_a_measured_death` for what it is for, and why it cannot be armed
        the way every other class here arms it.
        """
        self._n += 1
        name = f"d{self._n}"
        conf = self._conf_dir / f"{name}.conf"
        conf.write_text(commands_frame._PLACEHOLDER_CONF
                        + f"set-hook -g pane-died[{self._PROBE_INDEX}] "
                          f"'{self._probe_action(self._ON_THE_PANE)}'\n")
        _tmux("set", "-g", "remain-on-exit", "on")  # no-op (and an error) before the
                                                    # first session; `-f` covers that one
        r = _run(layout.session_argv(session=name, conf=str(conf), socket=SOCKET,
                                     cols=80, rows=24, harness_argv=list(harness_argv)))
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self.assertTrue(pane, "tmux reported no pane id for the new session")
        return pane, _await_dead(pane)

    def _a_measured_death(self, *harness_argv: str) -> tuple[str, int | None]:
        """`_die`, but only ever returning a death tmux actually had the child's status
        for — #507.

        **This is #487's mechanism reaching a class that had no defence against it.** On a
        loaded tmux 3.4 roughly one death in seventeen closes the pane's fd before tmux
        has the child's status and never gets it: `#{pane_dead}` `1`,
        `#{pane_dead_status}` EMPTY, the pane's `pane-died` array never run, permanently.
        `commands_frame._pane_state` folds that empty status to `_UNKNOWN_DEATH_CODE`, so
        `_die` hands back `1` — and `test_a_lone_argument_reaches_a_shell_not_execvp`
        compares it with `7` and fails, saying "a single argument must reach a shell"
        about a machine on which the argument reached a shell perfectly well. That is
        exactly what CI showed: red on Python 3.11 and 3.13 in one run, green on a re-run
        of identical bytes, six distinct tests in this module in one day.

        **The probe is armed in the server's own CONFIG FILE, and it has to be.** Every
        other class here arms `pane-died` on a pane it already holds, after a gate the
        test opens when it wants the death. There is no gate here: the whole subject is a
        command that dies BEFORE the frame is drawn, so the pane is often dead before
        `new-session` has even printed its id, and a hook installed afterwards could never
        have fired for it. `tmux -f <conf>` is read at server start, before any pane
        exists — measured against tmux 3.7c: a pane running `exit 7` that dies instantly
        still sets the option, and the global hook stays armed for the second and later
        sessions on the same server.

        **A void is asserted, never assumed** (`_the_array_never_ran`): a pane tmux
        DESTROYED rather than kept, and a pane tmux holds a status for but ran no hook
        for, are both real defects and both say which one they are. So the only reading
        that spends another pane is the one that was actually measured — and after
        `_HOOK_TRIALS` of them the test skips, naming the capability it could not get,
        rather than asserting on a number tmux never had.
        """
        for _ in range(self._HOOK_TRIALS):
            pane, code = self._die(*harness_argv)
            if self._the_array_ran(pane):
                return pane, code
            self._the_array_never_ran(
                pane, "tmux never ran this pane's `pane-died` array")
        self._no_death_was_delivered()

    def test_a_lone_argument_reaches_a_shell_not_execvp(self):
        """Half of the contract `charter frame --` is not allowed to narrow. `exit 7` is
        not a program on any machine — it is shell syntax, and it comes back as 7 only if
        tmux ran it through a shell. A `shutil.which(argv[0])` pre-check would refuse
        this text outright (it is not even one word), which is why #384's fix reports
        after the fact instead of predicting beforehand.

        Through `_a_measured_death`, not `_die`: the number this asserts on is the one a
        void death does not have (#507). Nothing is widened — `7` is still the only value
        that passes."""
        pane, code = self._a_measured_death("exit 7")
        self.assertEqual(code, 7,
                         "a single argument must reach a shell — `charter frame -- "
                         "'ulimit -n; exit 3'` and every other one-liner depend on it")

    def test_two_or_more_arguments_are_exec_d_directly_with_no_shell(self):
        """The other half, and what makes charter's own `$PATH` note SOUND rather than a
        guess: split into two arguments, the same text is `execvp`'d, so `exit` is looked
        up as a program, is not found, and cannot possibly have run. A word that resolves
        to nothing in THIS form provably never executed — which is the one condition
        under which `_could_not_have_run` is allowed to speak.

        `_a_measured_death` here for a reason the assertions do not show: this test's
        `assertNotEqual(code, 7)` would PASS on a void, whose `_UNKNOWN_DEATH_CODE` is
        `1` — a green that measured nothing at all, which is worse than the red its
        sibling gets."""
        pane, code = self._a_measured_death("exit", "7")
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
        itself comes back, which every one of them names.

        `_a_measured_death` for its `code`, which reaches `early_death_message` and is
        asserted to be non-zero: a void's `_UNKNOWN_DEATH_CODE` is `1`, so that assertion
        too would pass on a death nothing was learned from (#507)."""
        pane, code = self._a_measured_death(self.MISSING)
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
        can be pinned exactly, in `tests/test_frame_launcher.py::EarlyDeathIsLegible`.

        `_a_measured_death` for the same reason its siblings use it: `assertIn(str(code),
        msg)` compares the message against whatever number `_pane_state` produced, and on
        a void that number is `_UNKNOWN_DEATH_CODE` rather than the child's (#507)."""
        pane, code = self._a_measured_death(self.MISSING, "--flag")
        self.assertIsNotNone(code, "the pane should have died at once")
        self.assertNotEqual(code, 0)
        words = commands_frame._pane_last_words(SOCKET, pane)
        msg = commands_frame.early_death_message([self.MISSING, "--flag"], code, words)
        self.assertIn(self.MISSING, msg)
        self.assertIn(str(code), msg)


#: A SECOND real server, standing in for the tmux the OPERATOR already had open.
#:
#: **Separate from `SOCKET`, and that separation is #408's second half.**
#: `WindowInsideAnOperatorsTmux` used to run against `SOCKET` reached by PATH — the same
#: server every other class here uses, under another name. `_TmuxServerFixture._new_pane`
#: had already run `set -g remain-on-exit on` on it, so the "operator's" tmux arrived
#: pre-configured the way only charter's own private server ever is, and a test asking
#: whether charter makes a dead PANEL reachable was answered by the fixture. It stayed
#: green with the production call deleted. This one is never configured by anything: what
#: it does with a dying pane is tmux's own default, which is what an operator's tmux is.
OP_SOCKET = _tmuxreap.name("integration-operator")

#: The socket FILE tmux computes for `-L OP_SOCKET` — the same path `_teardown_socket`
#: already has to know. Needed as a path (not a name) by `WindowInsideAnOperatorsTmux`,
#: because the whole point of that class is exercising the `-S <socket path>` half of
#: `tmuxctl.server_argv`, which is how charter reaches a server it did not start.
OP_SOCKET_PATH = str(Path("/tmp") / f"tmux-{os.getuid()}" / OP_SOCKET)


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

    **The server is `OP_SOCKET`, not `SOCKET` reached by path, and nothing configures
    it.** Those are the two halves of the same requirement and this class had neither
    until #408's second round: it ran against the same server as every other class here,
    which `_new_pane` had already given `set -g remain-on-exit on` — so a pane that
    charter had done nothing to still kept its corpse and still fired `pane-died`, and
    the test below could not tell charter arming a panel from the fixture having armed
    the whole server. See `OP_SOCKET` and `ARMS_REMAIN_ON_EXIT` for the measurement.
    """

    SOCKET_NAME = OP_SOCKET
    ARMS_REMAIN_ON_EXIT = False

    def _operator_server(self, harness_dies_by="exit 0"):
        """A session standing in for one the operator already had open.

        Returns its name, its `$N` session id, its own pane id, and the gate the frame's
        harness will die by. The session's own pane runs a program that never exits on
        its own, so anything that quietly takes the operator's session down shows up as
        a missing session rather than as a race.

        **The precondition is checked, not assumed, and that is the whole of #408's
        second round.** "This is somebody else's tmux" is a claim about the SERVER'S
        STATE, not about which constant names its socket — a separate socket that
        something has already armed is charter's own server again, and a shared one that
        nothing has armed is not. So the property is asserted here, on every test in the
        class, against tmux's own answer: `remain-on-exit` is at the default an operator
        would have. Every test below whose subject is a dying pane is measuring charter
        only while this holds.
        """
        name, pane, gate = self._new_pane("exit 0")
        self.assertEqual(
            self._srv("show-options", "-g", "remain-on-exit").stdout.strip(),
            "remain-on-exit off",
            "the server standing in for the operator's tmux is not at tmux's default — "
            "something armed `remain-on-exit` on it before charter got there, and every "
            "test in this class about a pane surviving its own death is now measuring "
            "that instead of measuring charter (see `ARMS_REMAIN_ON_EXIT`)")
        sid = self._srv("display-message", "-p", "-t", pane, "#{session_id}").stdout.strip()
        self.assertTrue(sid.startswith("$"), sid)
        return name, sid, pane, gate

    def _open_frame_window(self, sid, fid="charter-demo-1"):
        """`layout.window_argv`'s own bytes, run for real. Returns (window id, pane id)."""
        r = _run(layout.window_argv(socket=OP_SOCKET_PATH, session=sid, window=fid,
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
        r = _run(commands_frame._remain_on_exit_argv(socket=OP_SOCKET_PATH,
                                                     harness_pane=pane_id))
        if r.returncode != 0:
            self.skipTest("this tmux does not accept a pane-scoped `remain-on-exit` "
                          f"({r.stderr.strip()}), so it cannot hold a dead harness pane "
                          "for its exit status to be read out of")

    def test_the_harnesss_real_exit_code_survives_in_someone_elses_server(self):
        """End to end for the one property the whole module exists for, on the path
        where there is no `attach` to read a code out of: charter opens the window,
        arms the pane, respawns the harness into it, and reads the status back through
        `_pane_state` — the same function the launcher's wait loop calls.

        **#487 was this test, and the ticket's own reading of it was wrong.** It failed
        on CI as `('dead', 1) != ('dead', 33)`, which the ticket read as "the spawned
        command failed before it reached the exit the test is about". It did not: `1`
        here is `commands_frame._UNKNOWN_DEATH_CODE`, which `_pane_state` reports for a
        pane tmux calls dead and holds NO status for. Reproduced on tmux 3.4 pinned to
        one cpu against twelve spin loops, running this exact path 118 times: 111 answered
        `1:33`, and the 7 that failed all answered `1:` — an empty status, unchanged
        after a further 8 seconds, with the pane's `pane-died` array never run. Not one
        trial ever reported a literal status of `1`, so nothing exited 1 and no readiness
        condition was being assumed.

        So the wait is on the CONSTANT-ACTION PROBE HOOK rather than on `#{pane_dead}`:
        tmux runs a pane's hook array once it has the child's status, which makes the
        probe's marker a synchronisation point where the dead flag is not. A trial whose
        array never ran measured nothing about charter and is spent again on a fresh
        window (`_HOOK_TRIALS`); every other shape of missing status fails, and
        `_the_array_never_ran` asserts the void rather than assuming it. The assertion
        itself is untouched — `(_DEAD, 33)`, the property the module exists for, with a
        harness that died by signal or with a lost code still landing on `(_DEAD, 1)` and
        still going red."""
        _, sid, _, _ = self._operator_server()
        for attempt in range(self._HOOK_TRIALS):
            window_id, pane_id = self._open_frame_window(sid, fid=f"charter-demo-{attempt}")
            self._require_pane_options(pane_id)
            gate = os.path.join(self._gate_dir, f"harness-gate-{attempt}")
            r = _run(layout.respawn_argv(socket=OP_SOCKET_PATH, harness_pane=pane_id,
                                         env={}, cwd=self._gate_dir,
                                         harness_argv=_gate_argv(gate, "exit 33")))
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertEqual(commands_frame._pane_state(OP_SOCKET_PATH, pane_id),
                             (commands_frame._ALIVE, None))
            self._arm_array_probe(pane_id)
            self._release(gate)
            if not self._the_array_ran(pane_id):
                self._the_array_never_ran(
                    pane_id, "tmux never ran the harness pane's `pane-died` array")
                _run(tmuxctl.server_argv(OP_SOCKET_PATH, "kill-window", "-t", window_id))
                continue
            self.assertEqual(commands_frame._pane_state(OP_SOCKET_PATH, pane_id),
                             (commands_frame._DEAD, 33))
            return
        self._no_death_was_delivered()

    def test_the_placeholder_does_not_exit_on_its_own(self):
        """`layout.PLACEHOLDER`'s only required property, and the one an argv assertion
        cannot make: if it could exit, the window would be gone before `remain-on-exit`
        was ever set on its pane and the ordering the whole path rests on would buy
        nothing."""
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        time.sleep(0.4)
        self.assertEqual(commands_frame._pane_state(OP_SOCKET_PATH, pane_id)[0],
                         commands_frame._ALIVE,
                         "the placeholder exited on its own")

    def test_a_pane_that_is_gone_answers_empty_rather_than_failing(self):
        """The measured fact `_pane_state` — and therefore the launcher's wait loop —
        rests on: `display-message -p -t <a pane that no longer exists>` does NOT fail.
        It returns 0 and prints an empty line. A loop that only stopped on
        `#{pane_dead}` being `1` would poll a window nobody can bring back, forever."""
        _, sid, _, _ = self._operator_server()
        window_id, pane_id = self._open_frame_window(sid)
        raw = _run(tmuxctl.server_argv(OP_SOCKET_PATH, "display-message", "-p", "-t",
                                       pane_id, commands_frame._DEAD_FORMAT))
        self.assertEqual(raw.returncode, 0)
        self.assertEqual(raw.stdout.strip(), "0:")
        _run(tmuxctl.server_argv(OP_SOCKET_PATH, "kill-window", "-t", window_id))
        gone = _run(tmuxctl.server_argv(OP_SOCKET_PATH, "display-message", "-p", "-t",
                                        pane_id, commands_frame._DEAD_FORMAT))
        self.assertEqual(gone.returncode, 0,
                         "tmux is expected to answer, not to refuse")
        self.assertEqual(gone.stdout.strip(), ":",
                         "both variables expand to nothing, and the format's own "
                         "literal `:` is all that is left — NOT an empty line, which "
                         "is what a guard written from memory assumed")
        self.assertEqual(commands_frame._pane_state(OP_SOCKET_PATH, pane_id),
                         (commands_frame._GONE, None))

    def test_closing_the_frames_window_leaves_the_operators_session_alone(self):
        """`kill-window`, never `kill-session` — the difference between charter tidying
        up after itself and charter ending every window the operator had open."""
        name, sid, op_pane, _ = self._operator_server()
        window_id, _ = self._open_frame_window(sid)
        before = self._srv("list-windows", "-a", "-F", "#{window_name}").stdout.split()
        self.assertIn("charter-demo-1", before)
        _run(tmuxctl.server_argv(OP_SOCKET_PATH, "kill-window", "-t", window_id))
        after = self._srv("list-windows", "-a", "-F", "#{window_name}").stdout.split()
        self.assertNotIn("charter-demo-1", after)
        self.assertIn(name, self._srv("list-sessions", "-F", "#{session_name}").stdout.split(),
                      "the operator's own session went with charter's window")
        self.assertEqual(self._srv("display-message", "-p", "-t", op_pane,
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
            socket=OP_SOCKET_PATH, harness_pane=pane_id, env=env, cwd=self._gate_dir,
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
            socket=OP_SOCKET_PATH, harness_pane=pane_id,
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

    def _panel_funnel(self, harness_pane, slots=()):
        """Run charter's OWN panel funnel against this server — the whole of what a
        launch (and every density change) does around a `split-window`.

        `slots=()` by default because a pane these tests can use has to be able to DIE ON
        COMMAND and `charter panel` cannot: what they need from the funnel is the state
        it leaves charter's WINDOW in before any panel is split, which is the half that
        was missing. It is a shape the launcher really passes, not one invented here —
        `_drawable_slots` answers `[]` below its size floors and `cmd_launch` calls
        straight through with it.

        **Two calls since #686, and that is exactly what these tests are for.** The window
        half moved out of `_split_panels` (`_dress_window`) so that a re-layout adding no
        pane still asserts it — and with `slots=()` this fixture adds no pane either, so
        driving the split alone would leave charter's own window at tmux's default
        `remain-on-exit` and the assertions below would be measuring nothing. Called in
        `_draw_panels`' own order.
        """
        commands_frame._dress_window(OP_SOCKET_PATH, fid="charter-demo-1",
                                     harness_pane=harness_pane, env=None, v=None)
        return commands_frame._split_panels(
            OP_SOCKET_PATH, slots=list(slots), fid="charter-demo-1",
            harness_pane=harness_pane, env=None, pane_env=None)

    def test_a_panels_respawn_hook_is_armed_against_this_server_and_fires(self):
        """#408, end to end on the path it was broken on. `_arm_panel_respawn` refused
        here outright, because `_panel_died_hook_argv` hand-built `["tmux", "-L", …]` and
        would have aimed a `run-shell` at charter's private server — or started an empty
        one named after a socket path.

        Three things a mock cannot check: that `set-hook -p` is accepted on a pane charter
        created inside somebody else's server at all, that the action reaches a real
        shell with the frame id ON IT — there is no `set-environment` here to read
        `$CHARTER_SESSION_ID` back out of, which is the reason `--frame` exists — and
        that the pane the hook is armed on is STILL THERE to fire it.

        **The third is #408's second half, and this test could not see it.** tmux runs
        `pane-died` only for a pane that died and STAYED; the panel used to be handed
        `set-option -p … remain-on-exit on` BY THIS TEST — a command production issues
        nowhere — on a server the fixture had already set the same option on globally. So
        the assertion below was about the test's own arming. Nothing here arms anything
        now: the server is left at tmux's default and the only thing run against it is
        charter's own panel funnel (`_panel_funnel`).

        The property, stated so the next reader tests the property and not this spelling:
        a panel is not covered because some function was called with its slot in a list —
        it is covered because it was BORN INTO A WINDOW THAT KEEPS CORPSES. The pane below
        is split in after the funnel has run and belongs to no slot the funnel was given,
        which is the same shape a density change (`_relayout`) and any future
        panel-creating path have. What it does NOT cover is a panel that leaves that
        window: the coverage is the window's, so `break-pane`/`join-pane` on a panel would
        silently undo it — charter issues neither today, and that is the next spelling of
        this defect.

        Tried up to `_HOOK_TRIALS` times — see `_a_hook_delivery`, and #487."""
        self._require_pane_died_fires()
        _, sid, _, _ = self._operator_server()
        for attempt in range(self._HOOK_TRIALS):
            _, pane_id = self._open_frame_window(sid, fid=f"charter-demo-{attempt}")
            self._require_pane_options(pane_id)
            self._panel_funnel(pane_id)
            gate = os.path.join(self._gate_dir, f"panel-gate-{attempt}")
            panel = _run(tmuxctl.server_argv(
                OP_SOCKET_PATH, "split-window", "-t", pane_id, "-v", "-l", "1",
                "-P", "-F", "#{pane_id}", "--", *_gate_argv(gate, "exit 4"))).stdout.strip()
            self.assertTrue(panel.startswith("%"), panel)
            seen = self._a_hook_delivery(
                socket=OP_SOCKET_PATH, pane=panel, gate=gate,
                interpreter_dir=os.path.join(self._gate_dir, "op interp"))
            if seen is _VOID:
                continue
            self.assertEqual(seen, ["-P", "-m", "charter", "frame-respawn", "top",
                                    "--pane", panel, "--frame", "demo-1"])
            return
        self._no_death_was_delivered()

    def test_a_panel_dying_here_is_destroyed_unless_charter_arms_the_window(self):
        """The NEGATIVE control for the test above, and the measurement #408's first
        round was missing.

        Same server, same window, same pane-scoped `pane-died` hook, same death — and
        charter's panel funnel simply not run. tmux destroys the pane, the hook goes with
        it, and nothing reaches a shell. Without this, "the hook fires" above could be
        true of any pane on any server and nobody would know which fact was carrying it;
        putting `ARMS_REMAIN_ON_EXIT` back to `True` is caught here and nowhere else.

        Asserted on the RECORDED ARGV rather than on any exit status: every command
        involved succeeds either way, which is exactly why this defect survived a round —
        `set-hook -p` on a doomed pane returns 0."""
        self._require_pane_died_fires()
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        gate = os.path.join(self._gate_dir, "unarmed-gate")
        panel = _run(tmuxctl.server_argv(
            OP_SOCKET_PATH, "split-window", "-t", pane_id, "-v", "-l", "1",
            "-P", "-F", "#{pane_id}", "--", *_gate_argv(gate, "exit 4"))).stdout.strip()
        self.assertTrue(panel.startswith("%"), panel)
        seen = self._hook_reaches_a_shim(
            socket=OP_SOCKET_PATH, pane=panel, gate=gate,
            interpreter_dir=os.path.join(self._gate_dir, "bare interp"), timeout=5.0)
        # The DETERMINISTIC half, asserted first: the pane is gone. After that no
        # deadline can change the answer below — tmux cannot fire `pane-died` for a pane
        # it has already destroyed — which is what makes the short wait above sound.
        self.assertTrue(self._wait_until(
            lambda: _run(tmuxctl.server_argv(
                OP_SOCKET_PATH, "display-message", "-p", "-t", panel,
                "#{pane_id}")).stdout.strip() != panel),
            "the pane was still there, so this is not the state the control is about")
        self.assertIsNone(
            seen, "a pane on a server at tmux's own default kept its corpse and fired "
                  "`pane-died` with nothing having armed `remain-on-exit` — if that is "
                  "now tmux's behaviour, the test above is no longer measuring charter")

    def test_arming_the_frames_window_leaves_the_operators_own_windows_alone(self):
        """The blast radius of the one option `_panel_remain_on_exit_argv` writes, read
        back out of tmux rather than argued from its flags.

        `-w` is a scope this module's own docstrings refused on the ground that it reaches
        past charter's own window. It does not, when the target is a pane charter created
        in a window charter opened — measured here in three directions: charter's window
        comes back `on`, the operator's own window is still exactly where it was, the
        SERVER is still at its default, and a pane of theirs that dies is still destroyed
        rather than left as a corpse in their window list. `-g` fails the last three,
        which is why the harness pane's own `remain-on-exit` is `-p` and this one is
        `-w`."""
        _, sid, op_pane, _ = self._operator_server()
        window_id, pane_id = self._open_frame_window(sid)
        default = self._srv("show-options", "-w", "-t", op_pane,
                            "remain-on-exit").stdout.strip()
        self._panel_funnel(pane_id)
        self.assertEqual(self._srv("show-options", "-w", "-t", window_id,
                                   "remain-on-exit").stdout.strip(), "remain-on-exit on")
        self.assertEqual(self._srv("show-options", "-w", "-t", op_pane,
                                   "remain-on-exit").stdout.strip(), default,
                         "charter changed how the operator's OWN window treats a death")
        self.assertEqual(self._srv("show-options", "-g", "remain-on-exit").stdout.strip(),
                         "remain-on-exit off",
                         "charter reached the whole server, not just its own window")
        # And the operator's own pane still dies the way it did before charter arrived.
        gate = os.path.join(self._gate_dir, "their-gate")
        theirs = self._srv("split-window", "-t", op_pane, "-v", "-l", "1", "-P", "-F",
                           "#{pane_id}", "--", *_gate_argv(gate, "exit 0")).stdout.strip()
        self.assertTrue(theirs.startswith("%"), theirs)
        self._release(gate)
        self.assertTrue(self._wait_until(
            lambda: self._srv("display-message", "-p", "-t", theirs,
                              "#{pane_id}").stdout.strip() != theirs),
            "a pane of the operator's own was left behind as a corpse")

    def test_a_frame_here_is_live_by_its_window_never_by_a_session(self):
        """`cmd_respawn` asked `_live_sessions(SOCKET)` unconditionally, which on this
        server is a question about somebody else's sessions: it answers "gone" for a
        frame that is on screen, so a panel could never have been brought back even once
        the hook reached charter. `_frame_is_live` asks the right question per server.

        Both directions against the same real server, so neither can pass by the answer
        always being the same one."""
        _, sid, _, _ = self._operator_server()
        self._open_frame_window(sid, fid="charter-demo-1")
        self.assertTrue(commands_frame._frame_is_live(OP_SOCKET_PATH, "charter-demo-1"))
        self.assertFalse(commands_frame._frame_is_live(OP_SOCKET_PATH, "a-frame-that-ended"))
        # And the operator's OWN session name is not a frame: a `list-sessions`-shaped
        # answer here would report it live and respawn a panel into a window charter
        # never opened.
        sessions = self._srv("list-sessions", "-F", "#{session_name}").stdout.split()
        self.assertTrue(sessions, "the fixture server reported no sessions at all")
        for name in sessions:
            self.assertFalse(commands_frame._frame_is_live(OP_SOCKET_PATH, name), name)

    def test_the_harness_starts_where_charter_was_typed(self):
        """A pane in a server charter did not start otherwise inherits the SESSION's
        working directory — wherever the operator was when they first ran `tmux`."""
        _, sid, _, _ = self._operator_server()
        _, pane_id = self._open_frame_window(sid)
        self._require_pane_options(pane_id)
        out = os.path.join(self._gate_dir, "harness-cwd")
        r = _run(layout.respawn_argv(
            socket=OP_SOCKET_PATH, harness_pane=pane_id, env={}, cwd=self._gate_dir,
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

        **Tried up to `_HOOK_TRIALS` times, and #609 is what put the loop here.** This is
        the test that reads a whole launch's own return code, and on a loaded
        `ubuntu-latest` it went red as `Lists differ: [1] != [21]` at a sha whose OTHER
        workflow run, started two seconds later on the same image, went green. `1` is
        `commands_frame._UNKNOWN_DEATH_CODE`, which is what this launcher answers for a
        death tmux never had the child's status for — #487's window, measured 7 times in
        118 — so the red described the runner and not charter, in the one test in this
        module that had no way to say so. It has one now: the harness pane here carries
        no `pane-died` hook of charter's at all (`_launch_in_operator_tmux`: "the
        harness's exit code travels without hooks here"), so `_PROBE_INDEX` is free, and
        the evidence goes on the SERVER because this launcher's own `kill-window` destroys
        the pane the moment it has the code (:data:`_VoidDeaths._ON_THE_SERVER`).

        **A void is retried and only then skipped, and every other `1` still fails.** Which
        `1` it is comes off two facts that are already recorded rather than a new guess:
        whether tmux ran the pane's array, and what `state.exit_code` holds — `None` for a
        pane that vanished rather than died askably (`_wait_for_harness`'s own `None`, and
        every early `return 1` before the harness ever ran), the fallback for a death tmux
        held no status for. A `1` with the array RUN is charter losing an exit code tmux
        had, and that is the failure this test exists to report.
        """
        name, sid, op_pane, _ = self._operator_server()
        server_pid = self._srv("display-message", "-p", "#{pid}").stdout.strip()

        def _snapshot():
            return tuple(self._srv(*args).stdout for args in (
                ("show-options", "-g"),
                ("show-options", "-t", name),
                ("show-options", "-g", "-w"),
                # The operator's OWN window, added with #408's second half: charter now
                # writes a WINDOW option (`_panel_remain_on_exit_argv`), and a scope slip
                # from charter's window to theirs would otherwise be invisible here.
                ("show-options", "-w", "-t", op_pane),
                ("list-keys",)))

        before = _snapshot()

        def _one_whole_launch(attempt: int) -> bool:
            """One real `cmd_launch` in the operator's server. ``True`` when it measured
            a death, ``False`` for a trial that measured nothing (`_HOOK_TRIALS`)."""
            gate = os.path.join(self._gate_dir, f"e2e-gate-{attempt}")
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

            args = SimpleNamespace(harness="frame",
                                   rest=["--", *_gate_argv(gate, "exit 21")],
                                   no_frame=False)
            rc: list[int] = []

            def _run_launch():
                env = dict(os.environ, TMUX=f"{OP_SOCKET_PATH},{server_pid},{sid[1:]}",
                           TMUX_PANE=op_pane)
                # `_no_real_detached_child` because a launch now FORKS (#512):
                # `_spawn_gather` sends `charter frame-gather` through `util.detach_self`,
                # a real `subprocess.Popen` carrying `os.environ.copy()`. That child is a
                # separate PROCESS, so neither `PersonaIso`'s in-process `config.use()` nor
                # `tests/_planeguard.py` reaches it, and the environment being copied here
                # is the DEVELOPER'S with no `$CHARTER_ROOT` in it — so on any machine
                # where `sys.executable` can import charter the child resolves the plane
                # from the checkout's cwd and gathers, bumps and git-sweeps the operator's
                # live one. Measured on the machine this was written on: `detach_self`
                # really fired, with `argv=['frame-gather', '--session', 'demo-<pid>',
                # '--workspace', 'demo']` and `CHARTER_ROOT=''`. This test is about what a
                # launch writes on somebody else's tmux SERVER; the gather child is not
                # part of that question, and it is the one part of a launch that escapes
                # the isolation.
                # BOTH halves of the tty pair, for the reason `test_frame_launcher._launch`
                # sets out (#545) and with one aggravation of its own: this launch runs on
                # a worker THREAD, so `_picker_wanted` finding a terminal here does not
                # merely hang the test — it hangs a daemon thread nobody is waiting on a
                # prompt from, and what the assertion below reports is "the frame's own
                # window never appeared", which is a true sentence about the wrong subject.
                with mock.patch.dict(os.environ, env, clear=True), \
                     _no_real_detached_child([]), \
                     mock.patch("sys.stdout.isatty", return_value=True), \
                     mock.patch("sys.stdin.isatty", return_value=False), \
                     mock.patch("charter.workspace.resolve", return_value="demo"), \
                     mock.patch.dict(config.FRAME, {"slots": []}):
                    rc.append(commands_frame.cmd_launch(args))

            worker = threading.Thread(target=_run_launch, daemon=True)
            worker.start()
            self.assertTrue(self._wait_until(
                lambda: "demo." in self._srv("list-windows", "-a", "-F",
                                         "#{window_name}").stdout),
                "the frame's own window never appeared in the operator's server")
            fid, harness_pane = self._the_frames_own_window()
            during = _snapshot()
            # AFTER the snapshot, so nothing this test writes can be mistaken for
            # something charter wrote — and before the gate, because the death is what the
            # probe is for. Armed on charter's own pane in charter's own window; the
            # option its action writes is the SERVER's, which outlives the `kill-window`
            # this launcher performs the instant it has the exit code.
            self._arm_array_probe(harness_pane, scope=self._ON_THE_SERVER)
            self.assertIn(
                self._probe_action(self._ON_THE_SERVER),
                self._srv("show-hooks", "-p", "-t", harness_pane).stdout,
                f"the probe is not in this pane's `pane-died` array at "
                f"[{self._PROBE_INDEX}] — charter has started arming a hook of its own "
                "on the harness pane in an operator's tmux, which silently REPLACES this "
                "probe (`set-hook -p` with an index overwrites that index). Every void "
                "check below would then read 'tmux ran no hook' and skip on a healthy "
                "machine")
            self._release(gate)
            worker.join(timeout=25)
            self.assertFalse(worker.is_alive(), "cmd_launch never returned")
            self.assertEqual(before, during,
                             "charter wrote something on a server it is only a guest on")
            self.assertEqual(before, _snapshot())
            self.assertIn(name,
                          self._srv("list-sessions", "-F", "#{session_name}").stdout.split())
            self.assertNotIn("demo.", self._srv("list-windows", "-a", "-F",
                                            "#{window_name}").stdout,
                             "the frame's window was left behind")
            self.assertFalse(decoy.exists(),
                             "a launch is expected to reap a frame directory with no "
                             "`server` marker — if it has stopped doing that, `state.reap`'s "
                             "migration case changed and the isolation this class rests on "
                             "is no longer being exercised by anything")
            if rc == [21]:
                # **The probe is checked on the GREEN path too, and that is what keeps a
                # broken one from turning this test into a permanent skip.** `21` can only
                # have come off `#{pane_dead_status}`, so tmux HAD the child's status — and
                # tmux runs a pane's `pane-died` array once it has it. The array therefore
                # must have run here, on every healthy machine, every time. A probe that
                # cannot see that (a scope that stopped resolving, an option this tmux will
                # not keep across `kill-window`) would be equally blind on the rare
                # non-21 attempt, where its silence is read as "#487's void" and skips —
                # which is a suite going quiet about the thing it was written to measure.
                # Asserted here it is a loud failure on the FIRST run instead.
                self.assertTrue(
                    self._the_array_ran(harness_pane, scope=self._ON_THE_SERVER),
                    "the harness's own exit code came back, so tmux had the child's "
                    "status and ran this pane's `pane-died` array — but the probe armed "
                    "on it reports nothing. Its silence on a void would then mean nothing "
                    "either, and every void would skip a test that could still have been "
                    "measured")
                return True
            # Not the harness's code. `_UNKNOWN_DEATH_CODE` is the ONLY other number this
            # path can answer for a harness that really started, so anything else is a
            # launch that bailed somewhere this test has never described.
            self.assertEqual(rc, [commands_frame._UNKNOWN_DEATH_CODE],
                             "the harness's own exit code did not come back, and what did "
                             "is not charter's fallback either")
            self.assertEqual(
                state.exit_code(fid), commands_frame._UNKNOWN_DEATH_CODE,
                "charter answered its fallback code and recorded no exit for this frame — "
                "the harness pane vanished rather than dying askably (nothing kept it: "
                "`_remain_on_exit_argv`), or the launch returned before the harness ever "
                "ran. Both are charter's, and neither is the tmux 3.4 window "
                "`_HOOK_TRIALS` describes")
            if self._the_array_ran(harness_pane, scope=self._ON_THE_SERVER):
                self.fail(
                    "tmux ran this pane's `pane-died` array — so it HAD the child's "
                    "status — and charter still answered "
                    f"{commands_frame._UNKNOWN_DEATH_CODE} instead of the harness's own "
                    "21. That is an exit code lost between tmux and `cmd_launch`, not a "
                    "trial that measured nothing")
            return False

        for attempt in range(self._HOOK_TRIALS):
            if _one_whole_launch(attempt):
                return
        self._no_death_was_delivered()

    def _the_frames_own_window(self) -> tuple[str, str]:
        """The frame id and harness pane of the window a launch just opened here.

        The window's NAME is the chat id (`layout.window_argv` is given `fid`), and with
        `[frame] slots` empty the frame's window holds exactly the harness pane — so
        `#{pane_id}` for that window is it. Read off tmux rather than guessed, because the
        launcher ALLOCATES `demo.<n>` on a worker thread against whatever this plane's
        frame root already holds, and this test never sees the value.
        """
        rows = [line.split() for line in self._srv(
            "list-windows", "-a", "-F", "#{window_name} #{pane_id}").stdout.splitlines()
            if line.startswith("demo.")]
        self.assertEqual(len(rows), 1, f"expected exactly one frame window, got {rows!r}")
        return rows[0][0], rows[0][1]


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
                                 carry: str | None = None,
                                 carry_raw: str | None = None) -> str:
        """A session on `SOCKET` whose pane writes `$_VAR` out; returns what it wrote.

        *client_value* is what the tmux CLIENT process is started with — the thing a
        launcher controls without `-e`. *carry* is `-e`, or `None` for a call that
        passes none. The pane sleeps afterwards so the SERVER stays up for the next
        session: a server that empties shuts itself down, and a second `new-session`
        against a dead server would start a fresh one and quietly measure nothing.

        *carry_raw* is the `-e` argument spelled by the CALLER rather than built from a
        name and a value — the one way to ask tmux what it does with a spelling charter
        never emits, which is the whole subject of
        :meth:`test_a_bare_e_name_cannot_take_a_variable_away`.
        """
        out = os.path.join(self._gate_dir, f"env-{name}")
        args = ["new-session", "-d", "-s", name, "-x", "80", "-y", "24",
                "-P", "-F", "#{pane_pid}"]
        if carry_raw is not None:
            args += ["-e", carry_raw]
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

    def test_a_bare_e_name_cannot_take_a_variable_away(self):
        """`-e NAME` with no `=` is not an unset, and tmux does not say so.

        `commands_frame._frame_identity_env` emits `NAME=` for every name it has no value
        for, and the obvious-looking alternative is to hand tmux the bare name and let it
        REMOVE the inherited one. Measured here instead of assumed, because the two
        failure modes are opposites and only one of them is visible: `-e` is purely
        ADDITIVE — the bare form is accepted, returns 0, prints nothing, and leaves the
        server's inherited value exactly where it was. A charter that reached for it would
        have handed a second frame the first frame's workspace pin and had no error
        anywhere to show for it.

        Three sessions against one server, so the three answers can only differ by the
        `-e`: the value the server was started with, the same value after a bare `-e`,
        and the empty string after `-e NAME=` — which is the only spelling that shadows,
        and the one charter emits.
        """
        self._require_new_session_env()
        self.assertEqual(self._session_reading_the_var("first", client_value="one"),
                         "one")
        self.assertEqual(
            self._session_reading_the_var("second", client_value="two",
                                          carry_raw=self._VAR),
            "one",
            "`-e NAME` with no `=` removed an inherited variable — if tmux now supports "
            "an unset, `_frame_identity_env` has a better option than shadowing with an "
            "empty value and should be told about it")
        self.assertEqual(
            self._session_reading_the_var("third", client_value="three", carry=""),
            "", "`-e NAME=` is the spelling charter relies on to shadow an inherited "
                "value, and it did not")

# --- The frame's own chrome (#514) ------------------------------------------------- #

#: Every code point tmux can draw a pane border with — the whole Box Drawing block, so a
#: rule is recognised whatever `pane-border-lines` is set to (`single`, `double`, `heavy`
#: and `number` all draw from it, and charter pins `single`).
#:
#: A GLYPH set rather than a coordinate calculation, deliberately: charter has no
#: business re-deriving tmux's own pane geometry inside a test, and every pane in these
#: tests runs `cat`, which draws nothing — so every box-drawing cell on the screen is a
#: cell tmux drew as chrome.
#:
#: **`pane-border-lines simple`'s own `+`/`-`/`|` are deliberately NOT here.** They are
#: ordinary ASCII, indistinguishable from text, and including them made this set match
#: the hyphens in the HOST tmux's status line — which is that server's chrome, drawn in
#: its own colours, and counted as a second rule colour in a frame that had exactly one.
#: `_screenshot` turns that status line off as well; a match that cannot happen for two
#: independent reasons is the right number of reasons for this one.
_RULE_GLYPHS = frozenset(chr(c) for c in range(0x2500, 0x2580))

_SGR_RE = re.compile("\x1b\\[([0-9;]*)m")


def _sgr_runs(text: str, *, carry: bool = False) -> list[tuple[frozenset, str]]:
    """`(appearance, character)` for every printable cell of *text*.

    The appearance is NORMALISED, not the raw escape bytes, because two spellings of one
    look must not read as two colours: `\\x1b[39m` and a line that never issued an escape
    at all are both "the terminal's own foreground", and a test comparing escape STRINGS
    would call those different — then pass on a frame that looks perfectly uniform. That
    is the spelling-versus-property trap this repo keeps paying for, so the property
    (what a cell LOOKS like) is what gets compared.

    Line-scoped by default: `capture-pane -e` re-states a line's attributes at its start,
    and a run carried across a newline would attribute one row's colour to the next.

    ***carry* turns that off, and it is needed for a BACKGROUND.** The re-statement is
    tmux's optimiser, not a rule: a row whose background is already what the previous row
    ended on gets no SGR at all, so a horizontal rule drawn over the frame's surface comes
    back looking like a rule over the terminal's own — measured on 3.7c, where the vertical
    rules carry `ESC[100m` and the horizontal ones carry nothing because the painted panel
    above them already left the terminal there. A real terminal carries attributes across a
    newline, so this is the faithful reading; the line-scoped default is the conservative
    one and #514's tests keep it.
    """
    out: list[tuple[frozenset, str]] = []
    fg: object = "default"
    bg: object = "default"
    attrs: set[int] = set()
    for line in text.split("\n"):
        if not carry:
            fg, bg, attrs = "default", "default", set()
        i = 0
        while i < len(line):
            m = _SGR_RE.match(line, i)
            if m:
                params = [int(p or 0) for p in m.group(1).split(";")]
                j = 0
                while j < len(params):
                    p = params[j]
                    if p == 0:
                        fg, bg, attrs = "default", "default", set()
                    elif p in (38, 48):
                        # 38;5;N or 38;2;R;G;B — consume the whole colour, whichever form
                        take = 3 if j + 1 < len(params) and params[j + 1] == 5 else 5
                        value = tuple(params[j:j + take])
                        if p == 38:
                            fg = value
                        else:
                            bg = value
                        j += take
                        continue
                    elif 30 <= p <= 37 or 90 <= p <= 97:
                        fg = p
                    elif p == 39:
                        fg = "default"
                    elif 40 <= p <= 47 or 100 <= p <= 107:
                        bg = p
                    elif p == 49:
                        bg = "default"
                    elif p in (1, 2, 3, 4, 5, 7, 9):
                        attrs.add(p)
                    elif p == 22:
                        attrs -= {1, 2}
                    elif p in (23, 24, 25, 27, 29):
                        attrs.discard(p - 20)
                    j += 1
                i = m.end()
                continue
            out.append((frozenset({("fg", fg), ("bg", bg),
                                   *(("attr", a) for a in sorted(attrs))}), line[i]))
            i += 1
    return out


def _rule_states(text: str, *, carry: bool = False) -> set:
    """The distinct appearances every pane-border cell in *text* is drawn with.

    One entry means every rule in the frame looks the same, which IS #514's property.

    *carry* is `_sgr_runs`' own, and a frame with a SURFACE needs it: tmux does not
    re-state a background a row already inherits, so the horizontal rules read as the
    terminal's own default while the vertical ones read as the surface, and the property
    would come back false on a frame that renders perfectly uniform.
    """
    return {state for state, ch in _sgr_runs(text, carry=carry) if ch in _RULE_GLYPHS}


def _rule_glyphs(text: str) -> set:
    """Which border characters *text* is drawn with — `─│┬┴` for `pane-border-lines
    single`, `═║╦╩` for `double`, and so on. What a rule is made OF, beside
    `_rule_states`' what it looks like."""
    return {ch for ch in text if ch in _RULE_GLYPHS}


#: What `pane-border-indicators arrows` marks the ACTIVE pane's borders with, and only
#: the active pane's — so an inherited `arrows` puts a glyph on one rule that its
#: neighbour does not carry. Outside the Box Drawing block, which is why `_rule_glyphs`
#: cannot see them and they are asked about by name.
_INDICATOR_GLYPHS = frozenset("←→↑↓")


class ChromeIsOneColour(_TmuxServerFixture, PersonaIso, unittest.TestCase):
    """#514, asked of the SCREEN: charter's own frame must draw every rule the same.

    **The only tests in this suite that look at what an operator looks at.** A pane
    border belongs to no pane, so `capture-pane` cannot see one — it captures a pane's
    CONTENT, and the chrome lives in the gaps between panes. So the frame is rendered
    inside a second tmux: an OUTER server holds one pane whose program is a client
    attached to the frame's own (inner) server, which makes that pane's content the
    frame's entire screen, borders and all. `capture-pane -e` on the outer pane then
    hands the rules back with their colour escapes still attached — as close to the
    operator's own screenshot as a test gets without a camera.

    **Every test here renders the defect first, as a control.** Not decoration: without
    it, "every rule is one colour" is equally satisfied by a machine that rendered no
    rules at all, or by a capture this parser happened to read as one long run — and a
    harness that cannot see the two-coloured frame cannot testify about the one-coloured
    one. The control is a live render on this machine at this moment, never a baseline
    read out of another branch. A machine whose control comes back uniform SKIPS, naming
    what it could not measure.
    """

    #: The frame's own shape, in the order the shipped `slots` produces it: a one-row
    #: `top`, a `bottom`, and a `right` column beside the harness — every split off the
    #: harness pane, exactly as `layout.panel_argvs` does it.
    #:
    #: Three panels rather than one, because the defect needs a rule that runs PAST the
    #: active pane's corner: with the harness and a single neighbour, every border cell
    #: touches the active pane and the frame comes out uniform by accident.
    _SPLITS = (("-v", "-b", "1"), ("-v", "", "3"), ("-h", "", "22"))

    def _outer(self, *args: str) -> subprocess.CompletedProcess:
        return _tmux_on(self._outer_socket, *args)

    def setUp(self) -> None:
        super().setUp()
        self._outer_socket = f"{self.SOCKET_NAME}-host"
        self.addCleanup(self._teardown_outer)

    def _teardown_outer(self) -> None:
        """The same two-step `_teardown_socket` does, for the second server these tests
        need — `kill-server` ends it, and the socket FILE it leaves behind is removed by
        hand because tmux does not."""
        self._outer("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / self._outer_socket).unlink(missing_ok=True)

    def _hostile(self) -> None:
        """Somebody else's `.tmux.conf`, applied the way theirs is: `-wg`, reaching every
        window on the server charter is a guest on."""
        for name, value in (("pane-border-style", "fg=blue"),
                            ("pane-active-border-style", "fg=red,bold"),
                            ("pane-border-indicators", "arrows"),
                            ("pane-border-lines", "double"),
                            ("pane-border-status", "top")):
            self.assertEqual(self._srv("set", "-wg", name, value).returncode, 0)

    def _screenshot(self, *, arm: bool, hostile: bool = False,
                    surface: str | None = None, design: str = "pane",
                    focus: int | None = None) -> str:
        """Build the frame on the inner server, show it through the outer one, and return
        what the outer pane holds — the frame's whole screen, escapes and all.

        *surface* paints the three PANELS the way `_surface_argvs` paints them (pane-scoped
        `window-style`, harness pane untouched), and *design* is which of charter's two
        answers for the RULES is applied — the whole point being that all three renderings
        come out of the production argvs and differ only in the arguments production would
        really pass:

        * ``"bare"`` — panels painted, rules left as `_CHROME_STYLE` alone. The defect the
          operator first reported: a one-cell strip of the terminal's own between panes
          that are all one colour. This class's "render the defect first" discipline,
          applied to a background rather than a foreground.
        * ``"window"`` — `_chrome_argvs` carries the frame-wide surface. What charter does
          BELOW `tmuxctl.PANE_BORDER_FLOOR`, and what #628 shipped everywhere.
        * ``"panel"`` — each panel carries its own edges and the harness's two options are
          left unset. What #631 shipped, kept as a CONTROL: tmux resolves every border cell
          around the harness against the harness's own options, so this renders one
          horizontal rule in two colours and is the defect the operator reported third.
        * ``"pane"`` — ``"panel"`` plus `_harness_rule_argvs` on the harness pane, which is
          what charter does above the floor now.
        """
        session = f"f{int(arm)}{int(hostile)}{int(bool(surface))}{design[0]}-{self._pane_counter}"
        self._pane_counter += 1
        # **Every pane-scoped write below is gated on the PROBE, not on *design* alone**,
        # and that is a hazard this harness has now demonstrated twice. Below
        # `tmuxctl.PANE_BORDER_FLOOR` `set -p` on a border option is rc 0 and writes the
        # WINDOW, and `set -p -u` REMOVES the window's — charter's own #514 pin for every
        # rule in the frame. `design="pane"` with no *surface* asks
        # `_harness_rule_argvs` for exactly that removal, so on CI's tmux 3.4 an ungated
        # call turned the two #514 screenshots green-and-default again. Production is
        # gated on the tmux version (`_pane_borders_wanted`); this is that gate, in the
        # form this module insists on. A test that needs the per-pane design still has to
        # say so with `_require_pane_scoped_borders`, which skips rather than degrading.
        per_pane = design in ("pane", "panel") and self._pane_scoped_borders()
        r = self._srv("new-session", "-d", "-s", session, "-x", "100", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        # `conf_text`'s own first line, so this screenshot is of a real frame rather than
        # of a bare tmux session. It is also load-bearing for the hostname assertion in
        # `test_the_frame_draws_every_rule_the_same_inside_the_operators_own_tmux`: tmux's
        # DEFAULT `status-right` carries `#{=21:pane_title}`, which is this machine's
        # hostname truncated to 21 cells — so on a machine whose hostname is 21
        # characters or shorter that assertion would fail on the status line and never
        # reach the borders it is about, and on this developer's 24-character hostname it
        # would have passed by truncation. Neither is a measurement.
        self.assertEqual(self._srv("set", "-t", session, "status", "off").returncode, 0)
        if hostile:
            self._hostile()
        if arm:
            # The production argv, never a hand-retyped `set-option` — see `_run`.
            for cmd in commands_frame._chrome_argvs(
                    socket=self.SOCKET_NAME, harness_pane=harness,
                    surface=surface if design == "window" else None):
                self.assertEqual(_run(cmd).returncode, 0, cmd)
            # The production argv for the harness's OWN three edges, which above the floor
            # no window option reaches. Handed the same surface the panels are painted
            # with, because that is what `instance.agreed_border_bg` answers for an
            # arrangement whose components all name one colour — which is what *surface*
            # makes this screenshot.
            for cmd in commands_frame._harness_rule_argvs(
                    socket=self.SOCKET_NAME, harness_pane=harness,
                    surface=surface, pane_borders=per_pane and design == "pane"):
                self.assertEqual(_run(cmd).returncode, 0, cmd)
        panels = []
        for direction, before, size in self._SPLITS:
            args = ["split-window", "-t", harness, direction, "-P", "-F", "#{pane_id}"]
            if before:
                args.insert(4, before)
            args += ["-l", size, "--", "cat"]
            r = self._srv(*args)
            self.assertEqual(r.returncode, 0, args)
            panels.append(r.stdout.strip())
            if surface:
                # The production argv again, and pane-scoped exactly as production is —
                # so the harness pane is never an argument to THIS one, which is the
                # boundary ADR 0018 draws: `_surface_argvs` is what paints an interior,
                # and the harness's is the rectangle charter never draws in.
                for cmd in commands_frame._surface_argvs(
                        socket=self.SOCKET_NAME, pane_id=r.stdout.strip(), chrome="off",
                        bg=surface.removeprefix("bg="), pane_borders=per_pane):
                    self.assertEqual(_run(cmd).returncode, 0, cmd)
        # *focus* is which pane is ACTIVE when the shot is taken. The harness by
        # default, and that default is why #514's two-colour rule went unnoticed
        # here for so long: tmux draws a border cell from `pane-active-border-style`
        # when it touches the active pane, so a frame only ever photographed with
        # one pane active has three quarters of its states unphotographed.
        self.assertEqual(
            self._srv("select-pane", "-t",
                      harness if focus is None else panels[focus]).returncode, 0)
        host = f"host-{session}"
        r = self._outer("new-session", "-d", "-s", host, "-x", "100", "-y", "24",
                        "--", "tmux", "-L", self.SOCKET_NAME, "attach", "-t", session)
        self.assertEqual(r.returncode, 0, r.stderr)
        # The HOST's own status line is not part of the frame, and leaving it on puts a
        # second server's chrome — its own colours, and this machine's hostname — into
        # the bottom row of every screenshot. Off, so the capture is the frame and
        # nothing else. (`charter` turns its own off the same way, in `conf_text`.)
        self._outer("set", "-t", host, "status", "off")
        # Polled rather than slept: the inner client has to connect, resize and paint
        # before there is anything to read, and how long that takes is the machine's
        # business. Gives up quietly — `_control` is what turns "nothing rendered" into a
        # skip that says so.
        deadline = time.time() + 10
        shot = ""
        while time.time() < deadline:
            # `-N` keeps the trailing spaces on each row, and without it a SURFACE is
            # invisible to this harness: every pane here runs `cat` and writes nothing, so
            # a painted pane is a rectangle of spaces and `capture-pane` trims the whole
            # of it, leaving the background SGR with no cell to describe. It adds no
            # glyph, so `_rule_states` and `_rule_glyphs` see exactly what they saw
            # before. Available at `tmuxctl.FLOOR` as well as on 3.7c (measured).
            got = self._outer("capture-pane", "-p", "-e", "-N", "-t", host)
            if got.returncode == 0:
                shot = got.stdout
            if _rule_states(shot):
                break
            time.sleep(0.1)
        self._outer("kill-session", "-t", host)
        self._srv("kill-session", "-t", session)
        return shot

    def _control(self, **kw) -> str:
        """The frame as it renders UNFIXED — and the proof these tests can see the defect
        at all. Skips (never fails, and never passes quietly) on a machine whose render
        this harness cannot read."""
        shot = self._screenshot(arm=False, **kw)
        states = _rule_states(shot)
        if not states:
            raise unittest.SkipTest(
                "this machine rendered no pane borders through a nested tmux client at "
                "all, so there is nothing here to measure the colour of")
        if len(states) == 1:
            raise unittest.SkipTest(
                "the unfixed frame already renders every rule identically on this tmux "
                f"({states}) — the two-colour defect #514 is about does not reproduce "
                "here, so a fixed frame rendering uniformly would prove nothing")
        return shot

    def test_the_unfixed_frame_really_does_draw_its_rules_in_two_colours(self):
        """The defect itself, named rather than only used as a control. tmux ships
        `pane-active-border-style fg=green` beside `pane-border-style default` and picks
        per BORDER CELL — so one horizontal rule is green for the cells above the active
        pane and the terminal's own default for the cells above its neighbour."""
        states = _rule_states(self._control())
        self.assertGreater(len({dict(s)["fg"] for s in states}), 1,
                           f"the rules differ, but not in colour: {states}")

    def test_the_frame_draws_every_rule_the_same_on_charters_own_server(self):
        self._control()
        states = _rule_states(self._screenshot(arm=True))
        self.assertTrue(states, "the armed frame rendered no rules at all")
        self.assertEqual(len(states), 1,
                         f"charter's own frame still draws its rules two ways: {states}")

    def test_the_frame_draws_every_rule_the_same_inside_the_operators_own_tmux(self):
        """The second server path, where charter's assumptions do not hold: the borders
        are not tmux's defaults here, they are the operator's own config, which charter
        would otherwise inherit whole. All five settings, because colour is only the most
        visible of them — their `pane-border-status top` writes this machine's hostname
        into every rule and takes a row the frame's own arithmetic never budgeted for."""
        self._control(hostile=True)
        shot = self._screenshot(arm=True, hostile=True)
        states = _rule_states(shot)
        self.assertTrue(states, "the armed frame rendered no rules at all")
        self.assertEqual(len(states), 1,
                         "the operator's own border styling still reaches charter's "
                         f"window: {states}")
        self.assertNotIn(socket.gethostname().split(".")[0], shot,
                         "`pane-border-status` is still on, so the frame's rules are "
                         "carrying this machine's hostname")
        self.assertEqual(set(shot) & _INDICATOR_GLYPHS, set(),
                         "`pane-border-indicators` is still theirs, so charter's frame "
                         "marks its active pane's borders and not its others")
        # The whole property in one line: the frame drawn inside their tmux is the same
        # frame, cell for cell of chrome, as the one drawn on charter's own server.
        # Colour alone would not catch a `pane-border-lines double` — every rule would
        # still be one colour, and the frame would still not be charter's.
        own = self._screenshot(arm=True)
        self.assertEqual((_rule_glyphs(shot), states),
                         (_rule_glyphs(own), _rule_states(own)),
                         "charter's frame is drawn differently on the operator's server "
                         "than on charter's own")

    def _require_pane_scoped_borders(self) -> None:
        """Skip unless THIS tmux really keeps `pane-border-style` per pane."""
        if not self._pane_scoped_borders():
            raise unittest.SkipTest(
                "this tmux has no pane scope for `pane-border-style` — `set -p` wrote the "
                "window, which is what `tmuxctl.PANE_BORDER_FLOOR` gates")

    def _pane_scoped_borders(self) -> bool:
        """Does THIS tmux really keep `pane-border-style` per pane?

        **Probed, never a version string**, per this plane's rule and this module's own
        docstring — and here the probe is the only honest form, because the thing being
        detected does not fail loudly. Below `tmuxctl.PANE_BORDER_FLOOR` the option is a
        window option: `set -p` returns 0 and writes the WINDOW, so a test that assumed
        pane scope would not error, it would assert against a frame where charter's write
        had leaked onto the harness — which is exactly what CI's tmux 3.4 did to the first
        version of these tests.

        The probe performs that write on a throwaway session this test owns, so the only
        thing it can damage is its own.
        """
        session = f"probe-{self._pane_counter}"
        self._pane_counter += 1
        r = self._srv("new-session", "-d", "-s", session, "-x", "40", "-y", "10",
                      "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self._srv("set", "-w", "-t", pane, "pane-border-style", "fg=default")
        self._srv("set", "-p", "-t", pane, "pane-border-style", "bg=red")
        window = self._srv("show", "-wv", "-t", pane, "pane-border-style").stdout.strip()
        self._srv("kill-session", "-t", session)
        return window == "fg=default"

    def test_the_floor_agrees_with_what_this_tmux_actually_does(self):
        """The version constant, checked against the binary rather than against tmux's
        source. Where a real tmux is present its behaviour is the authority, so a
        `PANE_BORDER_FLOOR` moved without a measurement goes red on the machine that could
        have measured it."""
        v = tmuxctl.version()
        if v is None:
            raise unittest.SkipTest("no readable tmux version to check the floor against")
        supported = True
        try:
            self._require_pane_scoped_borders()
        except unittest.SkipTest:
            supported = False
        self.assertEqual(supported, v >= tmuxctl.PANE_BORDER_FLOOR,
                         f"tmux {v} {'does' if supported else 'does not'} keep "
                         "`pane-border-style` per pane, which is not what "
                         f"`PANE_BORDER_FLOOR` {tmuxctl.PANE_BORDER_FLOOR} says")

    #: The colour every panel in a surfaced screenshot is painted, and the only
    #: non-default background on the screen — the harness pane is never painted, and every
    #: pane runs `cat` and writes nothing.
    _SURFACE = "bg=brightblack"

    def _grid(self, shot: str) -> list[list[tuple]]:
        """*shot* as rows of `(appearance, character)`, so a border cell can be compared
        with the pane cells ON EITHER SIDE OF IT.

        `_sgr_runs` answers one flat list with the row breaks dropped, which is right for
        "what colours are the rules" and useless for "does this rule match its
        neighbours". The printable width of each line is what slices it back into rows.
        `carry=True` for the reason that flag exists: tmux does not re-state a background a
        row already inherits.
        """
        lines = shot.split("\n")
        cells = _sgr_runs(shot, carry=True)
        rows, at = [], 0
        for line in lines:
            width = len(_SGR_RE.sub("", line))
            rows.append(cells[at:at + width])
            at += width
        return rows

    def _seams(self, shot: str) -> list[tuple]:
        """Every rule cell whose background matches NEITHER pane it separates.

        The definition of the defect, and it needs no pane geometry: a `─` is compared
        with the cell above and the cell below it, a `│` with the cells left and right.
        Junctions are skipped — a `┬` has three neighbours and no one right answer. A
        border cell may legitimately take either side (tmux draws one border, not two
        half-borders); what it may never be is a THIRD colour, which is what a seam is.
        """
        rows = self._grid(shot)
        out = []
        for y, row in enumerate(rows):
            for x, (state, ch) in enumerate(row):
                if ch == "\u2500":
                    sides = [rows[y - 1][x:x + 1], rows[y + 1][x:x + 1]]
                elif ch == "\u2502":
                    sides = [row[x - 1:x], row[x + 1:x + 2]]
                else:
                    continue
                near = [dict(c[0][0])["bg"] for c in sides if c and c[0]]
                if len(near) == 2 and dict(state)["bg"] not in near:
                    out.append((y, x, dict(state)["bg"], near))
        return out

    def _harness_edges(self, shot: str) -> set:
        """The backgrounds the rules touching the UNPAINTED pane are drawn in.

        The harness is the only pane charter never paints, so it is the only pane whose
        interior cells are the terminal's own — which is how this finds its edges without
        knowing where tmux put it.
        """
        rows = self._grid(shot)
        out = set()
        for y, row in enumerate(rows):
            for x, (state, ch) in enumerate(row):
                if ch == "\u2500":
                    sides = [rows[y - 1][x:x + 1], rows[y + 1][x:x + 1]]
                elif ch == "\u2502":
                    sides = [row[x - 1:x], row[x + 1:x + 2]]
                else:
                    continue
                near = [dict(c[0][0])["bg"] for c in sides if c and c[0]]
                if "default" in near:
                    out.add(dict(state)["bg"])
        return out

    def _painted(self, shot: str):
        """The one background the panels are painted in, or a skip naming what this
        machine could not render. `_control`'s discipline: a harness that cannot see the
        surface cannot testify about the rules drawn over it."""
        cells = _sgr_runs(shot, carry=True)
        if not any(ch in _RULE_GLYPHS for _s, ch in cells):
            raise unittest.SkipTest("this machine rendered no pane borders at all")
        painted = {dict(s)["bg"] for s, _ch in cells} - {"default"}
        if len(painted) != 1:
            raise unittest.SkipTest(
                "this tmux did not paint the panels one colour through a nested client "
                f"({painted}), so there is no surface here for a rule to match")
        return painted.pop()

    def test_an_unsurfaced_rule_really_does_leave_a_seam_between_painted_panes(self):
        """The first defect, rendered — and the control for both tests below. Panels
        painted, rules left as `_CHROME_STYLE` alone: border cells come back over the
        terminal's own background while the panes on BOTH sides of them are over another.
        That one-cell strip is what the operator reported off a screenshot."""
        shot = self._screenshot(arm=True, surface=self._SURFACE, design="bare")
        self._painted(shot)
        self.assertTrue(self._seams(shot),
                        "no rule here is drawn over a background neither of its "
                        "neighbours has, so the seam does not reproduce on this tmux and "
                        "a frame without one would prove nothing")

    def test_no_shipped_design_leaves_a_seam_between_two_panes_of_one_colour(self):
        """The property every one of charter's answers must have, asked of the screen. A
        rule may take either side — tmux draws one border, not two half-borders — but it may
        never be a third colour, and a third colour between two identically-painted panes
        is exactly the strip this began with."""
        self.assertTrue(
            self._seams(self._screenshot(arm=True, surface=self._SURFACE, design="bare")),
            "the control rendered no seam, so this machine cannot testify")
        for design in ("window", "pane"):
            with self.subTest(design=design):
                if design == "pane":
                    self._require_pane_scoped_borders()
                shot = self._screenshot(arm=True, surface=self._SURFACE, design=design)
                self._painted(shot)
                self.assertEqual(self._seams(shot), [])

    def _rule_rows(self, shot: str) -> dict[int, set]:
        """The backgrounds each ROW's horizontal rule is drawn in, keyed by row.

        `_seams` asks whether a cell matches a NEIGHBOUR, which the defect here satisfies:
        the dark half of the rule matches the dark harness under it and the surfaced half
        matches the surfaced sidebar. What it does not satisfy is #514's own property —
        one rule, one colour — and that is a question about a whole row rather than about
        a cell, so it needs its own reading.
        """
        out: dict[int, set] = {}
        for y, row in enumerate(self._grid(shot)):
            bgs = {dict(state)["bg"] for state, ch in row if ch == "─"}
            if bgs:
                out[y] = bgs
        return out

    def test_the_previous_design_really_does_draw_one_rule_in_two_colours(self):
        """**The control, and the report.** #631 left the harness's own two border options
        unset. Above `tmuxctl.PANE_BORDER_FLOOR` tmux resolves each border cell against one
        pane — the first in the window's list whose border box holds it, which is the
        harness — so the harness owns every cell along its top and bottom rules up to its
        own corner, and the sidebar owns the rest of those same two rows. Unset on one and
        surfaced on the other is one horizontal rule in two colours, which is #514's defect
        in a place #514 never looked."""
        self._require_pane_scoped_borders()
        shot = self._screenshot(arm=True, surface=self._SURFACE, design="panel")
        painted = self._painted(shot)
        split = {y: bgs for y, bgs in self._rule_rows(shot).items() if len(bgs) > 1}
        self.assertTrue(split,
                        "no horizontal rule came back in two colours, so this machine "
                        "cannot see the defect and cannot testify about the fix")
        self.assertEqual(set().union(*split.values()), {"default", painted})

    def test_every_rule_in_a_surfaced_frame_is_one_colour(self):
        """#514's own property, asked of a frame that has a SURFACE — which is where it was
        never asked before and is how #631 lost it. Both shipped designs must hold it: the
        frame-wide one below `tmuxctl.PANE_BORDER_FLOOR` and the per-pane one above it."""
        self._require_pane_scoped_borders()
        self.assertTrue(
            {y for y, bgs in self._rule_rows(
                self._screenshot(arm=True, surface=self._SURFACE,
                                 design="panel")).items() if len(bgs) > 1},
            "the control rendered every rule in one colour already")
        for design in ("window", "pane"):
            with self.subTest(design=design):
                shot = self._screenshot(arm=True, surface=self._SURFACE, design=design)
                painted = self._painted(shot)
                self.assertEqual(self._rule_states(shot), {painted},
                                 "the frame's rules are not all one colour")
                self.assertEqual(self._harness_edges(shot), {painted},
                                 "the rules touching the pane charter never paints stop "
                                 "short of it, which is the seam")

    def _rule_states(self, shot: str) -> set:
        """`_rule_states`' BACKGROUNDS, which is what a surface is. The module-level one
        answers whole appearances and would call two cells different over an `fg` neither
        design changes."""
        return {dict(state)["bg"]
                for state, ch in _sgr_runs(shot, carry=True) if ch in _RULE_GLYPHS}

    def test_the_frame_does_not_move_when_focus_does(self):
        """#514's four-focus-state measurement, re-run now that the HARNESS carries edges
        of its own.

        The harness is a pane that can be active, and tmux draws a border cell from
        `pane-active-border-style` when it touches the active one — so a harness whose two
        options differed would put #514's defect back on the very rules this closed, and
        would do it only while the operator's own pane had the keyboard. Every pane in the
        frame takes a turn, harness included, and the rules must be one colour in all of
        them — and the SAME one, which is the half a per-shot assertion would miss.

        `_painted` is not the reading here and that is the measurement: a focused panel is
        painted its `window-active-style` shade, so a shot with a panel active has two
        pane backgrounds on it by design. What must not move is the RULES.
        """
        self._require_pane_scoped_borders()
        seen = {}
        for focus in (None, 0, 1, 2):
            shot = self._screenshot(arm=True, surface=self._SURFACE, design="pane",
                                    focus=focus)
            states = self._rule_states(shot)
            if not states:
                raise unittest.SkipTest("this machine rendered no pane borders at all")
            seen["harness" if focus is None else focus] = states
        self.assertEqual(len(set(map(frozenset, seen.values()))), 1,
                         f"a rule changed colour with the active pane: {seen}")
        one = next(iter(seen.values()))
        self.assertEqual(len(one), 1, f"one shot has rules in two colours: {seen}")
        self.assertNotEqual(one, {"default"},
                            "no rule carried a surface in any focus state, so this "
                            "machine cannot testify about one moving")

    def test_the_harness_gives_up_its_edges_and_keeps_its_interior(self):
        """Read off tmux rather than off the screen, and both halves at once, because they
        are what the change traded against each other.

        Its EDGES carry charter's rule in the frame's surface — the same value the panel's
        two carry, byte for byte, which is what makes a rule that runs from one to the
        other one colour. Its INTERIOR carries nothing at all: `window-style` and
        `window-active-style` are not among `instance.PANE_BORDER_OPTIONS`, and
        `_surface_argvs` — the only thing that sets them — is still only ever handed a
        panel. That is ADR 0018's line, and it is the one this had to keep.
        """
        self._require_pane_scoped_borders()
        session = f"hb-{self._pane_counter}"
        self._pane_counter += 1
        r = self._srv("new-session", "-d", "-s", session, "-x", "80", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        for cmd in commands_frame._chrome_argvs(socket=self.SOCKET_NAME,
                                                harness_pane=harness):
            _run(cmd)
        for cmd in commands_frame._harness_rule_argvs(
                socket=self.SOCKET_NAME, harness_pane=harness, surface="bg=brightblack",
                pane_borders=True):
            self.assertEqual(_run(cmd).returncode, 0, cmd)
        p = self._srv("split-window", "-t", harness, "-P", "-F", "#{pane_id}", "--", "cat")
        panel = p.stdout.strip()
        for cmd in commands_frame._surface_argvs(socket=self.SOCKET_NAME, pane_id=panel,
                                                 chrome="dark", bg="brightblack",
                                                 pane_borders=True):
            self.assertEqual(_run(cmd).returncode, 0, cmd)
        for option in instance.PANE_BORDER_OPTIONS:
            with self.subTest(option=option):
                got = self._srv("show", "-p", "-t", harness, "-v", option).stdout.strip()
                self.assertEqual(got, f"{commands_frame._CHROME_STYLE},bg=brightblack",
                                 "tmux did not read charter's rule back off the harness")
                self.assertEqual(
                    got,
                    self._srv("show", "-p", "-t", panel, "-v", option).stdout.strip(),
                    "the harness's edges and the panel's are two different values, so a "
                    "rule running from one to the other is two colours")
        for option in instance.chrome_option_names():
            with self.subTest(option=option):
                self.assertEqual(
                    self._srv("show", "-p", "-t", harness, "-v", option).stdout.strip(),
                    "", "charter painted inside the harness pane")
                self.assertTrue(
                    self._srv("show", "-p", "-t", panel, "-v", option).stdout.strip(),
                    "the panel was not painted either, so the harness reading '' proves "
                    "nothing")
        self._srv("kill-session", "-t", session)

    def test_the_unset_really_does_take_the_harnesss_edges_back_off(self):
        """`chrome: off` on a plane that named no colour, and an arrangement whose panels
        stopped agreeing, both arrive at `_harness_rule_argvs` as no surface — and on a
        frame that has been running they have to REMOVE what is there rather than leave it.
        `set -p -u` on an option that was never set is rc 0 too, which is what lets the
        launch path share this function."""
        self._require_pane_scoped_borders()
        session = f"hu-{self._pane_counter}"
        self._pane_counter += 1
        r = self._srv("new-session", "-d", "-s", session, "-x", "80", "-y", "24",
                      "-P", "-F", "#{pane_id}", "--", "cat")
        harness = r.stdout.strip()
        for cmd in commands_frame._chrome_argvs(socket=self.SOCKET_NAME,
                                                harness_pane=harness):
            self.assertEqual(_run(cmd).returncode, 0, cmd)
        for cmd in commands_frame._harness_rule_argvs(
                socket=self.SOCKET_NAME, harness_pane=harness, surface="bg=brightblack",
                pane_borders=True):
            self.assertEqual(_run(cmd).returncode, 0, cmd)
        self.assertTrue(self._srv("show", "-p", "-t", harness, "-v",
                                  instance.PANE_BORDER_OPTIONS[0]).stdout.strip(),
                        "nothing was set, so the removal below proves nothing")
        for cmd in commands_frame._harness_rule_argvs(
                socket=self.SOCKET_NAME, harness_pane=harness, surface=None,
                pane_borders=True):
            self.assertEqual(_run(cmd).returncode, 0, cmd)
        for option in instance.PANE_BORDER_OPTIONS:
            with self.subTest(option=option):
                self.assertEqual(
                    self._srv("show", "-p", "-t", harness, "-v", option).stdout.strip(),
                    "", "the surface outlived the word that removed it")
        self.assertEqual(
            self._srv("show", "-w", "-t", harness, "-v",
                      instance.PANE_BORDER_OPTIONS[0]).stdout.strip(),
            commands_frame._CHROME_STYLE,
            "the pane-scoped unset reached the WINDOW's value, which is charter's own "
            "#514 pin for every rule in the frame")
        self._srv("kill-session", "-t", session)
        self._srv("kill-session", "-t", session)


class EveryBorderOptionThisTmuxHasIsPinned(_TmuxServerFixture, PersonaIso,
                                           unittest.TestCase):
    """Asked of tmux's own option table, never of a list somebody remembered to update.

    #514 was one option charter never set. The CLASS of defect is "tmux decides part of
    charter's chrome and charter never gave an answer", and a hand-written list of five
    names cannot see the sixth option a later tmux adds. `show-options -wg` is tmux
    itself saying what it draws a border from.
    """

    def _window_options(self) -> dict[str, str]:
        # A server has to exist for `show-options -g` to answer; `_new_pane` starts one
        # the way every other class here does.
        self._new_pane()
        r = self._srv("show-options", "-wg")
        self.assertEqual(r.returncode, 0, r.stderr)
        out = {}
        for line in r.stdout.splitlines():
            name, _, value = line.partition(" ")
            out[name] = value
        return out

    def test_no_pane_border_option_is_left_for_the_operators_config_to_decide(self):
        """Every `pane-*border*` window option this tmux has, pinned by `_CHROME`.

        `pane-border-format` is the one exclusion, and it is CONDITIONAL rather than
        permanent: it decides what `pane-border-status` draws, and charter pins that to
        `off`, so nothing renders it. The first assertion re-establishes that condition
        instead of trusting it — turn the status on and this test starts demanding the
        format be pinned too.
        """
        pinned = dict(commands_frame._CHROME)
        self.assertEqual(pinned.get("pane-border-status"), "off",
                         "`pane-border-status` is no longer off, so `pane-border-format` "
                         "renders and must be pinned as well")
        inert = {"pane-border-format"}
        theirs = {name for name in self._window_options()
                  if name.startswith("pane-") and "border" in name} - inert
        self.assertTrue(theirs, "this tmux reported no pane-border options at all")
        self.assertEqual(theirs - set(pinned), set(),
                         "this tmux draws pane borders from an option charter never "
                         "answers, so the operator's own config decides part of the "
                         "frame's chrome")

    def test_every_option_charter_pins_is_one_this_tmux_actually_has(self):
        """The other direction. A misspelt option name is refused by `set-option`, and a
        launch treats that refusal as non-fatal — so the typo would leave the real option
        inherited and the frame wrong, with nothing but a warning to show for it."""
        theirs = self._window_options()
        for name in dict(commands_frame._CHROME):
            self.assertIn(name, theirs, f"{name} is not a window option on this tmux")


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed on this machine")
class FocusEventsIntegration(_NeedsAttachedClient, PersonaIso, unittest.TestCase):
    """`conf_text`'s `set -g focus-events on`, and the reason it is a CONFIG LINE rather
    than something charter could detect at runtime and decide about.

    Spec §4f closes a component's event kinds at six, two of which are `focus` and `blur`.
    tmux ships `focus-events` OFF; with it off tmux writes `\\x1b[?1004l` to the client and
    never delivers a pane focus transition, so those two kinds do not exist until charter
    turns the option on. `tests/test_frame_launcher.py`'s `Conf` pins the TEXT (and fails
    on a machine with no tmux at all); this class pins what tmux does with it.

    **The second test here is the interesting one, and it is a refutation.** The obvious
    alternative to a config line is a runtime guard — ask tmux whether the client is
    focused and behave accordingly — and `#{client_flags}` is the format that looks like
    the answer. It is not: it reports `focused` whether or not focus events are being
    delivered, so a guard written against it passes with the feature dead. Measured on
    3.7c and on 3.2 (`tmuxctl.FLOOR`, built from the release tarball and run) with one
    attached pty client and the option flipped underneath it, all three readings were
    `attached,focused,UTF-8`. That test asserts the flags are IDENTICAL across the flip,
    which is the property that makes them useless as a guard — an `assertIn("focused")`
    alone would pass on a tmux where the flag had started telling the truth.

    The third pins the SCOPE, and exists so nobody "corrects" `-g` back to the
    `set -t <session>` the Phase 2 plan asked for. `focus-events` is a server option on
    both versions; a session-scoped write of it lands on every session on the server, so
    the `-t` spelling would read as containment while containing nothing. `mouse` is the
    control: run through the same probe it leaves the sibling untouched, which is what a
    genuinely session-scoped option looks like.

    Its own server, its own socket, and no attached client except in the one test that
    needs one (`_NeedsAttachedClient` skips where tmux will not attach one at all).
    """

    def setUp(self) -> None:
        super().setUp()
        # kill-server THEN unlink, registered so LIFO runs them in that order — see
        # `PanelIntegration._teardown_socket`'s own docstring. `conf_text` emits
        # `set -g remain-on-exit on`, which this class sources for real, so its panes
        # outlive their commands exactly as `MenuClientIntegration`'s do.
        self.addCleanup(self._teardown_socket)

    def _teardown_socket(self) -> None:
        _tmux("kill-server")
        (Path("/tmp") / f"tmux-{os.getuid()}" / SOCKET).unlink(missing_ok=True)

    def _new_session(self, fid: str) -> None:
        """A `cat`-backed session with its pane's own pid registered for cleanup —
        `kill-server` alone is documented in `_kill_pid` as unreliable at reaping one."""
        r = _tmux("new-session", "-d", "-s", fid, "-x", "80", "-y", "24", "cat")
        self.assertEqual(r.returncode, 0, r.stderr)
        pid = _tmux("display-message", "-p", "-t", fid, "#{pane_pid}").stdout.strip()
        self.addCleanup(_kill_pid, pid)

    def _source_conf(self, fid: str) -> None:
        """`commands_frame.conf_text`'s own output, `source-file`'d for real — byte for
        byte, never a hand-retyped `set`. The point is that the PRODUCTION text is what
        turns the option on."""
        text = commands_frame.conf_text(hotkey="F2", mouse=False, history_limit=1,
                                        session=fid)
        conf = Path(tempfile.mkdtemp(prefix="charter-integ-focusconf-")) / "tmux.conf"
        self.addCleanup(shutil.rmtree, conf.parent, True)
        conf.write_text(text)
        r = _tmux("source-file", str(conf))
        self.assertEqual(r.returncode, 0, r.stderr)

    @staticmethod
    def _focus_events(target: str) -> str:
        return _tmux("show-options", "-t", target, "focus-events").stdout.strip()

    def test_charters_own_conf_text_turns_focus_events_on_for_real(self):
        """The behavioural pin for `set -g focus-events on`: tmux reports the option OFF
        before charter's config is sourced and ON after it, with nothing hand-retyped in
        between.

        The BEFORE half is read from tmux rather than assumed, per this module's rule
        against asserting on a version string — a tmux that had changed its own default
        would make this test say so instead of quietly passing on a coincidence.
        """
        fid = "focus-conf"
        self._new_session(fid)
        self.assertEqual(self._focus_events(fid), "focus-events off",
                         "this tmux does not ship focus-events off, so what the line "
                         "below proves is no longer what it was written to prove")
        self._source_conf(fid)
        self.assertEqual(self._focus_events(fid), "focus-events on",
                         "charter's own frame config left focus-events off — §4f's "
                         "`focus`/`blur` event kinds do not exist without it")

    def test_client_flags_cannot_serve_as_the_guard_this_line_replaces(self):
        """`#{client_flags}` LIES, so no runtime guard can stand in for the config line.

        It reads `focused` with focus events being delivered and `focused` with them
        dead. Asserted as an EQUALITY across the flip rather than as "the string contains
        focused": the defect is that the flag cannot distinguish the two states, and only
        comparing them can catch a tmux where it started to.
        """
        fid = "focus-flags"
        self._new_session(fid)
        self._attach_pty(fid)

        def flags() -> str:
            r = _tmux("list-clients", "-t", fid, "-F", "#{client_flags}")
            self.assertEqual(r.returncode, 0, r.stderr)
            return r.stdout.strip()

        self.assertEqual(_tmux("set", "-g", "focus-events", "off").returncode, 0)
        dead = flags()
        self.assertEqual(_tmux("set", "-g", "focus-events", "on").returncode, 0)
        live = flags()
        self.assertEqual(_tmux("set", "-g", "focus-events", "off").returncode, 0)
        dead_again = flags()

        self.assertTrue(dead, "no client was attached, so nothing was measured")
        self.assertIn("focused", dead,
                      "this tmux stopped reporting `focused` while focus events were "
                      "off — the flag may have become honest; re-measure before relying "
                      "on it")
        self.assertEqual(dead, live,
                         "`#{client_flags}` now distinguishes focus-events on from off; "
                         "the reason `conf_text` sets the option rather than probing a "
                         "flag no longer holds and should be re-argued")
        self.assertEqual(live, dead_again)

    def test_focus_events_is_a_server_option_so_a_session_scoped_write_would_not_contain_it(self):
        """Why the line is `-g` and not the `set -t <session>` the plan asked for.

        Two sessions on one server. Setting `focus-events` for one sets it for BOTH,
        because it lives in tmux's server table — so the `-t` spelling would read as
        containment while containing nothing, in a config whose first three lines are
        session-scoped precisely so one frame cannot rewrite another's. `mouse` is the
        control that stops this test passing on a broken probe: run through exactly the
        same two calls it leaves the sibling reading empty, which is what a genuinely
        session-scoped option does.
        """
        self._new_session("focus-one")
        self._new_session("focus-two")
        self.assertIn("focus-events", _tmux("show-options", "-s").stdout,
                      "focus-events is no longer a tmux SERVER option on this version — "
                      "`conf_text`'s `-g` should be re-argued against a per-session form")

        self.assertEqual(_tmux("set", "-t", "focus-one", "focus-events", "on").returncode, 0)
        self.assertEqual(self._focus_events("focus-two"), "focus-events on",
                         "a session-scoped focus-events write did NOT reach the sibling, "
                         "so tmux grew a per-session form and `conf_text` can use it")

        self.assertEqual(_tmux("set", "-t", "focus-one", "mouse", "on").returncode, 0)
        self.assertEqual(
            _tmux("show-options", "-t", "focus-two", "mouse").stdout.strip(), "",
            "the control failed: even `mouse`, which conf_text relies on being "
            "session-scoped, reached the sibling here — this probe measures nothing")


class _ChatsOnOneSession:
    """Opening real chats as real windows of one workspace's session.

    A mixin rather than a base class with tests in it, for `_TmuxServerFixture`'s own
    reason: `unittest` collects an inherited test as the subclass's own, so a second
    class about chats would re-run every one of Stage 5a's against its own server for
    nothing. What is shared is the FIXTURE — the production builders, the cleanup, and
    the two readings every chat test needs — and nothing else.
    """

    #: The workspace these chats belong to — the tmux SESSION's name, and the part of each
    #: chat id before the dot. A literal, so a test cannot be satisfied by whatever
    #: `state.workspace_prefix` happens to do with it.
    WS = "wsdemo"

    def _chat(self, chat: str, *, first: bool, dies_by: str = "exit 0") -> tuple[str, str]:
        """Open *chat* on this class's server and return its pane id and gate.

        The FIRST chat of a workspace starts the session (`layout.session_argv`); every
        later one joins it (`layout.chat_window_argv`). Both are the production builders,
        never a hand-retyped command, so this measures the argv charter really sends.
        """
        gate = os.path.join(self._gate_dir, f"gate-{chat}")
        argv = _gate_argv(gate, dies_by)
        if first:
            conf = os.path.join(self._gate_dir, f"{chat}.conf")
            Path(conf).write_text(commands_frame._PLACEHOLDER_CONF)
            cmd = layout.session_argv(session=self.WS, conf=conf, chat=chat,
                                      socket=self.SOCKET_NAME, cols=80, rows=24,
                                      harness_argv=argv,
                                      env={"CHARTER_SESSION_ID": chat})
        else:
            cmd = layout.chat_window_argv(socket=self.SOCKET_NAME, session=self.WS,
                                          chat=chat, cwd=self._gate_dir,
                                          harness_argv=argv,
                                          env={"CHARTER_SESSION_ID": chat})
        r = _run(cmd)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self.assertTrue(pane.startswith("%"), f"tmux reported no pane id: {r.stdout!r}")
        self.addCleanup(_kill_pid, self._pane_pid_on(pane))
        named = commands_frame._chat_option_argv(socket=self.SOCKET_NAME,
                                                 harness_pane=pane, chat=chat)
        self.assertIsNotNone(named, f"charter refused to name the chat {chat!r}")
        self.assertEqual(_run(named).returncode, 0)
        # `state.reap` only ever removes a directory whose server it can match, so the
        # marker is what makes the reaping tests below about liveness rather than about
        # the migration case.
        state.record_server(chat, self.SOCKET_NAME)
        state.record_harness_pane(chat, pane)
        state.bump(chat)
        return pane, gate

    def _window_names(self) -> list[str]:
        return self._srv("list-windows", "-a", "-F",
                         "#{window_name}").stdout.split()

    def _pane_pid_on(self, target: str) -> str:
        return self._srv("display-message", "-p", "-t", target,
                         "#{pane_pid}").stdout.strip()

    def _wait_until(self, predicate, timeout=_DEADLINE) -> bool:
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if predicate():
                return True
            time.sleep(0.05)
        return False


class TheChatALaunchIsLeavingIsReadFromTmux(_ChatsOnOneSession, _TmuxServerFixture,
                                            PersonaIso):
    """#688: which chat the client is on, and whether charter can name it without naming
    a session.

    **A mock cannot make either claim.** `#{window_active}` is tmux's own word for "the
    session's current window", and `-t <session name>` is how a target gets parsed — a
    workspace called `api.2` is read as ``window.pane``, so a question aimed that way lands
    on somebody else's window or on none. The reading here is `list-windows -a` over ids
    and an option, and this is where that is measured rather than argued.

    Verified on tmux 3.7c and on tmux 3.2 (`tmuxctl.FLOOR`); the answers were identical,
    so nothing here carries a version gate.
    """

    def test_the_active_window_is_the_one_the_session_is_on(self):
        one, _ = self._chat(f"{self.WS}.1", first=True)
        two, _ = self._chat(f"{self.WS}.2", first=False)
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside=f"{self.WS}.2"),
            f"{self.WS}.1",
            "the chat a second launch would be leaving was not the one on screen")
        self._srv("select-window", "-t", two)
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside=f"{self.WS}.1"),
            f"{self.WS}.2",
            "`#{window_active}` did not follow the select on this tmux, so the whole "
            "reading is measuring something else")
        self.assertTrue(one)

    def test_the_only_chat_of_a_session_answers_nothing(self):
        """A launch that CREATED the session has one window and it is its own — there is
        nothing to leave, and the `chat != beside` filter is what says so."""
        self._chat(f"{self.WS}.1", first=True)
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside=f"{self.WS}.1"), "")

    def test_a_chat_in_another_session_is_never_the_one_being_left(self):
        """The session comes from the new chat's OWN row rather than from a name, so a
        window that is current in a different session cannot be mistaken for this one's."""
        self._chat(f"{self.WS}.1", first=True)
        r = self._srv("new-session", "-d", "-s", "elsewhere", "-P", "-F",
                      "#{pane_id}", "--", *_gate_argv(
                          os.path.join(self._gate_dir, "gate-elsewhere"), "exit 0"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(r.stdout.strip()))
        named = commands_frame._chat_option_argv(socket=self.SOCKET_NAME,
                                                 harness_pane=r.stdout.strip(),
                                                 chat="other.1")
        self.assertEqual(_run(named).returncode, 0)
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside=f"{self.WS}.1"), "")

    def test_a_workspace_whose_name_holds_a_dot_is_still_answered(self):
        """The reason nothing here is a `-t <session>`: tmux parses a target on `.` as
        ``window.pane``, so `display-message -t api.2` resolves to a pane INDEX of some
        other window rather than to the session called `api.2`. Measured on 3.7c: the
        session is found, the window is not, and the answer is whatever was current.

        This class's own reading takes no target at all, so a workspace an operator is
        perfectly entitled to name is answered like any other."""
        r = self._srv("new-session", "-d", "-s", "api.2", "-n", "api.2.1", "-P", "-F",
                      "#{pane_id}", "--", *_gate_argv(
                          os.path.join(self._gate_dir, "gate-dotted"), "exit 0"))
        self.assertEqual(r.returncode, 0, r.stderr)
        first = r.stdout.strip()
        self.addCleanup(_kill_pid, self._pane_pid_on(first))
        self.assertEqual(_run(commands_frame._chat_option_argv(
            socket=self.SOCKET_NAME, harness_pane=first, chat="api.2.1")).returncode, 0)
        # A second window of that session, made the way `_chat_being_left`'s caller would
        # have to reach it — off the FIRST window's pane id, never off the session's name.
        window = self._srv("display-message", "-p", "-t", first,
                           "#{window_id}").stdout.strip()
        r2 = self._srv("new-window", "-d", "-a", "-t", window, "-P", "-F", "#{pane_id}",
                       "--", *_gate_argv(os.path.join(self._gate_dir, "gate-dotted2"),
                                         "exit 0"))
        self.assertEqual(r2.returncode, 0, r2.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(r2.stdout.strip()))
        self.assertEqual(_run(commands_frame._chat_option_argv(
            socket=self.SOCKET_NAME, harness_pane=r2.stdout.strip(),
            chat="api.2.2")).returncode, 0)
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside="api.2.2"),
            "api.2.1")

    def test_a_window_carrying_no_chat_is_not_one_to_tear_down(self):
        """The operator's-tmux case: the window they were on is one of THEIRS and carries
        no `@charter_chat` at all, so this answers `""` and nothing of theirs is touched."""
        pane, _ = self._chat(f"{self.WS}.1", first=True)
        window = self._srv("display-message", "-p", "-t", pane,
                           "#{window_id}").stdout.strip()
        r = self._srv("new-window", "-a", "-t", window, "-P", "-F", "#{pane_id}",
                      "--", *_gate_argv(os.path.join(self._gate_dir, "gate-theirs"),
                                        "exit 0"))
        self.assertEqual(r.returncode, 0, r.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(r.stdout.strip()))
        self.assertEqual(
            commands_frame._chat_being_left(self.SOCKET_NAME, beside=f"{self.WS}.1"), "",
            "charter would have torn down a window that is not a chat")

    def test_the_panels_of_the_chat_left_behind_really_stop(self):
        """The other half, end to end against real panes: `_drop_panels` kills them and
        rewrites the map, so nothing is left rendering at a width its window no longer
        has."""
        pane, _ = self._chat(f"{self.WS}.1", first=True)
        panels = {}
        for slot in ("top", "bottom"):
            r = self._srv("split-window", "-d", "-t", pane, "-P", "-F", "#{pane_id}",
                          "--", *_gate_argv(
                              os.path.join(self._gate_dir, f"gate-{slot}"), "exit 0"))
            self.assertEqual(r.returncode, 0, r.stderr)
            panels[slot] = r.stdout.strip()
        state.record_panes(f"{self.WS}.1", panels=panels)
        commands_frame._drop_panels(self.SOCKET_NAME, f"{self.WS}.1")
        alive = self._srv("list-panes", "-t", pane, "-F", "#{pane_id}").stdout.split()
        for slot, panel in panels.items():
            self.assertNotIn(panel, alive, f"the {slot} panel is still running")
        self.assertEqual(state.panes(f"{self.WS}.1"), {})


class AWorkspaceNamedWithADotIsStillAWorkspace(_TmuxServerFixture, PersonaIso,
                                              unittest.TestCase):
    """#695, against a real server: a session NAME is not a string tmux reads as a string.

    `instance.WORKSPACE_NAME_RE` accepts a dot, so `api.2` is a workspace
    `charter workspace create` takes — and `state.workspace_prefix` turns a workspace into
    the tmux SESSION name. Two facts about tmux, neither of which a mock can produce, and
    which disagree with each other:

    * **3.7c keeps the dot and then splits on it.** `-t api.2` is `window.pane`, so
      `new-window` answers `can't specify pane here` rc 1 and `set-environment` answers rc
      **0** onto a sibling session called `api`.
    * **3.2 — `tmuxctl.FLOOR` — does not keep it at all.** `new-session -s api.2` creates
      a session named `api_2`, so charter asked for one name and got another and every
      later target missed.

    That disagreement is the whole reason the fix is in what charter MINTS rather than in
    how it spells a target: a trailing `:` disambiguates on 3.7c and finds nothing on 3.2.
    So this class asserts the property both versions can hold — the identifier charter
    derives is one tmux gives back unchanged, and the frame works.

    The sibling session called `api` is not decoration: on 3.7c it is what `-t api.2`
    resolves TO once tmux has split the name, so without it the wrong target would merely
    fail instead of quietly working on another workspace's frame.
    """

    WS = "api.2"
    SIBLING = "api"

    def _session(self, name: str) -> str:
        r = self._srv("new-session", "-d", "-s", name, "-n", f"{name}.1", "-P", "-F",
                      "#{pane_id}", "--", *_gate_argv(
                          os.path.join(self._gate_dir, f"gate-{name}"), "exit 0"))
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self.addCleanup(_kill_pid, self._srv("display-message", "-p", "-t", pane,
                                             "#{pane_pid}").stdout.strip())
        return pane

    def setUp(self) -> None:
        super().setUp()
        self.prefix = state.workspace_prefix(self.WS)
        self.pane = self._session(self.prefix)
        self._session(self.SIBLING)

    def test_tmux_gives_back_the_name_charter_asked_for(self):
        """The property both versions can hold, and the one a dotted name cannot. Read out
        of `list-sessions` rather than assumed: on 3.2 a dotted name comes back rewritten,
        which is how charter came to target a session that was never there."""
        self.assertNotIn(".", self.prefix,
                         "charter is still minting a dot into a tmux session name")
        names = self._srv("list-sessions", "-F", "#{session_name}").stdout.split()
        self.assertIn(self.prefix, names)

    def test_a_second_chat_can_be_opened_in_it(self):
        """The reachable defect. `layout.chat_window_argv` is the production builder, and
        with a dotted session name it answered `can't specify pane here` rc 1 on 3.7c — so
        `charter claude` a second time in that workspace returned 1 with nothing to explain
        it, and the workspace was a one-chat workspace for good."""
        chat = f"{self.prefix}.2"
        cmd = layout.chat_window_argv(socket=self.SOCKET_NAME, session=self.prefix,
                                      chat=chat, cwd=self._gate_dir,
                                      harness_argv=_gate_argv(
                                          os.path.join(self._gate_dir, "gate-2"),
                                          "exit 0"))
        r = _run(cmd)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.addCleanup(_kill_pid, self._srv("display-message", "-p", "-t",
                                             r.stdout.strip(),
                                             "#{pane_pid}").stdout.strip())
        self.assertEqual(
            sorted(self._srv("list-windows", "-t", self.prefix,
                             "-F", "#{window_name}").stdout.split()),
            sorted([f"{self.prefix}.1", chat]))

    def test_the_frames_identity_lands_on_its_own_session(self):
        """`$CHARTER_SESSION_ID` is what the hotkey bind and every palette action resolve
        themselves from. With a dotted session name `set-environment` answered rc **0** and
        wrote it on session `api` — another workspace's frame, told it was this one."""
        cmd = commands_frame._session_id_env_argv(socket=self.SOCKET_NAME,
                                                  session=self.prefix,
                                                  chat=f"{self.prefix}.1")
        self.assertEqual(_run(cmd).returncode, 0)
        mine = self._srv("show-environment", "-t", self.prefix, "CHARTER_SESSION_ID")
        self.assertEqual(mine.stdout.strip(), f"CHARTER_SESSION_ID={self.prefix}.1")
        theirs = self._srv("show-environment", "-t", self.SIBLING, "CHARTER_SESSION_ID")
        self.assertNotEqual(theirs.returncode, 0,
                            "this frame's identity was written on another workspace's "
                            "session")

    def test_the_reattach_line_charter_prints_is_one_that_works(self):
        """The launcher's own advice, run. An operator who cannot reattach to a detached
        agent has lost it, and a hint that fails is worse than none."""
        self.assertEqual(self._srv("has-session", "-t", self.prefix).returncode, 0)

    def test_a_chat_id_carries_exactly_one_dot_and_it_is_the_ordinal(self):
        """What the mint buys beyond the target: the only dot left in a chat id is
        `state._CHAT_SEP`, so `chats._order`'s `rpartition` reads an ordinal rather than
        whichever dot happened to be last."""
        chat = state.new_chat_id(self.WS)
        self.assertEqual(chat, f"{self.prefix}.1")
        self.assertEqual(chat.count("."), 1)


class ChatsAreWindowsOnOneWorkspaceSession(_ChatsOnOneSession, _TmuxServerFixture,
                                           PersonaIso):
    """Phase 5 Stage 5a, against a real server: a workspace is a SESSION and a chat is a
    WINDOW in it.

    Every claim here is one a mock cannot make. Whether `new-window -n` pins a name and
    whether a pane can take it back; whether a window user option survives that; whether
    a pane-scoped `pane-died[1] kill-window` leaves its siblings alone and still ends the
    session when it is the last window; and whether a per-window `-e` really beats a
    session-wide `set-environment` in the pane's own environment.

    **Re-run against tmux 3.2 — `tmuxctl.FLOOR` — as well as 3.7c**, by putting a 3.2
    built from the release tarball first on `$PATH` and running this module: every
    assertion below passed identically on both, so nothing here carries a version gate.
    The two versions' answers are quoted in the tests that turn on them.
    """

    def test_two_chats_in_one_workspace_are_two_windows_on_one_session(self):
        """The shape, end to end. One session named for the workspace, one window per
        chat, and each window answering for its own chat through `@charter_chat`."""
        self._chat("wsdemo.1", first=True)
        self._chat("wsdemo.2", first=False)
        # A window that is NOT a chat, on the same server — the operator's own, or one
        # tmux made for a reason charter had nothing to do with. It carries no
        # `@charter_chat`, so `list-windows -F` prints an EMPTY line for it, and an empty
        # string in the live set is a name `state.reap` would compare every directory
        # against.
        stray = self._srv("new-window", "-d", "-t", self.WS, "-P", "-F", "#{pane_id}",
                          "sleep", "600")
        self.assertEqual(stray.returncode, 0, stray.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(stray.stdout.strip()))

        self.assertEqual(self._srv("list-sessions", "-F", "#{session_name}").stdout.split(),
                         [self.WS])
        names = self._window_names()
        self.assertIn("wsdemo.1", names)
        self.assertIn("wsdemo.2", names)
        self.assertEqual(commands_frame._live_chats(self.SOCKET_NAME),
                         {"wsdemo.1", "wsdemo.2"})

    def test_reap_keeps_both_chats_while_their_windows_exist_and_no_launcher_does(self):
        """The bounding rule, with the pid rule deliberately unable to help: a chat id
        carries no launcher pid, so the liveness list is the only thing keeping these
        directories. Both are kept while their windows are there; kill one window and
        exactly that one is removed."""
        self._chat("wsdemo.1", first=True)
        pane_two, _ = self._chat("wsdemo.2", first=False)
        for chat in ("wsdemo.1", "wsdemo.2"):
            self.assertIsNone(state._launcher_pid(chat),
                              "a chat id must carry no pid, or this test is measuring "
                              "the rule it exists to replace")

        live = commands_frame._live_chats(self.SOCKET_NAME)
        self.assertEqual(state.reap(live, server=self.SOCKET_NAME), [])
        self.assertTrue(state.frame_dir("wsdemo.1").exists())
        self.assertTrue(state.frame_dir("wsdemo.2").exists())

        self.assertEqual(self._srv("kill-window", "-t", pane_two).returncode, 0)
        live = commands_frame._live_chats(self.SOCKET_NAME)
        self.assertEqual(live, {"wsdemo.1"})
        self.assertEqual(state.reap(live, server=self.SOCKET_NAME), ["wsdemo.2"])
        self.assertTrue(state.frame_dir("wsdemo.1").exists())

    def test_a_pane_can_take_its_windows_name_and_not_its_chat(self):
        """**The measurement that decides where liveness is read from.**

        `new-window -n` pins the name — it turns that window's `automatic-rename` off —
        but with `allow-rename on` the pane's own output takes it anyway. Measured here,
        and identically on tmux 3.7c and tmux 3.2: after the pane writes
        `\033kPWNED\033\\`, `#{window_name}` is `PWNED` while `#{@charter_chat}` is
        untouched. `automatic-rename` is ON by default too, so a window charter did not
        name follows whatever runs in it.

        A liveness list read from `#{window_name}` therefore loses this chat, and
        `state.reap` — the only bound on `.charter/frame/` — deletes the state of a chat
        that is still running. `_live_chats` reads the option, and this test is what says
        the two are not the same fact.
        """
        pane, _ = self._chat("wsdemo.1", first=True)
        window = self._srv("display-message", "-p", "-t", pane,
                           "#{window_id}").stdout.strip()
        self.assertEqual(self._srv("show", "-w", "-t", window,
                                   "automatic-rename").stdout.strip(),
                         "automatic-rename off",
                         "`-n` no longer pins a window's name on this tmux")
        self.assertEqual(self._srv("set", "-w", "-t", window,
                                   "allow-rename", "on").returncode, 0)
        self.assertEqual(self._srv("respawn-window", "-k", "-t", window,
                                   "sh -c 'printf \"\\033kPWNED\\033\\\\\"; "
                                   "sleep 600'").returncode, 0)
        renamed = self._wait_until(lambda: "PWNED" in self._window_names())
        self.assertTrue(renamed,
                        "the pane did not manage to rename its window — this tmux may "
                        "have stopped honouring `allow-rename`, in which case the risk "
                        "this test measures is gone and the claim should be re-argued")
        self.addCleanup(_kill_pid, self._pane_pid_on(window))

        self.assertNotIn("wsdemo.1", self._window_names())
        self.assertEqual(commands_frame._live_chats(self.SOCKET_NAME), {"wsdemo.1"},
                         "the chat's identity went with its name — reaping would now "
                         "delete a running chat's state")

    def _arm_the_production_pair(self, pane: str) -> None:
        """Both `pane-died` hooks, in `cmd_launch`'s own order and from its own builders.

        **No `_arm_array_probe` beside them, and that is not an oversight.**
        :data:`_PROBE_INDEX` is `1`, which is exactly where the teardown hook lands, so a
        probe here would REPLACE the thing under test and the test would be measuring its
        own probe. The window disappearing is this pair's own evidence, and a trial where
        it does not is classified by `_the_array_never_ran` on the pane itself.
        """
        self.assertEqual(_run(commands_frame._pane_died_write_hook_argv(
            socket=self.SOCKET_NAME, harness_pane=pane)).returncode, 0)
        self.assertEqual(_run(commands_frame._pane_died_teardown_hook_argv(
            socket=self.SOCKET_NAME, harness_pane=pane)).returncode, 0)

    def test_one_chats_harness_dying_leaves_every_other_chat_running(self):
        """`pane-died[1]` runs `kill-window`, and this is what that buys.

        Two chats, one workspace, real hooks armed exactly as `cmd_launch` arms them.
        The first chat's harness dies; its window goes and NOTHING else does — the
        sibling's window is still listed, its pane is not dead, and the session is still
        there. With `kill-session` in that hook (which is what shipped while a frame WAS a
        session) the sibling's harness goes down mid-turn for a death that was not its
        own. Measured identically on tmux 3.7c and tmux 3.2.

        Retried like every death-dependent test here, and for #487's reason rather than
        for flakiness: a death tmux never ran the array for measured nothing."""
        self._require_pane_died_fires()
        for attempt in range(self._HOOK_TRIALS):
            dying, live = f"wsdemo.{attempt}1", f"wsdemo.{attempt}2"
            one, gate = self._chat(dying, first=(attempt == 0), dies_by="exit 9")
            two, _ = self._chat(live, first=False)
            self._arm_the_production_pair(one)
            self._release(gate)
            if not self._wait_until(lambda: dying not in self._window_names()):
                self._the_array_never_ran(
                    one, "the dying chat's window is still there")
                continue
            self.assertIn(live, self._window_names(),
                          "a sibling chat's window went down with the one that died")
            self.assertEqual(self._srv("display-message", "-p", "-t", two,
                                       "#{pane_dead}").stdout.strip(), "0",
                             "the sibling chat's harness was killed")
            # `assertIn`, not equality: `_require_pane_died_fires` leaves its own probe
            # session on this server, and it is not what this test is about.
            self.assertIn(
                self.WS,
                self._srv("list-sessions", "-F", "#{session_name}").stdout.split(),
                "the workspace's session went down with one of its chats")
            return
        self.skipTest("every trial was a death tmux never ran the array for")

    def test_the_last_chats_teardown_still_ends_the_session(self):
        """The half that keeps the single-chat case unchanged: killing a session's LAST
        window destroys the session, so `cmd_launch`'s `attach` returns exactly as it did
        when the teardown said `kill-session`. Measured on tmux 3.7c and tmux 3.2."""
        self._require_pane_died_fires()
        for attempt in range(self._HOOK_TRIALS):
            chat = f"wsdemo.{attempt}"
            pane, gate = self._chat(chat, first=True, dies_by="exit 3")
            self._arm_the_production_pair(pane)
            self._release(gate)
            if self._wait_until(lambda: self.WS not in self._srv(
                    "list-sessions", "-F", "#{session_name}").stdout.split()):
                return
            self._the_array_never_ran(
                pane, "the workspace's session outlived its last chat, so a launcher's "
                      "`attach` would never return")
        self.skipTest("every trial was a death tmux never ran the array for")

    def test_each_chat_reports_its_own_identity_over_a_session_wide_one(self):
        """Task 3's whole mechanism, measured. A per-window `-e` beats a session-wide
        `set-environment` inside the pane — `chat-A claude-code` and `chat-B codex` on
        tmux 3.7c and on tmux 3.2 alike, under a session that says `session-wide`.

        This is what makes `.charter/sessions/<chat id>.persona`, `.workspace`, `.tools`
        and `.gate` per chat with no new code: `session.current()` reads
        `$CHARTER_SESSION_ID`, and every process in a chat's pane gets the chat's own.
        """
        out = os.path.join(self._gate_dir, "identity")
        os.makedirs(out, exist_ok=True)
        conf = os.path.join(self._gate_dir, "identity.conf")
        Path(conf).write_text(commands_frame._PLACEHOLDER_CONF)
        first = _run(layout.session_argv(
            session=self.WS, conf=conf, socket=self.SOCKET_NAME, cols=80, rows=24,
            harness_argv=["sleep", "600"],
            env={"CHARTER_SESSION_ID": "wsdemo.1", "CHARTER_HARNESS": "claude-code"}))
        self.assertEqual(first.returncode, 0, first.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(first.stdout.strip()))
        # The session says something else entirely, and says it BEFORE the windows are
        # created — which is the order `cmd_launch` uses and the order that makes the
        # override a real one rather than a last-writer-wins accident.
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=self.SOCKET_NAME, session=self.WS,
            chat="session-wide")).returncode, 0)

        for chat, harness in (("wsdemo.2", "codex"), ("wsdemo.3", "opencode")):
            cmd = layout.chat_window_argv(
                socket=self.SOCKET_NAME, session=self.WS, chat=chat,
                cwd=self._gate_dir,
                # ONE write, then an atomic rename: two appends make the file non-empty
                # after the first, and `_await_text` stops at non-empty — measured, that
                # read back `wsdemo.3` with the harness line still in flight.
                harness_argv=["/bin/sh", "-c",
                              f'printf "%s\n%s\n" "$CHARTER_SESSION_ID" '
                              f'"$CHARTER_HARNESS" > {out}/{chat}.part && '
                              f"mv {out}/{chat}.part {out}/{chat}; sleep 600"],
                env={"CHARTER_SESSION_ID": chat, "CHARTER_HARNESS": harness})
            r = _run(cmd)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.addCleanup(_kill_pid, self._pane_pid_on(r.stdout.strip()))

        for chat, harness in (("wsdemo.2", "codex"), ("wsdemo.3", "opencode")):
            with self.subTest(chat=chat):
                self.assertEqual(_await_text(os.path.join(out, chat)),
                                 f"{chat}\n{harness}",
                                 "a chat's pane read the session's identity instead of "
                                 "its own")

    def test_the_session_wide_value_is_what_a_run_shell_child_reads(self):
        """The other side of the same measurement, and the reason `conf_text`'s binds
        carry `#{@charter_chat}` rather than trusting the variable.

        A bind's action is a `run-shell`, and a `run-shell` child reads the SESSION's
        environment — never the window's `-e`. Measured on tmux 3.7c and on tmux 3.2: the
        pane above reports its own chat, and this reports `session-wide`. So the variable
        cannot tell two chats of one workspace apart, and the window option is what does.
        """
        conf = os.path.join(self._gate_dir, "runshell.conf")
        Path(conf).write_text(commands_frame._PLACEHOLDER_CONF)
        first = _run(layout.session_argv(
            session=self.WS, conf=conf, socket=self.SOCKET_NAME, cols=80, rows=24,
            harness_argv=["sleep", "600"], env={"CHARTER_SESSION_ID": "wsdemo.1"}))
        self.assertEqual(first.returncode, 0, first.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(first.stdout.strip()))
        self.assertEqual(_run(commands_frame._session_id_env_argv(
            socket=self.SOCKET_NAME, session=self.WS,
            chat="session-wide")).returncode, 0)
        seen = os.path.join(self._gate_dir, "runshell-saw")
        self.assertEqual(self._srv(
            "run-shell", "-b", "-t", self.WS,
            f"printenv CHARTER_SESSION_ID > {seen}").returncode, 0)
        self.assertEqual(_await_text(seen), "session-wide")


class ASwitchDressesTheWindowItEntersEvenWithNothingToSplit(_ChatsOnOneSession,
                                                            _TmuxServerFixture,
                                                            PersonaIso):
    """#686, against a real server: the options are *present on the window* afterwards.

    The argv half is `tests/test_frame_border_surface.py`'s. What only a real tmux can say
    is that the writes landed — nothing here is server- or session-scoped, so every chat's
    window has to be told separately, and until this fix the only thing that ever told one
    was that chat's own launch.

    **The fixture is the one a mock cannot plant honestly**: a chat holding every panel it
    wants, with the options stripped off it. That is the shipping state of a chat surfaced
    by a charter predating #657/#631 and re-entered while its panels are alive, and it is
    also one `charter claude` away on this charter — the second launch selects its own new
    window and leaves the first's panels running and recorded.

    Verified on tmux 3.7c and on tmux 3.2 (`tmuxctl.FLOOR`). Which options exist differs
    between them — `pane-border-indicators` is 3.7's — so this asserts about the ones THIS
    tmux has rather than about a list, the way `EveryBorderOptionThisTmuxHasIsPinned`
    already does.
    """

    def _has(self, name: str) -> bool:
        """Does this tmux have *name* as a window option at all? Probed, never inferred
        from a version string — this module's own rule."""
        return self._srv("set-option", "-w", "-t", self.pane, name,
                         "off").returncode == 0

    def _shown(self, scope: str, name: str) -> str:
        return self._srv("show-options", scope, "-t", self.pane,
                         name).stdout.strip()

    def setUp(self) -> None:
        super().setUp()
        self.one, _ = self._chat(f"{self.WS}.1", first=True)
        self.pane, _ = self._chat(f"{self.WS}.2", first=False)
        for chat in (f"{self.WS}.1", f"{self.WS}.2"):
            state.record_workspace(chat, self.WS)
            state.record_chrome(chat, "dark")
        # Every drawable panel already split into the target's window — the branch
        # `if missing:` skipped, and the one no existing switch test plants.
        want = commands_frame._drawable_slots(
            80, 24, commands_frame._visible_now(f"{self.WS}.2", config.FRAME))
        self.assertTrue(want, "nothing is drawable at this size, so nothing is measured")
        panels = {}
        for slot in want:
            r = self._srv("split-window", "-d", "-t", self.pane, "-P", "-F",
                          "#{pane_id}", "--", *_gate_argv(
                              os.path.join(self._gate_dir, f"gate-{slot}"), "exit 0"))
            self.assertEqual(r.returncode, 0, r.stderr)
            panels[slot] = r.stdout.strip()
        state.record_panes(f"{self.WS}.2", panels=panels)
        self.kept = panels
        self.names = [n for n, _ in commands_frame._CHROME if self._has(n)]
        self.assertTrue(self.names, "this tmux has none of the options under test")
        for name in self.names:
            self._srv("set-option", "-w", "-u", "-t", self.pane, name)
        for name in instance.PANE_BORDER_OPTIONS:
            self._srv("set-option", "-p", "-u", "-t", self.pane, name)

    def _switch(self) -> None:
        with mock.patch.object(commands_frame, "SOCKET", self.SOCKET_NAME), \
             mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: self.fail(msg)), \
             mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": f"{self.WS}.1",
                              "CHARTER_WORKSPACE": self.WS}, clear=False):
            self.assertEqual(commands_frame.cmd_chat(
                SimpleNamespace(chat_id=f"{self.WS}.2", chat=f"{self.WS}.1")), 0)

    def test_the_fixture_really_has_nothing_left_to_split(self):
        """Without this the class would be measuring the branch that already worked."""
        before = self._srv("list-panes", "-t", self.pane, "-F", "#{pane_id}").stdout.split()
        self._switch()
        self.assertEqual(
            self._srv("list-panes", "-t", self.pane, "-F", "#{pane_id}").stdout.split(),
            before, "the switch split or killed something, so `missing` was not empty")

    def test_the_windows_own_options_are_set_after_the_switch(self):
        self.assertEqual([self._shown("-w", n) for n in self.names], [""] * len(self.names),
                         "the fixture never stripped the options")
        self._switch()
        for name in self.names:
            with self.subTest(option=name):
                self.assertTrue(self._shown("-w", name),
                                f"{name} is still unset on the window charter just "
                                f"switched into")

    def test_the_rules_around_the_harness_are_set_after_the_switch(self):
        """#657's two `-p` writes — the ones the reported screenshot is about."""
        self._switch()
        self.assertTrue([self._shown("-p", n) for n in instance.PANE_BORDER_OPTIONS
                         if self._shown("-p", n)],
                        "the rules around the pane charter does not paint were never "
                        "re-asserted, so a horizontal rule stays two colours")

    def test_the_harnesss_interior_is_still_never_painted(self):
        """#657's own design point, kept while making its rules apply: the harness's
        RULES are charter's and its INTERIOR is the agent's (ADR 0018). A re-layout that
        now dresses the window on every pass must not start painting inside it."""
        self._switch()
        for name in instance.chrome_option_names():
            with self.subTest(option=name):
                self.assertEqual(self._shown("-p", name), "",
                                 f"charter painted the harness pane's {name}")


class SwitchingBetweenChatsMovesTheClientAndThePanes(_ChatsOnOneSession,
                                                     _NeedsAttachedClient,
                                                     _TmuxServerFixture, PersonaIso):
    """Phase 5 Stage 5b, Task 4: every claim the switch rests on, against a REAL attached
    client whose size this test changes.

    **A mock cannot make any of these**, which is why the module's own fake server is
    only ever asked about ARGV and ORDER (`tests/test_frame_chat_switch.py`). Whether a
    background window keeps stale geometry, whether `select-window` corrects it by the
    time the very next command runs, whether a pane split before that switch is born at
    the wrong width, and whether `select-pane` on another window's pane drags the client
    with it — all four are facts about tmux, and the design is wrong if any of them is
    not what it was measured to be.

    **Re-run against tmux 3.2 — `tmuxctl.FLOOR` — as well as 3.7c**, by putting a 3.2
    built from the release tarball first on `$PATH`. Every assertion below was identical
    on both, so nothing here carries a version gate; the two numbers are quoted in the
    tests that turn on them.
    """

    #: The client's size before and after the resize each test makes. Two shapes rather
    #: than one because the whole property is that a background window keeps the FIRST
    #: while the client is at the SECOND — so a single size would measure nothing.
    BIG = (200, 50)
    SMALL = (100, 30)

    def setUp(self) -> None:
        super().setUp()
        # **The panels this class's switch splits are REAL `charter panel` children**, and
        # `layout.panel_command` builds their argv with `-P` (#390) — so this checkout has
        # to reach them on `$PYTHONPATH`, exactly as `PanelIntegration` and
        # `FourEdgeIntegration` already arrange for their own.
        #
        # Set into `os.environ` and set HERE, before this class's first tmux command, and
        # both halves matter. A live re-layout hands `tmuxctl.run` no client environment
        # at all (`_relayout`'s `env=None`), so there is nowhere to pass one; and the
        # first tmux command is what STARTS this class's server, whose environment is what
        # every pane it later creates inherits. Set afterwards it would reach the test
        # process and no pane, and the panels would die with `No module named charter` —
        # which is what `test_a_placed_chat_bar_…` caught, because it is the only test
        # here that asserts on what a panel actually PAINTED rather than on its geometry.
        self.enterContext(mock.patch.dict(
            os.environ, {"PYTHONPATH": _importable_env(os.environ)["PYTHONPATH"]},
            clear=False))

    def _resize(self, fd: int, cols: int, rows: int) -> None:
        """Change the pty's size the way a terminal emulator does — `TIOCSWINSZ`, which
        is what makes tmux resize the client and, at the switch, the window."""
        fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def _window_of(self, pane: str) -> str:
        return self._srv("display-message", "-p", "-t", pane,
                         "#{window_id}").stdout.strip()

    def _current_window(self) -> str:
        return self._srv("display-message", "-p", "-t", f"{self.WS}:",
                         "#{window_id}").stdout.strip()

    def _size_of(self, window: str) -> tuple[int, int]:
        out = self._srv("display-message", "-p", "-t", window,
                        "#{window_width}:#{window_height}").stdout.strip()
        w, _, h = out.partition(":")
        return int(w), int(h)

    def _two_chats_and_a_client(self):
        """Two chats on one session, a real client attached at :data:`BIG`, **both**
        windows already at that size, and the client sitting on the first chat.

        Returns the two panes and the client's pty fd. The attach is
        `_NeedsAttachedClient`'s, so a machine tmux will not attach a client on skips
        rather than asserting about a switch nothing was there to see.

        **Both windows are visited once on purpose.** A window is created at the SESSION's
        size (80x24 here, `layout.session_argv`'s own), so a second chat that has never
        been selected is already stale — which would make every test below pass for the
        wrong reason, measuring "a window nobody looked at kept its birth size" instead of
        "a window that WAS correct went stale when the client moved". Selecting each once
        puts both at the client's size, so the only staleness left is the one the resize
        creates.
        """
        one, _ = self._chat(f"{self.WS}.1", first=True)
        two, _ = self._chat(f"{self.WS}.2", first=False)
        _name, fd, _screen = self._attach_pty(self.WS)
        self._resize(fd, *self.BIG)
        for pane in (two, one):
            self._srv("select-window", "-t", pane)
            self.assertTrue(
                self._wait_until(lambda p=pane: self._size_of(self._window_of(p))[0]
                                 == self.BIG[0]),
                "the client never took the size this test set, so nothing below "
                "measures what it claims to")
        self.assertEqual(self._current_window(), self._window_of(one))
        return one, two, fd

    def test_a_background_chats_window_keeps_stale_geometry_until_the_switch(self):
        """§7.4, re-measured on this tree because the whole design turns on it.

        With the client resized 200x50 → 100x30 the ACTIVE chat's window follows and the
        background chat's does not — it is still 200 columns wide. Identical on tmux 3.7c
        and tmux 3.2. That is why panels may not simply be left running in a background
        chat: they are not idle, they are rendering at a width that is not their
        window's.
        """
        one, two, fd = self._two_chats_and_a_client()
        self._resize(fd, *self.SMALL)
        self.assertTrue(
            self._wait_until(lambda: self._size_of(self._window_of(one))[0]
                             == self.SMALL[0]),
            "the active window never followed the client's resize")
        self.assertEqual(self._size_of(self._window_of(two))[0], self.BIG[0],
                         "the background window followed the resize on this tmux — the "
                         "stale-geometry premise this design rests on is gone and the "
                         "decision should be re-argued")

    def test_select_window_corrects_that_geometry_by_the_very_next_command(self):
        """And this is why the switch splits panels AFTER the select and not before.

        No sleep, no hook, no poll: the invocation immediately after `select-window`
        already reports the target window at the client's own size. Measured 200x50 →
        100x29 on tmux 3.7c and identically on tmux 3.2 — which matters because
        `set-hook -w window-resized` does not exist at all on 3.2 (`invalid option`,
        rc=1), so there is nothing there to repair a window charter did not correct
        itself.
        """
        one, two, fd = self._two_chats_and_a_client()
        self._resize(fd, *self.SMALL)
        self.assertTrue(
            self._wait_until(lambda: self._size_of(self._window_of(one))[0]
                             == self.SMALL[0]))
        self.assertEqual(self._srv("select-window", "-t", two).returncode, 0)
        self.assertEqual(self._size_of(self._window_of(two))[0], self.SMALL[0],
                         "the target window was still stale on the command after the "
                         "switch, so panels split here would be born at the wrong width")

    def test_a_pane_split_before_the_switch_is_born_at_the_wrong_width(self):
        """The defect the ordering exists to avoid, demonstrated rather than argued.

        Split into the background chat while it is still stale and the new pane is 200
        columns wide in a window the client will draw at 100. `panel._component_text`'s
        `width=slots._width()` guard is what that would have reached.
        """
        one, two, fd = self._two_chats_and_a_client()
        self._resize(fd, *self.SMALL)
        self.assertTrue(
            self._wait_until(lambda: self._size_of(self._window_of(one))[0]
                             == self.SMALL[0]))
        early = self._srv("split-window", "-d", "-t", two, "-l", "3",
                          "-P", "-F", "#{pane_id}", "sleep", "600")
        self.assertEqual(early.returncode, 0, early.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(early.stdout.strip()))
        stale_width = int(self._srv("display-message", "-p", "-t",
                                    early.stdout.strip(),
                                    "#{pane_width}").stdout.strip())

        self.assertEqual(self._srv("select-window", "-t", two).returncode, 0)
        late = self._srv("split-window", "-d", "-t", two, "-l", "3",
                         "-P", "-F", "#{pane_id}", "sleep", "600")
        self.assertEqual(late.returncode, 0, late.stderr)
        self.addCleanup(_kill_pid, self._pane_pid_on(late.stdout.strip()))
        fresh_width = int(self._srv("display-message", "-p", "-t",
                                    late.stdout.strip(),
                                    "#{pane_width}").stdout.strip())

        self.assertEqual(stale_width, self.BIG[0])
        self.assertEqual(fresh_width, self.SMALL[0])

    def test_select_pane_on_another_windows_pane_does_not_move_the_client(self):
        """**The measurement that makes the switch safe to run DETACHED.**

        `_close_palette` runs in the palette's own process, after `cmd_chat` has been
        started, and aims `select-pane` at the harness of the chat being LEFT. If that
        moved the client, the two processes would be racing for which window the operator
        ends on. It does not: current window `@0` before and `@0` after, on tmux 3.7c and
        on tmux 3.2. The pane it names becomes its own window's active pane and nothing
        else happens.
        """
        one, two, _fd = self._two_chats_and_a_client()
        before = self._current_window()
        self.assertEqual(self._srv("select-pane", "-t", two).returncode, 0)
        self.assertEqual(self._current_window(), before,
                         "`select-pane` dragged the client to another window on this "
                         "tmux — the palette's own close would then undo every switch")

    def test_kill_pane_works_on_the_window_the_client_has_just_left(self):
        """The teardown half. The chat being left is a background window by the time its
        panels are killed, and tmux is as happy to close a pane there as anywhere."""
        one, two, _fd = self._two_chats_and_a_client()
        extra = self._srv("split-window", "-d", "-t", one, "-l", "3",
                          "-P", "-F", "#{pane_id}", "sleep", "600")
        self.assertEqual(extra.returncode, 0, extra.stderr)
        panel = extra.stdout.strip()
        self.assertEqual(self._srv("select-window", "-t", two).returncode, 0)
        self.assertEqual(self._srv("kill-pane", "-t", panel).returncode, 0)
        self.assertNotIn(panel, self._srv("list-panes", "-a", "-F",
                                          "#{pane_id}").stdout.split())
        self.assertIn(one, self._srv("list-panes", "-a", "-F",
                                     "#{pane_id}").stdout.split(),
                      "killing the chat's panel took its harness with it")

    def test_a_placed_chat_bar_is_split_by_the_switch_and_paints_both_chats(self):
        """**The end-to-end nobody else in this branch makes**, and the one a unit test
        cannot: a plane that places `chats` with a `[[frame.component]]` table gets a real
        pane split for it by the switch, running a real `charter panel chats` process, and
        that process paints both chats with the active one marked.

        Every link in that chain was a separate gate before this branch —
        `instance.component_tables` refused the id for not being in `builtins.SLOT_OF`,
        `slots.drawable` refused the name for the same reason, and `panel._run` would then
        have held the pane open painting `unknown slot`. Asserting on what is ON THE PANE
        is what says all three opened; asserting that a pane exists would pass with
        `unknown slot` drawn in it.
        """
        # **The panel has to read the plane the frames are IN**, which is this case's own
        # throwaway root — not a second one. `child_plane_env` hands out a fresh empty
        # plane, and a `charter panel chats` pointed at that one scans an empty
        # `.charter/frame/`, finds no sibling, and draws a perfectly correct one-chat bar
        # for a workspace that has two. (It did exactly that, which is the second thing
        # this test caught.) `make_plane` puts a `charter.toml` at `PersonaIso`'s own
        # root, which is what makes a CHILD resolve the same plane this process has —
        # `PersonaIso` alone only redirects `config` in memory.
        plane = make_plane(self)
        one, two, fd = self._two_chats_and_a_client()
        frame = dict(config.FRAME)
        frame["components"] = instance.frame_components(
            {"frame": {"component": [{"use": "chats", "edge": "top", "size": 1},
                                     {"use": "identity"}]}})
        frame["slots"] = [p["use"] for p in frame["components"]]
        with mock.patch.object(commands_frame, "SOCKET", self.SOCKET_NAME), \
             mock.patch.object(config, "FRAME", frame), \
             mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": f"{self.WS}.1",
                              "CHARTER_WORKSPACE": self.WS,
                              "CHARTER_ROOT": str(plane)}, clear=False):
            for chat in (f"{self.WS}.1", f"{self.WS}.2"):
                state.record_workspace(chat, self.WS)
                state.record_identity(chat, {"CHARTER_SESSION_ID": chat,
                                             "CHARTER_ROOT": str(plane),
                                             "CHARTER_WORKSPACE": self.WS})
            self.assertEqual(
                commands_frame.cmd_chat(
                    SimpleNamespace(chat_id=f"{self.WS}.2", chat=f"{self.WS}.1")), 0)
            pane = state.panes(f"{self.WS}.2").get("chats")
            self.assertIsNotNone(
                pane, "the switch split no pane for a placed `chats` — it split "
                      f"{sorted(state.panes(f'{self.WS}.2'))}")
            drew = self._wait_for_text(pane, f"{self.WS}.1")
        self.assertIn(f"{self.WS}.1", drew, f"the chat bar painted {drew!r}")
        self.assertIn(f"*{self.WS}.2", drew,
                      f"the chat bar did not mark the chat switched to: {drew!r}")
        self.assertNotIn("unknown slot", drew)

    def _wait_for_text(self, pane: str, needle: str) -> str:
        """What *pane* is showing once *needle* is on it, or whatever it ends up showing.

        A panel is a real process that starts, imports `charter.frame` and paints —
        `--once` is not what production runs — so this polls rather than sleeps, and
        returns the last capture either way, so a failure says what WAS drawn rather than
        only that something was not.
        """
        seen = ""
        deadline = time.monotonic() + _DEADLINE
        while time.monotonic() < deadline:
            seen = self._srv("capture-pane", "-p", "-t", pane).stdout
            if needle in seen:
                return seen
            time.sleep(0.1)
        return seen

    def test_each_chat_keeps_its_own_escape_hatch_across_a_switch(self):
        """Stage 5b's exit criterion: `F12` returns to the harness from any chat.

        The hatch is a WINDOW option (`overlay.arm_hatch_argv`: `set-option -w -t <pane>`,
        which resolves to that pane's own window), so one chat per window is one hatch per
        chat for free — and the switch must not disturb either. The case that could have
        broken it is `_close_palette`, which re-arms the option on the chat being LEFT
        while the client is already on the chat being entered: window-scoped, that touches
        the old chat's window and nothing else. A global write would have handed one
        chat's hatch the other's harness pane, which is the "last launched wins" trap
        `conf_text` names for `mouse` and `history-limit`.
        """
        one, two, _fd = self._two_chats_and_a_client()
        for pane in (one, two):
            armed = overlay.arm_hatch_argv(self.SOCKET_NAME, harness=pane)
            self.assertIsNotNone(armed)
            self.assertEqual(_run(armed).returncode, 0)
        self.assertEqual(self._srv("select-window", "-t", two).returncode, 0)
        # And the palette's own close, aimed at the chat that was left.
        commands_frame._close_palette(self.SOCKET_NAME, harness=one,
                                      overlay_pane=self._spare_pane(one))
        for pane in (one, two):
            hatch = self._srv("display-message", "-p", "-t", pane,
                              f"#{{{overlay.HATCH_OPTION}}}").stdout.strip()
            self.assertIn(pane, hatch,
                          f"chat window for {pane} lost its own hatch: {hatch!r}")

    def _spare_pane(self, near: str) -> str:
        """A throwaway pane in *near*'s window, for `_close_palette` to kill.

        It needs a real overlay pane id or `overlay.close_argvs` refuses outright and the
        test would measure a command that was never sent.
        """
        made = self._srv("split-window", "-d", "-t", near, "-l", "3",
                         "-P", "-F", "#{pane_id}", "sleep", "600")
        self.assertEqual(made.returncode, 0, made.stderr)
        return made.stdout.strip()

    def test_the_whole_command_moves_the_client_and_re_lays_out_the_target(self):
        """`commands_frame.cmd_chat` end to end on a real server with a real client.

        The client starts on chat one with a panel pane; the switch is run; the client
        ends on chat two, chat one has lost its panel, and chat two has gained panes that
        are the CLIENT's width rather than the stale one. This is the exit criterion
        "switching between chats loses nothing" expressed as the smallest thing that can
        fail.
        """
        one, two, fd = self._two_chats_and_a_client()
        # A stand-in panel for chat one, recorded exactly as `_draw_panels` records one,
        # so the teardown path is charter's own rather than this test's.
        extra = self._srv("split-window", "-d", "-t", one, "-l", "3",
                          "-P", "-F", "#{pane_id}", "sleep", "600")
        self.assertEqual(extra.returncode, 0, extra.stderr)
        state.record_panes(f"{self.WS}.1", panels={"top": extra.stdout.strip()})
        # Resized while chat two is in the background, which is the case a bare
        # `select-window` would leave broken on tmux 3.2.
        self._resize(fd, *self.SMALL)
        self.assertTrue(
            self._wait_until(lambda: self._size_of(self._window_of(one))[0]
                             == self.SMALL[0]))

        # The panels this splits are REAL `charter panel` children, so they are pointed
        # at THIS case's own throwaway plane — `tests._planeguard` refuses the spawn
        # otherwise, and it is right to: a panel resolving the developer's own plane
        # would rewrite its caches. `make_plane` rather than `child_plane_env` for the
        # reason `test_a_placed_chat_bar_…` records: a panel must read the plane the
        # frames are IN, and a second empty plane is one it can read nothing out of.
        #
        # BOTH halves are needed and they are two different things. The `-e` is what the
        # PANE's process gets, and `_relayout_pane_env` builds it out of `state.identity`
        # — which is why the root is recorded there. `$CHARTER_ROOT` in this process is
        # what the tmux CLIENT inherits, because a live re-layout hands `tmuxctl.run` no
        # client environment at all (`_relayout`'s `env=None`), and that is what the
        # guard resolves a spawn's plane from.
        plane = make_plane(self)
        with mock.patch.object(commands_frame, "SOCKET", self.SOCKET_NAME), \
             mock.patch.dict(os.environ,
                             {"CHARTER_SESSION_ID": f"{self.WS}.1",
                              "CHARTER_WORKSPACE": self.WS,
                              "CHARTER_ROOT": str(plane)}, clear=False):
            for chat in (f"{self.WS}.1", f"{self.WS}.2"):
                state.record_workspace(chat, self.WS)
                state.record_identity(chat, {"CHARTER_SESSION_ID": chat,
                                             "CHARTER_ROOT": str(plane),
                                             "CHARTER_WORKSPACE": self.WS})
            rc = commands_frame.cmd_chat(
                SimpleNamespace(chat_id=f"{self.WS}.2", chat=f"{self.WS}.1"))
        self.assertEqual(rc, 0)

        self.assertEqual(self._current_window(), self._window_of(two),
                         "the client did not end on the chat that was switched to")
        self.assertEqual(state.panes(f"{self.WS}.1"), {},
                         "the chat that was left kept its panels, so they are now "
                         "rendering at a width that is not their window's")
        self.assertNotIn(extra.stdout.strip(),
                         self._srv("list-panes", "-a", "-F",
                                   "#{pane_id}").stdout.split())
        for pane in state.panes(f"{self.WS}.2").values():
            width = int(self._srv("display-message", "-p", "-t", pane,
                                  "#{pane_width}").stdout.strip())
            self.assertLessEqual(width, self.SMALL[0],
                                 "a panel was born at the background window's stale "
                                 "width")


class ASwitchAcrossSessionsIsRefusedRatherThanReported(_TmuxServerFixture, PersonaIso,
                                                       unittest.TestCase):
    """#684, against a real server: `select-window` at another session's pane returns 0.

    **A mock cannot make this claim.** Every other guard in `cmd_chat` is about a value
    charter read; this one is about what tmux DOES with a target it accepts, and the whole
    defect is that accepting it and acting on it look identical from charter's side.
    Measured here rather than quoted: the return code, both sessions' current windows, and
    whether the panels of the chat still on screen are alive afterwards.

    **Re-run against tmux 3.2 — `tmuxctl.FLOOR` — as well as 3.7c**, by putting a 3.2
    built from the release tarball first on `$PATH`. Identical on both, so nothing here
    carries a version gate.

    The plane is the one `charter workspace use beta` typed inside chat `api.1` produces:
    two chats whose `workspace` files both say `beta`, on one server, in two different
    tmux sessions. `chats.of_workspace` reads the file, so both land in one roster and
    `chats.check` says ok — which is what makes this reachable rather than hypothetical.
    """

    WS = "beta"

    def _chat_in_its_own_session(self, session: str, chat: str) -> str:
        """One chat as `cmd_launch` builds the FIRST chat of a workspace: its own tmux
        session, its window named for the chat, the production records beside it."""
        conf = os.path.join(self._gate_dir, f"{chat}.conf")
        Path(conf).write_text(commands_frame._PLACEHOLDER_CONF)
        gate = os.path.join(self._gate_dir, f"gate-{chat}")
        r = _run(layout.session_argv(session=session, conf=conf, chat=chat,
                                     socket=self.SOCKET_NAME, cols=80, rows=24,
                                     harness_argv=_gate_argv(gate, "exit 0"),
                                     env={"CHARTER_SESSION_ID": chat}))
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()
        self.addCleanup(_kill_pid, self._srv("display-message", "-p", "-t", pane,
                                             "#{pane_pid}").stdout.strip())
        state.record_server(chat, self.SOCKET_NAME)
        state.record_harness_pane(chat, pane)
        state.record_workspace(chat, self.WS)
        state.bump(chat)
        return pane

    def _current_window(self, session: str) -> str:
        return self._srv("display-message", "-p", "-t", f"{session}:",
                         "#{window_id}").stdout.strip()

    def setUp(self) -> None:
        super().setUp()
        self.one = self._chat_in_its_own_session("api", "api.1")
        self.two = self._chat_in_its_own_session("beta", "beta.1")
        # Two real panels on the chat the operator is looking at — what a wrongful
        # teardown kills, and the only way to tell "charter refused" from "charter had
        # nothing to tear down".
        panels = {}
        for slot in ("top", "bottom"):
            p = self._srv("split-window", "-d", "-t", self.one, "-P", "-F",
                          "#{pane_id}", "--", *_gate_argv(
                              os.path.join(self._gate_dir, f"gate-{slot}"), "exit 0"))
            self.assertEqual(p.returncode, 0, p.stderr)
            panels[slot] = p.stdout.strip()
        state.record_panes("api.1", panels=panels)
        self.panels = panels

    def _switch(self) -> list[str]:
        said: list[str] = []
        with mock.patch.object(commands_frame, "SOCKET", self.SOCKET_NAME), \
             mock.patch.object(commands_frame, "_say_on_screen",
                               lambda fid, msg, *a, **k: said.append(msg)), \
             mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "api.1",
                                          "CHARTER_WORKSPACE": self.WS}, clear=False):
            self.assertEqual(
                commands_frame.cmd_chat(SimpleNamespace(chat_id="beta.1",
                                                        chat="api.1")), 0)
        return said

    def test_tmux_really_does_accept_the_wrong_target(self):
        """The control this whole class rests on, measured on THIS tmux rather than
        quoted from the report. If `select-window` ever starts refusing a pane of another
        session, the guard above it is guarding nothing and the decision should be
        re-argued."""
        before = self._current_window("api")
        r = self._srv("select-window", "-t", self.two)
        self.assertEqual(r.returncode, 0,
                         "select-window refused another session's pane on this tmux")
        self.assertEqual(self._current_window("api"), before,
                         "the asking session moved after all, which would make that "
                         "return code an honest answer")

    def test_the_two_chats_share_a_roster_and_not_a_session(self):
        """The premise. Both files say `beta`, so both are in one roster; their windows
        are in two sessions, which is the fact no file holds."""
        self.assertEqual(chats.of_workspace(self.WS), ["api.1", "beta.1"])
        self.assertNotEqual(
            self._srv("display-message", "-p", "-t", self.one,
                      "#{session_id}").stdout.strip(),
            self._srv("display-message", "-p", "-t", self.two,
                      "#{session_id}").stdout.strip())

    def test_the_switch_is_refused_and_both_sessions_stay_where_they_were(self):
        api_before, beta_before = (self._current_window("api"),
                                   self._current_window("beta"))
        said = self._switch()
        self.assertEqual(len(said), 1, said)
        self.assertIn("another tmux session", said[0])
        self.assertEqual(self._current_window("api"), api_before)
        self.assertEqual(self._current_window("beta"), beta_before,
                         "charter moved a session it is not in")

    def test_the_panels_of_the_chat_on_screen_are_still_running(self):
        """The cost. Before this refusal the rc-0 reading was followed by
        `_apply_arrangement(fid, want=[])`, which killed these panes — the panels of the
        chat the operator was still looking at, torn down over a switch that never
        happened."""
        self._switch()
        alive = self._srv("list-panes", "-t", self.one, "-F",
                          "#{pane_id}").stdout.split()
        for slot, pane in self.panels.items():
            self.assertIn(pane, alive, f"the {slot} panel of the chat on screen was "
                                       f"killed by a switch that did not happen")
        self.assertEqual(state.panes("api.1"), self.panels,
                         "charter rewrote the pane map of a chat it did not leave")


if __name__ == "__main__":
    unittest.main()
