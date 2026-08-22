"""`charter <harness>` — run the harness inside charter's frame.

The launcher does NOT exec tmux, and that is measured rather than stylistic: an attached
`tmux new-session` returns 0 whatever its command exited with (tmux 3.7c). So the status
is carried out of band — normally by a pane-scoped `pane-died` hook, with a direct query
as a fallback for the race described below — and this process waits for tmux, reads the
recorded (or queried) code, and exits with it. `exec` survives only on the bypass path,
where there is no frame in the way.

**Every frame shares one tmux server (`SOCKET`), told apart by session name (the frame
id), not one server per frame.** That single choice is what makes the rest of this module
non-obvious, and it is why the session-scoped/hook-installing config reaches tmux through
`source-file`/a direct `set-hook` command rather than through the `-f` flag
`layout.session_argv` also carries. Measured against tmux 3.7c: `-f` is read only at the
moment a client's connection actually STARTS the server — a later `new-session -f`
against a server that is already running (the ordinary case, once a first frame is up) is
silently ignored. A frame launched second, third, or fifty-first would then never get its
own config applied at all if it relied on `-f` alone. `source-file` and a direct
`set-hook` invocation both re-apply against whatever server already answers on the
socket, so they work identically for the first frame and the fifty-first (verified by
hand against tmux 3.7c: a hook installed this way for a SECOND session on an
already-running server fires correctly, and its `kill-session` tears down only that
session, leaving a sibling frame's session untouched).

**A second, narrower race survives even that fix.** `new-session` starts the harness
running immediately; the hook that would record its exit code is not installed until a
separate `set-hook` call some milliseconds later (measured 8.2-10.5ms). A harness that
dies inside that window is never caught by the hook — hooks do not fire retroactively for
an event that already happened, so it never existed yet to catch anything — and this is
worse than `state.exit_code` merely reading back `None`: with nothing left to run the
hook's own `kill-session`, an `attach` against that session BLOCKS FOREVER (verified by
hand against a real tmux 3.7c — `remain-on-exit`, armed for exactly this reason, is
legitimately keeping the dead pane's session alive; the module never called anything to
end it). `remain-on-exit on` in the placeholder `-f` config is still necessary — it is
what keeps the pane around long enough to be askable at all — but is not sufficient on
its own. What actually closes the race is asking tmux directly, `display-message -p -t
<harness_pane> '#{pane_dead}:#{pane_dead_status}'`, IMMEDIATELY after the `set-hook` call
returns and BEFORE ever calling `attach`: if the pane is already dead at that point, this
launcher finishes the hook's job itself (records the code, runs `kill-session`) and skips
`attach` entirely, rather than block on a session that will never end on its own. The
same query runs again as a fallback after `attach` DOES return, for whatever gap the
eager check could not have seen yet.
"""

from __future__ import annotations

import os
import shlex
import subprocess
import sys

from . import config, harness, util, workspace
from .frame import layout, state, tmuxctl

#: One shared tmux server for every frame this machine runs, sessions (not servers) told
#: apart by name — the frame id. Never the operator's own default socket: a frame's
#: config (`conf_text`) is charter's, not something to lay over `~/.tmux.conf`.
SOCKET = "charter"

#: Terminal size assumed when `os.get_terminal_size()` cannot report one even though
#: `sys.stdout.isatty()` said yes (seen from a harness invoked under a test/CI wrapper
#: that fakes a tty for capture but backs it with nothing `TIOCGWINSZ` can answer). The
#: same floor `shutil.get_terminal_size()` itself falls back to — small enough that
#: `layout.visible_slots` degrades the frame to a bare harness instead of a guess that
#: might not fit the terminal that is actually there.
_FALLBACK_SIZE = (80, 24)

#: The placeholder loaded via `session_argv`'s `-f`, in effect ONLY if this call happens
#: to start a brand-new tmux server on `SOCKET` (see the module docstring: `-f` is
#: ignored otherwise). `remain-on-exit` alone, deliberately global (`-g`, not scoped to
#: this one session): every frame wants the same value, so there is nothing to leak by
#: sharing it, and this is what has to be in effect from the instant the harness's
#: process starts — before this launcher has even read the pane id back off tmux's
#: stdout, let alone had a chance to `source-file` the rest of the config — or a harness
#: that dies in that opening window tears its own pane down before anything downstream
#: (the hook, this launcher's fallback query) has a pane left to learn anything from.
_PLACEHOLDER_CONF = "set -g remain-on-exit on\n"


def bypass(argv: list[str]) -> int:
    """Run the harness with no frame at all — `exec`, so the exit code needs no help.

    Correct ONLY here. `cmd_launch`'s frame path never execs tmux, because an attached
    `tmux new-session` returns 0 regardless of what ran inside it (see the module
    docstring) — but there is no tmux in the way on this path, so the exit code an exec'd
    process carries out is already the real one.
    """
    os.execvp(argv[0], argv)
    return 127  # unreachable; execvp either replaces this process or raises


def conf_text(*, hotkey: str, mouse: bool, history_limit: int, session: str) -> str:
    """The private tmux config for one frame's own settings. Never `~/.tmux.conf`.

    `status`, `mouse` and `history-limit` are SESSION-scoped (`set -t <session>`, no
    `-g`) — not global. `source-file` applies whatever this returns to the one shared
    server every frame runs on, so a `-g` write here would rewrite every OTHER frame's
    settings too: the second frame launched turns the first frame's mouse/scrollback
    into its own, "last launched wins" for both, silently, even across planes with
    different `[frame]` config (verified by hand against tmux 3.7c — a session-scoped
    `set -t` sibling session does NOT pick up a value set this way for another session).
    `escape-time` stays `-g`: verified against tmux 3.7c that it is a genuine SERVER
    option (`show-options -s`, not `-g`), so there is no per-session form to move it to.
    The `WheelUpPane` bind is likewise left global — key tables are server-wide in tmux,
    there being no such thing as a per-session keymap, and every frame wants the
    identical binding anyway, so nothing is lost by sharing it the same way
    `remain-on-exit` is (see `_PLACEHOLDER_CONF`).

    The `pane-died` hook does NOT live here — see `_pane_died_hook_argv` for why it is
    issued as its own tmux command instead of text baked into this string.

    `hotkey` is accepted and never bound to anything here on purpose: binding the key AND
    populating the menu it opens both belong to the task that makes the menu reachable,
    and half of that wiring (a key bound to nothing) would be worse than none of it.
    """
    return "\n".join([
        f"set -t {session} status off",
        f"set -t {session} mouse {'on' if mouse else 'off'}",
        f"set -t {session} history-limit {int(history_limit)}",
        "set -g escape-time 0",
        "set -g remain-on-exit on",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        "",
    ])


def _pane_died_hook_argv(*, socket: str, harness_pane: str, status_path: str) -> list[str]:
    """The `set-hook` command that records the harness's real exit code.

    Scoped to *harness_pane* — the id `cmd_launch` read off tmux's own stdout after
    creating the session, never a guess. Unscoped, `pane-died` fires for ANY pane, and a
    crashed side panel would be reported to the operator as the agent's own exit code.

    Issued as its OWN tmux command — never embedded as text inside `conf_text`'s output
    the way an earlier version of this module did — because the hook's action is itself
    a shell command string that needs its own quoting, and nesting THAT inside a second,
    text-file level of tmux quoting (`source-file`'s config syntax) breaks even
    `shlex.quote`'s own escaping for a path containing a literal single quote: verified
    against tmux 3.7c that the two nested quote layers corrupt each other's boundaries
    (the outer layer's quote character terminates early on a quote character that only
    means something to the inner one). Passed as one clean argv element instead —
    matching every other command `frame/layout.py` builds, and the reason its own module
    docstring gives for never joining argv (`gh -F`, #328) — `shlex.quote` then needs no
    help and was verified, by hand against the same tmux 3.7c, to round-trip a path
    containing a space, a literal single quote, and a `$(...)` injection attempt
    correctly: the exact byte string reaches the file, and nothing embedded in it runs.

    *status_path* reaches here from `STATE_DIR` (the plane root, or `$CHARTER_HOME`) —
    never operator argv — but it is still handed to `/bin/sh -c` inside `run-shell`, the
    one place this module hands tmux a STRING rather than an argv list, so it is quoted
    like any other value that reaches a shell: a plane root with a space in it must not
    silently write to the wrong file, and one shaped like `$(...)` must not run.
    """
    action = (f'run-shell "echo #{{pane_dead_status}} > {shlex.quote(status_path)}" '
             f'; kill-session')
    return ["tmux", "-L", socket, "set-hook", "-p", "-t", harness_pane, "pane-died", action]


def _live_sessions(socket: str) -> set[str]:
    """Every session name `tmux -L socket list-sessions` currently reports.

    Empty rather than raised when nothing has ever run on *socket* — `list-sessions`
    exits non-zero with nothing on stdout in that case, which the plain `splitlines()`
    below already turns into an empty set with no special-casing needed. The `except`
    guards the rarer case: tmux vanishing between `tmuxctl.available()`'s own check and
    this call, not the ordinary "no server yet" one.
    """
    try:
        out = subprocess.run(["tmux", "-L", socket, "list-sessions", "-F", "#{session_name}"],
                             capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return set()
    return {line.strip() for line in out.stdout.splitlines() if line.strip()}


def _report_tmux_failure(action: str, cmd: list[str], proc: subprocess.CompletedProcess) -> None:
    """Name the command that failed and tmux's own stderr. Never silent.

    This is what correction 2 exists to force: `subprocess.run(cmd, env=env)` with the
    result thrown away is exactly how the pane-index bug `frame/layout.py`'s own module
    docstring describes would have shipped — a frame missing a panel, and nothing
    anywhere saying why.
    """
    stderr = (proc.stderr or "").strip() or "(tmux printed nothing to stderr)"
    util.err(f"charter frame: {action} failed — `{' '.join(cmd)}`: {stderr}")


def _query_pane_dead_status(socket: str, harness_pane: str) -> int | None:
    """Ask tmux directly whether *harness_pane* died, and with what status.

    Called twice by `cmd_launch`, for two different gaps a hook alone cannot cover: once
    EAGERLY, right after `set-hook` returns, to close the install race the module
    docstring measures (a harness that died before the hook existed is never caught by
    it, because a hook only fires for a death AFTER it exists); and again as a fallback
    after `attach` returns, for whatever the eager check could not have seen yet.
    `remain-on-exit` (armed from the very first moment via `_PLACEHOLDER_CONF`) is what
    keeps the pane around long enough to still be askable either time — without it, this
    query would simply find no such pane, exactly like the hook found no such event.

    ``None`` for "cannot tell" (the pane is alive, or the query itself failed) — never a
    fabricated 0, which is the exact silent-success failure this whole module exists to
    rule out.
    """
    dm = subprocess.run(["tmux", "-L", socket, "display-message", "-p", "-t", harness_pane,
                         "#{pane_dead}:#{pane_dead_status}"],
                        capture_output=True, text=True, timeout=5)
    if dm.returncode != 0:
        return None
    dead, _, status = dm.stdout.strip().partition(":")
    if dead != "1":
        return None
    status = status.strip()
    if status.lstrip("-").isdigit():
        return int(status)
    return None


def cmd_launch(args) -> int:
    """One launcher, shared by every registered harness and by `charter frame --`."""
    h = next((x for x in harness.all() if x.cli_name == args.harness), None)
    rest = list(args.rest or [])
    # `nargs=argparse.REMAINDER` keeps a literal leading `--` when the operator typed
    # one (`charter frame -- claude -p hi` → `rest == ["--", "claude", "-p", "hi"]`); it
    # is the separator that told argparse to stop parsing, not part of the command.
    if rest and rest[0] == "--":
        rest = rest[1:]
    argv = h.launch_argv(rest) if h else rest
    if not argv:
        util.err("charter frame: nothing to run — `charter frame -- <command>`")
        return 2

    if args.no_frame or not sys.stdout.isatty():
        return bypass(argv)

    # ONE call (correction 5): `available()`, `version()` and `meets_floor()` each shell
    # out to `tmux -V` independently, so calling all three in sequence would spawn three
    # subprocesses to ask the same unchanging question on every single launch.
    v = tmuxctl.version()
    if v is None:
        util.err(tmuxctl.absent_message())
        return 1
    if v < tmuxctl.FLOOR:
        # Below the floor: degrade (the hotkey menu, not yet wired to anything by this
        # task, would be unusable anyway), never refuse — the frame itself still works.
        util.warn(tmuxctl.below_floor_message(v))

    ws = workspace.resolve()
    fid = state.frame_id(ws, os.getpid())

    # Reap BEFORE this frame's own directory exists, not after: `frame_dir(create=True)`
    # below makes that directory, and a `reap()` run afterward — but still before
    # `session_argv` starts this frame's OWN tmux session — would see a directory with
    # no live session yet and delete it out from under this very launch. Reaping first
    # also narrows (though does not close) the same race for a sibling frame's `exit`
    # file: less time between "session gone" and "directory removed" for a sibling's own
    # launcher to lose the read.
    state.reap(_live_sessions(SOCKET))

    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        # `frame_dir` refuses rather than raises (see charter/frame/state.py) — an id
        # `contain.child` cannot shape into a directory (or a name so long `mkdir` hits
        # ENAMETOOLONG) must not be treated as a Path here just because it usually is one.
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1
    state.bump(fid)

    try:
        cols, rows = os.get_terminal_size()
    except OSError:
        # `os.get_terminal_size()` raises even when `isatty()` said yes — a tty with
        # nothing behind it to answer `TIOCGWINSZ`. Falling back rather than propagating
        # is the deliberate choice; see `_FALLBACK_SIZE`'s own docstring for why 80x24.
        cols, rows = _FALLBACK_SIZE

    frame = config.FRAME
    slots = layout.visible_slots(frame["slots"], cols, rows, frame["min_cols"], frame["min_rows"])

    env = dict(os.environ, CHARTER_SESSION_ID=fid)
    if h:
        env["CHARTER_HARNESS"] = h.name

    conf_path = fdir / "tmux.conf"
    status_path = fdir / "exit"
    conf_path.write_text(_PLACEHOLDER_CONF)

    session_cmd = layout.session_argv(session=fid, conf=str(conf_path), socket=SOCKET,
                                      cols=cols, rows=rows, harness_argv=argv)
    proc = subprocess.run(session_cmd, env=env, capture_output=True, text=True, timeout=15)
    if proc.returncode != 0:
        _report_tmux_failure("starting the frame", session_cmd, proc)
        return 1
    harness_pane = proc.stdout.strip()
    if not harness_pane:
        util.err("charter frame: tmux started the session but did not report a pane id "
                 "— cannot scope the exit-code hook to it")
        return 1

    conf_path.write_text(conf_text(hotkey=frame["hotkey"], mouse=frame["mouse"],
                                   history_limit=frame["history_limit"], session=fid))
    src_cmd = ["tmux", "-L", SOCKET, "source-file", str(conf_path)]
    src = subprocess.run(src_cmd, env=env, capture_output=True, text=True, timeout=15)
    if src.returncode != 0:
        _report_tmux_failure("loading the frame's config", src_cmd, src)
        util.warn("charter frame: continuing without it — mouse/history-limit/hotkey "
                  "settings may not be in effect for this frame")

    hook_cmd = _pane_died_hook_argv(socket=SOCKET, harness_pane=harness_pane,
                                    status_path=str(status_path))
    hook = subprocess.run(hook_cmd, env=env, capture_output=True, text=True, timeout=15)
    if hook.returncode != 0:
        _report_tmux_failure("installing the exit-code hook", hook_cmd, hook)
        util.warn("charter frame: continuing without it — the exit code may not be "
                  "recorded for this frame")

    # Closes the install race, rather than merely working around its symptom. A harness
    # that died in the window between `new-session` starting it and the `set-hook` call
    # above actually registering leaves the hook registered for an event that ALREADY
    # happened — hooks do not fire retroactively, so it never fires at all, and nothing
    # is left to run the hook's own `kill-session`. Verified by hand against a real tmux
    # 3.7c: `attach` below then blocks FOREVER on a session `remain-on-exit` is legitimately
    # keeping alive, not merely returns 0 early — a worse failure than the one the module
    # docstring describes, because there is no return to read a code from at all. Asking
    # directly, immediately, closes it: if the pane is already dead, the job the hook
    # would have done (record the code, end the session) is finished right here, and
    # `attach` is never even called against a session already known to be over.
    code = _query_pane_dead_status(SOCKET, harness_pane)
    if code is not None:
        state.record_exit(fid, code)
        subprocess.run(["tmux", "-L", SOCKET, "kill-session", "-t", fid], env=env,
                       capture_output=True, text=True, timeout=15)

    attach = None
    attach_cmd = None
    if code is None:
        for cmd in layout.panel_argvs(slots=slots, session=fid, socket=SOCKET,
                                      charter_argv=[sys.executable, "-m", "charter"],
                                      harness_pane=harness_pane):
            p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=15)
            if p.returncode != 0:
                # Reported, not fatal: one decorative panel failing to draw must not
                # take down a harness pane that is already up and running (correction 2
                # asks for every return code to be CHECKED, not every failure refused).
                _report_tmux_failure("drawing a panel", cmd, p)

        # No capture_output: this is the operator's own terminal for as long as the
        # harness runs, not an admin command whose output charter should own.
        attach_cmd = ["tmux", "-L", SOCKET, "attach", "-t", fid]
        attach = subprocess.run(attach_cmd, env=env)

        code = state.exit_code(fid)
        if code is None:
            # A second ask, for whatever gap the eager check above could not have seen
            # yet (the harness was still alive at that point and died later — the
            # ordinary case, caught here by the hook, or a rarer one where the hook
            # itself never actually reached the server despite reporting success).
            code = _query_pane_dead_status(SOCKET, harness_pane)

    live_after = _live_sessions(SOCKET)
    state.reap(live_after)
    if code is not None:
        return code
    if fid in live_after:
        # Detach, not completion — the session tmux still lists is this frame's own.
        # Silence here is exactly what the spec calls out: "an agent surviving a closed
        # lid is a feature, and returning silently to a shell with it still running is
        # not."
        util.info(f"charter frame: detached — the harness is still running.\n"
                 f"  reattach with: tmux -L {SOCKET} attach -t {fid}")
        return 0
    if attach is not None and attach.returncode != 0:
        # Nothing was recorded, tmux is not still tracking this session, AND `attach`
        # itself reported trouble — surfaced rather than folded into a bare 0, which is
        # precisely the failure this whole module exists to stop happening silently.
        _report_tmux_failure("attaching to the frame", attach_cmd, attach)
        return attach.returncode
    return 0
