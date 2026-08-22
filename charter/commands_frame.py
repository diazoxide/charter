"""`charter <harness>` — run the harness inside charter's frame.

The launcher does NOT exec tmux, and that is measured rather than stylistic: an attached
`tmux new-session` returns 0 whatever its command exited with (tmux 3.7c). So the status
is carried out of band by a pane-scoped `pane-died` hook, and this process waits for tmux,
reads the recorded code, and exits with it. `exec` survives only on the bypass path, where
there is no frame in the way.

**Every frame shares one tmux server (`SOCKET`), told apart by session name (the frame
id), not one server per frame.** That single choice is what makes the rest of this module
non-obvious, and it is why `conf_text`'s output reaches tmux through `source-file` rather
than through the `-f` flag `layout.session_argv` also carries. Measured against tmux
3.7c: `-f` is read only at the moment a client's connection actually STARTS the server —
a later `new-session -f` against a server that is already running (the ordinary case,
once a first frame is up) is silently ignored. A frame launched second, third, or fifty-
first would then never get its OWN `pane-died` hook installed, because the config that
would have installed it was never read at all — and with no hook, `state.exit_code`
never gets written, so this launcher would report exit 0 for a harness that actually
crashed, which is the exact failure the module docstring above describes tmux itself
committing. `source-file` re-reads the same text against whatever server already
answers on the socket, so it is applied identically for the first frame and the
fifty-first (verified by hand against tmux 3.7c: a hook installed this way for a SECOND
session on an already-running server fires correctly, and its `kill-session` tears down
only that session, leaving a sibling frame's session untouched).
"""

from __future__ import annotations

import os
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


def bypass(argv: list[str]) -> int:
    """Run the harness with no frame at all — `exec`, so the exit code needs no help.

    Correct ONLY here. `cmd_launch`'s frame path never execs tmux, because an attached
    `tmux new-session` returns 0 regardless of what ran inside it (see the module
    docstring) — but there is no tmux in the way on this path, so the exit code an exec'd
    process carries out is already the real one.
    """
    os.execvp(argv[0], argv)
    return 127  # unreachable; execvp either replaces this process or raises


def conf_text(*, hotkey: str, mouse: bool, history_limit: int,
              status_path: str, harness_pane: str) -> str:
    """The private tmux config for one frame. Never the operator's ~/.tmux.conf.

    The `pane-died` hook is scoped to *harness_pane* — the id `cmd_launch` read off
    tmux's own stdout after creating the session, never the literal `"%0"`. Unscoped
    (or scoped to a guess) it either fires for any pane or never fires for the right one,
    and a crashed side panel — or a frame whose harness landed on some other pane id —
    would be reported to the operator as the agent's own exit code.

    `hotkey` is accepted and never bound to anything here on purpose: binding the key AND
    populating the menu it opens both belong to the task that makes the menu reachable,
    and half of that wiring (a key bound to nothing) would be worse than none of it.
    """
    return "\n".join([
        "set -g status off",
        f"set -g mouse {'on' if mouse else 'off'}",
        f"set -g history-limit {int(history_limit)}",
        "set -g escape-time 0",
        # Without this, the pane (and often the whole session) is torn down the instant
        # the harness exits — before the hook below has a pane left to read
        # `#{pane_dead_status}` from, and the exit code is lost with it.
        "set -g remain-on-exit on",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        f"set-hook -p -t {harness_pane} pane-died "
        f"'run-shell \"echo #{{pane_dead_status}} > {status_path}\" ; kill-session'",
        "",
    ])


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
    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        # `frame_dir` refuses rather than raises (see charter/frame/state.py) — an id
        # `contain.child` cannot shape into a directory (or a name so long `mkdir` hits
        # ENAMETOOLONG) must not be treated as a Path here just because it usually is one.
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1

    state.reap(_live_sessions(SOCKET))
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
    # A placeholder. `session_argv`'s `-f` reads it ONLY if this call is what starts a
    # brand-new tmux server on SOCKET — content here is never load-bearing (tmux
    # tolerates the path not existing at all), because the config that actually matters
    # is `source-file`d in below, once the harness pane id is known. See the module
    # docstring for the measured reason a *second* frame cannot rely on `-f` at all.
    conf_path.write_text("")

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
                                   history_limit=frame["history_limit"],
                                   status_path=str(status_path), harness_pane=harness_pane))
    src_cmd = ["tmux", "-L", SOCKET, "source-file", str(conf_path)]
    src = subprocess.run(src_cmd, env=env, capture_output=True, text=True, timeout=15)
    if src.returncode != 0:
        _report_tmux_failure("loading the frame's config", src_cmd, src)
        util.warn("charter frame: continuing without it — mouse/history-limit/hotkey "
                  "settings and the exit-code hook may not be in effect for this frame")

    for cmd in layout.panel_argvs(slots=slots, session=fid, socket=SOCKET,
                                  charter_argv=[sys.executable, "-m", "charter"],
                                  harness_pane=harness_pane):
        p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=15)
        if p.returncode != 0:
            # Reported, not fatal: one decorative panel failing to draw must not take
            # down a harness pane that is already up and running (correction 2 asks for
            # every return code to be CHECKED, not for every failure to be a refusal).
            _report_tmux_failure("drawing a panel", cmd, p)

    # No capture_output: this is the operator's own terminal for as long as the harness
    # runs, not an admin command whose output charter should own.
    attach_cmd = ["tmux", "-L", SOCKET, "attach", "-t", fid]
    attach = subprocess.run(attach_cmd, env=env)

    code = state.exit_code(fid)
    state.reap(_live_sessions(SOCKET))
    if code is not None:
        return code
    if attach.returncode != 0:
        # The hook never recorded anything AND tmux itself reported trouble attaching —
        # surfaced rather than folded into a bare 0, which is precisely the failure this
        # whole module exists to stop happening silently.
        _report_tmux_failure("attaching to the frame", attach_cmd, attach)
        return attach.returncode
    return 0
