"""`charter <harness>` — run the harness inside charter's frame.

The launcher does NOT exec tmux, and that is measured rather than stylistic: an attached
`tmux new-session` returns 0 whatever its command exited with (tmux 3.7c). So the status
is carried out of band — normally by a pair of `pane-died` hooks, with a direct query as
a fallback for the race described below — and this process waits for tmux, reads the
recorded (or queried) code, and exits with it. `exec` survives only on the bypass path,
where there is no frame in the way.

**Every frame shares one tmux server (`SOCKET`), told apart by session name (the frame
id), not one server per frame.** That single choice is what makes the rest of this module
non-obvious, and it is why the session-scoped/hook-installing config reaches tmux through
`source-file`/direct `set-hook`/`set-environment` commands rather than through the `-f`
flag `layout.session_argv` also carries. Measured against tmux 3.7c: `-f` is read only at
the moment a client's connection actually STARTS the server — a later `new-session -f`
against a server that is already running (the ordinary case, once a first frame is up) is
silently ignored. A frame launched second, third, or fifty-first would then never get its
own config applied at all if it relied on `-f` alone. `source-file` and direct commands
both re-apply against whatever server already answers on the socket, so they work
identically for the first frame and the fifty-first (verified by hand against tmux 3.7c: a
hook installed this way for a SECOND session on an already-running server fires correctly,
and its `kill-session` tears down only that session, leaving a sibling frame untouched).

**A second, narrower race survives even that fix.** `new-session` starts the harness
running immediately; the hooks that would record its exit code and end the session are
not installed until separate `set-hook` calls a few milliseconds later (measured
8.2-10.5ms). A harness that dies inside that window is never caught by them — hooks do
not fire retroactively for an event that already happened — and this is worse than
`state.exit_code` merely reading back `None`: with nothing left to run `kill-session`, an
`attach` against that session BLOCKS FOREVER (verified by hand against a real tmux 3.7c,
via a Python `pty` driving the real launcher end to end — `remain-on-exit`, armed for
exactly this reason, is legitimately keeping the dead pane's session alive; nothing else
was ever going to end it). `remain-on-exit on` in the placeholder `-f` config is still
necessary — it is what keeps the pane around long enough to be askable at all — but is
not sufficient on its own. What actually closes the race is asking tmux directly,
`display-message -p -t <harness_pane> '#{pane_dead}:#{pane_dead_status}'`, IMMEDIATELY
after the hooks are installed and BEFORE ever calling `attach`: if the pane is already
dead at that point, this launcher finishes the hooks' own job itself (records the code,
runs `kill-session`) and skips `attach` entirely, rather than block on a session that
will never end on its own. The same query runs again as a fallback after `attach` DOES
return, for whatever gap the eager check could not have seen yet.

**The exit-code hook's action text is a CONSTANT string, on purpose — status_path is
never embedded in it.** An earlier version of this module interpolated the path directly
into the hook's `run-shell "echo ... > <path>"` action. That action is TEXT tmux
re-parses as a fresh command line when the hook fires — one tmux-quote layer nested
inside the `source-file`/`set-hook` install call that wrote it — and `shlex.quote()`
cannot escape a path containing a literal `'` correctly across that nesting: verified by
hand against tmux 3.7c that the install call still reported success (`set-hook` returned
0) while the stored action was silently corrupted (`; kill-session` disappeared into a
mangled argument), so a plane root with an apostrophe in it hung the exact same way the
install race does, with SUCCESS printed on the way in. Fixed by delivering the path out
of band instead — `set-environment -t <session> CHARTER_FRAME_EXIT <path>`, a single
argv value, no shell involved — and writing a hook action that never varies by frame at
all (`_pane_died_write_hook_argv`): nothing in it depends on plane state, so there is
nothing left for any path, however hostile, to corrupt (verified against tmux 3.7c with
a path containing a space, a literal `'`, and a `$(...)` injection attempt: the exact
byte string reaches the file and nothing embedded in it runs).

**Teardown is its own hook, `pane-died[1]`, entirely separate from the write hook
(`pane-died[0]`).** `kill-session` alone, a constant string with no interpolated data at
all. Verified by hand: even with the write hook's own action deliberately mangled, the
teardown hook — sharing no text with it — still fired and ended the session correctly.
Belt and braces over the constant-action fix above: it means a *future* bug in the write
hook's own construction can degrade to "the wrong exit code was recorded" and never
regress all the way back to "the session never ends."

**The two hooks must be installed in this order — write, then teardown — and that is
load-bearing, not incidental.** Verified against tmux 3.7c: an UNINDEXED `set-hook -p -t
<pane> pane-died '<action>'` call does not overwrite index 0 of an existing hook array;
it REPLACES THE WHOLE ARRAY, silently deleting `[1]` if it was already there. Installing
the write hook second would wipe out the teardown hook the moment it lands — reproduced
end to end by swapping the two `cmd_launch` calls: the session hung exactly the way it
did before either hook existed. See `_pane_died_teardown_hook_argv`'s own docstring.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys

from . import config, harness, util, workspace
from .frame import layout, menu, state, tmuxctl
# Aliased: `cmd_launch` already has a local variable named `slots` (the VISIBLE slot
# list `layout.visible_slots` returns) — importing the renderer registry under its own
# name would be shadowed by that local the moment it's assigned, and a `slots.SLOTS`
# lookup after that point would silently resolve to the wrong thing (an `AttributeError`
# on a `list`, not a helpful one).
from .frame import slots as frame_slots

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
#: ignored otherwise — which is also why `cmd_launch` arms this same setting a second,
#: direct way when a server is already running; see the "by construction" comment
#: there). `remain-on-exit` alone, deliberately global (`-g`, not scoped to this one
#: session): every frame wants the same value, so there is nothing to leak by sharing
#: it, and this is what has to be in effect from the instant the harness's process
#: starts — before this launcher has even read the pane id back off tmux's stdout, let
#: alone had a chance to install anything else — or a harness that dies in that opening
#: window tears its own pane down before anything downstream (the hooks, this
#: launcher's fallback query) has a pane left to learn anything from.
_PLACEHOLDER_CONF = "set -g remain-on-exit on\n"

#: The session-scoped environment variable the write hook's shell reads the exit-status
#: path back from — see the module docstring's "constant string" section for why the
#: path is delivered this way instead of being embedded in the hook's own action text.
_EXIT_PATH_ENV = "CHARTER_FRAME_EXIT"

#: What `_query_pane_dead_status` returns for a pane confirmed dead (`#{pane_dead}` is
#: `1`) whose `#{pane_dead_status}` tmux itself could not report — measured against tmux
#: 3.7c: EMPTY, not negative, for a harness killed by SIGKILL/SIGTERM/SIGSEGV. `None`
#: means "cannot tell" everywhere in this module (see `_query_pane_dead_status`'s own
#: docstring) and must never be confused with "dead, but the real number isn't known" —
#: the pane genuinely IS gone either way, so returning `None` here would send
#: `cmd_launch` on to `attach` a session with nothing left to end it, recreating the
#: exact hang the eager check exists to close, just from a signal instead of a race.
_UNKNOWN_DEATH_CODE = 1


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

    Neither `pane-died` hook lives here — see `_pane_died_write_hook_argv` and
    `_pane_died_teardown_hook_argv` for why they are issued as their own tmux commands
    instead of text baked into this string.

    *session* is the frame id, which `state.frame_id` already sanitises (see
    `charter/frame/state.py`) before this function ever sees it — interpolated into
    plain `set -t <session> ...` config text, never into a shell command string, so it
    carries none of the risk `_EXIT_PATH_ENV`'s docstring describes for `status_path`.

    `hotkey` opens this frame's own menu: `bind -n {hotkey} run-shell 'charter
    frame-menu'`. A key BINDING has no per-session form the way `status`/`mouse`/
    `history-limit` above do — key tables are server-wide in tmux, so every frame on
    `SOCKET` ends up sharing this exact bind text, "last launched wins" exactly like
    `escape-time`/`remain-on-exit`/the `WheelUpPane` bind two lines down already do. That
    is only safe here because the action itself carries no frame identity: `charter
    frame-menu` (`cmd_menu`) resolves the CURRENT session from `$CHARTER_SESSION_ID` —
    carried out of band via `set-environment`, see `_session_id_env_argv` — at the moment
    the key actually fires, never from anything baked into this text. A bind that
    embedded one frame's own id here would start opening the WRONG frame's menu the
    instant a second frame launched, the same trap this function's own docstring already
    names for `mouse`/`history-limit`, just reached through a binding instead of a
    session-scoped `set`.

    This also satisfies correction 2's "only in charter's own server" rule by
    construction rather than by discipline at each call site: `conf_text`'s only caller
    (`cmd_launch`) sources this text against `SOCKET`, charter's own private server —
    never the operator's own default socket (see this module's own docstring, "Never the
    operator's own default socket").

    No `-t` on the `run-shell` itself, unlike `_exit_path_env_argv`'s callers (which
    target a session by NAME): verified by hand against tmux 3.7c that `run-shell -t =`
    — the idiom the `WheelUpPane` bind below already uses for `if-shell -F` — does NOT
    carry a session's own `set-environment` values into the shell it spawns, while
    OMITTING `-t` entirely does. `-t =` resolves for FORMAT evaluation (`if-shell -F`,
    which never spawns a process); it is not the same mechanism a spawned shell's
    environment goes through, and the two do not behave alike.
    """
    return "\n".join([
        f"set -t {session} status off",
        f"set -t {session} mouse {'on' if mouse else 'off'}",
        f"set -t {session} history-limit {int(history_limit)}",
        "set -g escape-time 0",
        "set -g remain-on-exit on",
        f"bind -n {hotkey} run-shell 'charter frame-menu'",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        "",
    ])


def _exit_path_env_argv(*, socket: str, session: str, status_path: str) -> list[str]:
    """`set-environment`: carries *status_path* to the write hook's shell out of band.

    One argv value, no shell parsing at all on this side — the whole point (see the
    module docstring). `run-shell`'s own spawned shell later reads it back from its
    inherited environment via `$CHARTER_FRAME_EXIT`, verified by hand to work for a
    SESSION-scoped `set-environment` (no `-g`) reaching a hook fired for a pane in that
    session.
    """
    return ["tmux", "-L", socket, "set-environment", "-t", session, _EXIT_PATH_ENV,
           status_path]


def _session_id_env_argv(*, socket: str, session: str) -> list[str]:
    """`set-environment`: makes *session* resolvable from its own `run-shell` calls.

    `cmd_menu` and `cmd_action` (the hotkey bind's own action, and every menu item's own
    action) both read `$CHARTER_SESSION_ID` back out of a `run-shell`-spawned process's
    environment — the same out-of-band shape `_exit_path_env_argv` already uses for
    `status_path`, and for the identical reason: this is what lets `conf_text`'s bind
    stay a single, frame-agnostic line shared by every session on `SOCKET` (see its own
    docstring) while still resolving the RIGHT frame at the moment the key fires.

    Without this call, `run-shell` fired from a LATER frame sharing `SOCKET` does not
    fall back to "no id" — it falls back to the FIRST frame's own id, silently. Verified
    by hand against tmux 3.7c: a second session started on an already-running server,
    `run-shell`'d with no override of its own, reported the FIRST frame's
    `$CHARTER_SESSION_ID` (present in the SERVER's own starting process environment,
    inherited from whichever `new-session` call happened to start it) rather than an
    empty string — `show-environment -t <session>` confirmed the value was never tracked
    per-session at all until a call exactly like this one ties it there explicitly. Left
    unfixed, every frame after the first would open the FIRST frame's own menu and run
    its actions against the wrong session's state.

    The value IS the session's own name (`state.frame_id`'s own restricted alphabet —
    see its docstring — so there is nothing here for this call's own text to sanitise),
    which is why this hands the session right back to itself rather than taking a
    separate value the way `_exit_path_env_argv` takes `status_path`.
    """
    return ["tmux", "-L", socket, "set-environment", "-t", session, "CHARTER_SESSION_ID",
           session]


def _pane_died_write_hook_argv(*, socket: str, harness_pane: str) -> list[str]:
    """`pane-died[0]`: writes the harness's real exit status, out of band.

    A CONSTANT action — no operator- or plane-derived string is ever embedded in this
    text (see the module docstring's "constant string" section for the bug that fixes).
    `\\"` before a literal `"` or `$` is load-bearing, not decorative: verified by hand
    that an UNESCAPED `$` inside this tmux double-quoted argument is consumed by tmux's
    OWN parsing before the shell ever sees it (a first draft came out with the variable
    reference silently missing, and a SECOND unescaped `$` — inside `${v:-1}` — made
    `set-hook` itself fail outright with "invalid environment variable"); the escaped
    form is what reaches `/bin/sh -c` as literal text for the SHELL to interpret.

    `v=#{pane_dead_status}; echo "${v:-N}" > ...` rather than a bare `echo
    #{pane_dead_status} > ...`: `#{pane_dead_status}` is EMPTY, not present as some
    fallback digit, for a harness killed by a signal (SIGKILL/SIGTERM/SIGSEGV; measured
    against tmux 3.7c) — an unqualified `echo` would then write an empty line, which
    `state.exit_code`'s `int(...)` cannot parse, silently reading back as `None` exactly
    like "nothing was ever recorded." The shell's own `${v:-N}` (not `${v-N}`, which only
    substitutes for UNSET, not empty) closes that at the point of writing, so the file
    this hook produces is always a parseable integer — `_UNKNOWN_DEATH_CODE` on a signal
    death, the real status otherwise. Verified against tmux 3.7c for both.
    """
    action = ('run-shell "v=#{pane_dead_status}; echo '
             f'\\"\\${{v:-{_UNKNOWN_DEATH_CODE}}}\\" > '
             f'\\"\\${_EXIT_PATH_ENV}\\""')
    return ["tmux", "-L", socket, "set-hook", "-p", "-t", harness_pane, "pane-died", action]


def _pane_died_teardown_hook_argv(*, socket: str, harness_pane: str) -> list[str]:
    """`pane-died[1]`: ends the session. Nothing else — see the module docstring's
    "Teardown is its own hook" section for why this is never combined with the write
    hook above.

    **Must be installed AFTER `_pane_died_write_hook_argv`, never before — this is not
    a style preference, it is the entire fix.** Verified by hand against tmux 3.7c: an
    UNINDEXED `set-hook -p -t <pane> pane-died '<action>'` call does not merely overwrite
    index 0 of an existing array — it REPLACES THE WHOLE ARRAY, silently deleting any
    `[1]` that was already there. So installing this hook first and the write hook
    second wipes teardown before the harness ever runs, and the exact hang this pair of
    hooks exists to close comes back — reproduced end to end (`RESULT: TIMEOUT`, session
    left attached) by swapping the two calls in `cmd_launch`. `cmd_launch`'s own call
    order (write, then this one) is the only thing enforcing the requirement; nothing in
    the type system does, which is why it is pinned by a dedicated ordering test
    (`Launch.test_the_write_hook_is_installed_before_the_teardown_hook`) as well as by
    `tests/test_frame_tmux_integration.py`'s own test of this exact array-replacement
    behaviour against a real server.
    """
    return ["tmux", "-L", socket, "set-hook", "-p", "-t", harness_pane, "pane-died[1]",
           "kill-session"]


#: Which `resize-pane` flag re-asserts a slot's fixed dimension: `-y` (rows) for the
#: horizontal strips, `-x` (columns) for the side columns — the same axis `layout.py`'s
#: own `-v`/`-h` split direction already encodes for the same slots, read here rather
#: than re-derived a third way.
_RESIZE_FLAG = {"top": "-y", "bottom": "-y", "left": "-x", "right": "-x"}

#: Every real pane id tmux's own `-P -F '#{pane_id}'` has ever reported (`%<digits>`,
#: confirmed against tmux 3.7c and never observed otherwise). Checked before a value
#: read off `split-window`'s stdout is trusted as a pane id, because `_resize_hook_argv`
#: interpolates it directly into a hook ACTION STRING tmux later re-parses as a command
#: line — the exact construction the module docstring's "constant string" section bans
#: for `status_path`, for the same reason: something interpolated into an action must be
#: safe BY CONSTRUCTION, not merely safe because the one program that currently produces
#: it (tmux itself) happens to be well-behaved. A value that fails this check is treated
#: the same as `split-window` reporting no id at all (see the empty-string check right
#: below this) — that one panel simply gets no resize-hook entry, rather than gambling
#: that whatever the string actually was cannot corrupt the action tmux re-parses.
_PANE_ID_RE = re.compile(r"%\d+")

#: tmux's own answer, verbatim, for a `set-hook` call naming an event this binary does
#: not recognise at all — confirmed by hand against a real tmux 3.7c with a fabricated
#: hook name (`invalid option: <name>`, generic `set-hook` argument-parsing text, not
#: specific to any one hook's name). Checked against a FAILED install's own stderr
#: rather than trusted only up front: `RESIZE_HOOK_FLOOR` is a fast path to skip the
#: attempt for a version already KNOWN too old, not the only thing standing between an
#: operator and a loud, recurring error if that constant ever turns out to be wrong —
#: this is what makes the mechanism safe BY CONSTRUCTION rather than by the constant
#: being right (see `_report_tmux_failure`'s call site below for how the two combine).
_INVALID_HOOK_NAME = "invalid option"


def _resize_hook_argv(*, socket: str, harness_pane: str, panes: dict[str, str]) -> list[str]:
    """`window-resized`: re-asserts every fixed-size panel's dimension after a resize.

    Measured against tmux 3.7c (see `layout.panel_argvs`'s own docstring): tmux's layout
    engine redistributes EVERY pane proportionally whenever the window's size changes,
    `-l size` notwithstanding — a 120x30 frame grown to 200x50 stretched two one-row
    panels to 8 and 7 rows apiece, and only snapped back to 1 row because that particular
    shrink happened to be an exact round trip of the same grow. `resize-pane -t <pane>
    -y/-x <size>` re-applies the intended size directly and was verified by hand to hold
    across repeated grows AND shrinks to arbitrary sizes, not just a round trip.

    Installed as a WINDOW hook (`-w`, scoped via *harness_pane* — tmux resolves its
    containing window from the pane), not a session or global one, so a sibling frame's
    own window is left untouched. Fires on EVERY resize for the life of the window, not
    only the first, since an operator's terminal can be grown and shrunk any number of
    times.

    *panes* maps each slot that was actually split to the pane id tmux reported for it —
    never a slot whose `split-window` itself failed, since there is nothing there to
    resize. Pane ids are tmux's own (`%<digits>`), never operator- or plane-derived, so
    this action is safe to build directly: there is no hostile string here for a nested
    tmux-quote layer to mangle, unlike `status_path` (see `_EXIT_PATH_ENV`'s own
    docstring).

    A SINGLE `set-hook` call, not one per slot: nothing else in this codebase installs a
    `window-resized` hook, so there is no existing index to collide with — contrast
    `pane-died`, which needed two INDEXED hooks specifically because two independent
    actions shared one event (see `_pane_died_teardown_hook_argv`'s own docstring). One
    action string chaining every slot's `resize-pane` with `;` covers all of them.
    """
    action = " ; ".join(
        f"resize-pane -t {pane} {_RESIZE_FLAG[slot]} {layout.SLOT_SIZE[slot]}"
        for slot, pane in panes.items())
    return ["tmux", "-L", socket, "set-hook", "-w", "-t", harness_pane,
           "window-resized", action]


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


def _menu_clients(socket: str, session: str) -> list[str]:
    """Every client `tmux -L socket list-clients -t session` reports for *session*, now.

    `cmd_menu`'s own query for `-c`'s answer (see `menu.menu_argv`'s docstring for why
    `-t` alone cannot choose the client) — same shape as `_live_sessions` above, and for
    the same reason: `list-clients` exits non-zero with nothing on stdout for the
    ordinary "nothing attached right now" case, which `splitlines()` already turns into
    an empty list with no special-casing needed. The `except` guards tmux vanishing
    between calls, not that ordinary case.
    """
    try:
        out = subprocess.run(["tmux", "-L", socket, "list-clients", "-t", session,
                              "-F", "#{client_name}"],
                             capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return []
    return [line.strip() for line in out.stdout.splitlines() if line.strip()]


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

    Called twice by `cmd_launch`, for two different gaps the hooks alone cannot cover:
    once EAGERLY, right after both hooks are installed, to close the install race the
    module docstring measures (a harness that died before the hooks existed is never
    caught by them, because a hook only fires for a death AFTER it exists); and again as
    a fallback after `attach` returns, for whatever the eager check could not have seen
    yet. `remain-on-exit` (armed from the very first moment via `_PLACEHOLDER_CONF`, or
    directly when a server is already running — see `cmd_launch`) is what keeps the pane
    around long enough to still be askable either time — without it, this query would
    simply find no such pane, exactly like the hooks found no such event.

    ``None`` means "cannot tell" (the pane is alive, or the query itself failed) — never
    a fabricated 0. An EMPTY `#{pane_dead_status}` is a THIRD case, distinct from both:
    the pane is confirmed dead (`#{pane_dead}` is `1`) but tmux itself has no status to
    report — measured against tmux 3.7c for a harness killed by a signal
    (SIGKILL/SIGTERM/SIGSEGV), not merely a hypothetical. Returning `None` for THAT case
    would tell `cmd_launch` "alive, go ahead and attach" for a pane that is provably
    dead — the exact hang the eager check exists to close, reopened by a signal instead
    of a timing race. See `_UNKNOWN_DEATH_CODE`.
    """
    try:
        dm = subprocess.run(["tmux", "-L", socket, "display-message", "-p", "-t", harness_pane,
                             "#{pane_dead}:#{pane_dead_status}"],
                            capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    if dm.returncode != 0:
        return None
    dead, _, status = dm.stdout.strip().partition(":")
    if dead != "1":
        return None
    status = status.strip()
    if status.lstrip("-").isdigit():
        return int(status)
    return _UNKNOWN_DEATH_CODE


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
    live_before = _live_sessions(SOCKET)
    if live_before:
        # Arm `remain-on-exit` by construction here, not by coincidence. The placeholder
        # `-f` config above only takes effect if THIS `new-session` call is what starts
        # the tmux server — but a server on `SOCKET` may already be running for a reason
        # that has nothing to do with charter (an operator's own `tmux -L charter
        # new-session`, say), in which case `-f` is silently ignored and remain-on-exit
        # stays at tmux's own default (off) until SOME charter frame happens to
        # `source-file` it. Verified by hand: without this, a harness that dies in the
        # race window against such a server returns the wrong code (1, not its own)
        # deterministically — not a hang, since the session simply vanishes with it, but
        # still wrong, and worth closing the same way the placeholder closes the
        # first-frame-ever case.
        arm_cmd = ["tmux", "-L", SOCKET, "set", "-g", "remain-on-exit", "on"]
        arm = subprocess.run(arm_cmd, capture_output=True, text=True, timeout=15)
        if arm.returncode != 0:
            _report_tmux_failure("arming remain-on-exit ahead of an already-running server",
                                 arm_cmd, arm)
    state.reap(live_before)

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

    # `[frame] slots` accepts `left`/`right` (`instance.FRAME_SLOTS`, sized by
    # `layout.SLOT_SIZE`) even though `frame.slots.SLOTS` — the RENDERER registry —
    # does not implement either one yet. Left unfiltered, `panel_argvs` below would
    # still split a real pane for it; `panel.run` correctly refuses or exits 2 (Task
    # 7's own "no empty pane" rule), but with `remain-on-exit on` keeping that pane
    # alive, the operator is left with a permanently dead, wrapped-error 22-column
    # pane and no explanation at the point the frame actually came up. Skipping an
    # unimplemented slot here instead means the harness pane simply keeps that space —
    # the same degrade `visible_slots` itself already makes under a tight terminal —
    # and one message, printed once, says why up front rather than leaving the operator
    # to puzzle out a dead pane's own stderr.
    unimplemented = sorted({s for s in slots if s not in frame_slots.SLOTS})
    if unimplemented:
        util.warn(f"charter frame: no renderer yet for {', '.join(unimplemented)} — "
                  f"not drawing {'it' if len(unimplemented) == 1 else 'them'}; "
                  f"the harness pane keeps that space instead")
        slots = [s for s in slots if s not in unimplemented]

    env = dict(os.environ, CHARTER_SESSION_ID=fid)
    # Belt and braces, not the fix itself: every pane (harness or panel) measures its
    # OWN tty rather than trusting these (`frame/slots.py`, `frame/panel.py`), so a
    # stale value here cannot mislay anything charter draws. But `env` is inherited
    # WHOLE by every process tmux starts on this launch — the harness's shell among
    # them — and COLUMNS/LINES describe the LAUNCHING terminal, not any pane this frame
    # creates. Left in place, a shell inside a pane that echoes them back (or a
    # program, charter's own or not, still reading `$COLUMNS` the way `tui.term_width`
    # deliberately does for the status line) would report the wrong size for no reason
    # charter needs to accept.
    env.pop("COLUMNS", None)
    env.pop("LINES", None)
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

    env_cmd = _exit_path_env_argv(socket=SOCKET, session=fid, status_path=str(status_path))
    env_set = subprocess.run(env_cmd, env=env, capture_output=True, text=True, timeout=15)
    if env_set.returncode != 0:
        _report_tmux_failure("carrying the exit-status path", env_cmd, env_set)
        util.warn("charter frame: continuing without it — the exit code may not be "
                  "recorded for this frame")

    # Ties this session to its own id BEFORE anything else can ask for it — the hotkey
    # bind's action (`charter frame-menu`) and every menu item's own action (`charter
    # frame-action <id>`) both resolve `$CHARTER_SESSION_ID` from a `run-shell`-spawned
    # process's environment, and without this call a frame beyond the first sharing
    # `SOCKET` would silently resolve the FIRST frame's id instead of its own (see
    # `_session_id_env_argv`'s own docstring for what was verified by hand).
    sid_cmd = _session_id_env_argv(socket=SOCKET, session=fid)
    sid_set = subprocess.run(sid_cmd, env=env, capture_output=True, text=True, timeout=15)
    if sid_set.returncode != 0:
        _report_tmux_failure("carrying the frame id to its own hotkey menu", sid_cmd, sid_set)
        util.warn("charter frame: continuing without it — the hotkey menu may not find "
                  "this frame's own actions")

    # The menu itself: what the hotkey actually opens. A single "Detach" entry — the
    # spec's own words, "Detach is allowed and prints how to reattach" — proves the
    # mechanism end to end (bind → menu → opaque id → real command) without inventing a
    # feature this task was never asked to build. `-s fid` (not `-t`): `detach-client`'s
    # `-s` targets every client attached to a SESSION, `-t` a single CLIENT — this frame
    # normally has exactly one attached client, but `-s` is correct even if it ever has
    # more than one.
    menu.record(fid=fid, entries=[
        ("Detach", ["tmux", "-L", SOCKET, "detach-client", "-s", fid]),
    ])

    write_hook_cmd = _pane_died_write_hook_argv(socket=SOCKET, harness_pane=harness_pane)
    write_hook = subprocess.run(write_hook_cmd, env=env, capture_output=True, text=True, timeout=15)
    if write_hook.returncode != 0:
        _report_tmux_failure("installing the exit-status hook", write_hook_cmd, write_hook)
        util.warn("charter frame: continuing without it — the exit code may not be "
                  "recorded for this frame")

    teardown_hook_cmd = _pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=harness_pane)
    teardown_hook = subprocess.run(teardown_hook_cmd, env=env, capture_output=True, text=True,
                                   timeout=15)
    if teardown_hook.returncode != 0:
        _report_tmux_failure("installing the session-teardown hook", teardown_hook_cmd,
                             teardown_hook)

    # Closes the install race directly, rather than merely working around its symptom. A
    # harness that died in the window between `new-session` starting it and the hooks
    # above actually registering leaves them registered for an event that ALREADY
    # happened — hooks do not fire retroactively, so neither fires at all, and nothing is
    # left to run `kill-session`. Verified by hand against a real tmux 3.7c: `attach`
    # below then blocks FOREVER on a session `remain-on-exit` is legitimately keeping
    # alive, not merely returns 0 early — a worse failure than the one the module
    # docstring describes, because there is no return to read a code from at all. Asking
    # directly, immediately, closes it: if the pane is already dead, the job the hooks
    # would have done (record the code, end the session) is finished right here, and
    # `attach` is never even called against a session already known to be over.
    code = _query_pane_dead_status(SOCKET, harness_pane)
    if code is not None:
        state.record_exit(fid, code)
        kill_cmd = ["tmux", "-L", SOCKET, "kill-session", "-t", fid]
        kill = subprocess.run(kill_cmd, env=env, capture_output=True, text=True, timeout=15)
        if kill.returncode != 0:
            _report_tmux_failure("ending the frame after an early death", kill_cmd, kill)

    attach = None
    attach_cmd = None
    refused_to_attach = False
    if code is None:
        if teardown_hook.returncode != 0:
            # Refuse to attach rather than risk it: without the teardown hook, a crash
            # ANY time later in the harness's life would leave `attach` blocked forever
            # with nothing to end the session — the same hang the eager check above
            # exists to close, just moved later and made permanent for this frame. The
            # harness keeps running (it was already started, detached); the operator can
            # still attach manually and accept that risk themselves.
            refused_to_attach = True
            util.err("charter frame: refusing to attach — the session-teardown hook "
                     "failed to install, so a crash later would block `attach` forever "
                     "with nothing to end the session. The harness is still running, "
                     f"detached; reattach manually if you must: "
                     f"tmux -L {SOCKET} attach -t {fid}")
        else:
            panel_cmds = layout.panel_argvs(slots=slots, session=fid, socket=SOCKET,
                                            charter_argv=[sys.executable, "-m", "charter"],
                                            harness_pane=harness_pane)
            # Zipped with `slots`, not just iterated: `_resize_hook_argv` below needs to
            # know WHICH slot each successfully-created pane belongs to (for its size and
            # its resize-pane flag), and `panel_argvs` returns exactly one command per
            # slot, in the same order (see its own docstring).
            panes: dict[str, str] = {}
            for slot, cmd in zip(slots, panel_cmds):
                p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=15)
                if p.returncode != 0:
                    # Reported, not fatal: one decorative panel failing to draw must not
                    # take down a harness pane that is already up and running (correction
                    # 2 asks for every return code to be CHECKED, not every failure
                    # refused).
                    _report_tmux_failure("drawing a panel", cmd, p)
                    continue
                pane_id = p.stdout.strip()
                if pane_id and _PANE_ID_RE.fullmatch(pane_id):
                    panes[slot] = pane_id

            if panes and v < tmuxctl.RESIZE_HOOK_FLOOR:
                # Below RESIZE_HOOK_FLOOR, `window-resized` is not a hook name THIS
                # tmux recognises at all — `set-hook` fails with `invalid option:
                # <name>` for any name it does not know (see RESIZE_HOOK_FLOOR's own
                # docstring for exactly what was, and was not, confirmed by hand) —
                # skip the attempt rather than printing that confusing text on every
                # single launch. One quiet, honest note instead, naming the real
                # consequence (item 4's own standard: every degrade in this launcher
                # says what it costs).
                util.warn(f"charter frame: tmux {v[0]}.{v[1]} predates the "
                         f"resize-recovery hook (needs "
                         f"{tmuxctl.RESIZE_HOOK_FLOOR[0]}.{tmuxctl.RESIZE_HOOK_FLOOR[1]}+)"
                         f" — panels may drift out of shape if this terminal is resized")
            elif panes:
                # Only once any panel actually exists — a resize hook with nothing to
                # resize would just be a wasted `set-hook` call, and (per the module
                # docstring's "belt and braces" framing) every pane already measures its
                # own tty on every repaint regardless, so this hook is purely cosmetic
                # geometry upkeep, not something a launch's correctness depends on.
                resize_cmd = _resize_hook_argv(socket=SOCKET, harness_pane=harness_pane,
                                               panes=panes)
                resize = subprocess.run(resize_cmd, env=env, capture_output=True, text=True,
                                        timeout=15)
                if resize.returncode != 0:
                    if _INVALID_HOOK_NAME in (resize.stderr or ""):
                        # RESIZE_HOOK_FLOOR believed this tmux would recognise the
                        # hook name and it does not — the constant is wrong, not the
                        # launch. Trust what THIS tmux just said over the constant and
                        # degrade the same quiet way the version-gate above already
                        # does, rather than report it as a broken integration (a
                        # capability ceiling, not a failure — see the module's own
                        # "belt and braces" framing and `harness.base.Deficit`'s same
                        # philosophy for a harness-level capability gap).
                        util.warn("charter frame: this tmux does not support the "
                                 "resize-recovery hook — panels may drift out of "
                                 "shape if this terminal is resized")
                    else:
                        _report_tmux_failure("installing the resize hook", resize_cmd, resize)
                        util.warn("charter frame: continuing without it — panels may "
                                 "drift out of shape if this terminal is resized")

            # `split-window` makes the newly created pane the ACTIVE one by default, so
            # after every slot has been drawn, the LAST panel drawn — not the harness —
            # has focus, and an interactive harness never receives a keystroke (measured:
            # `%2 active=1, %0 active=0` after two splits). Pre-existing in
            # `layout.panel_argvs`'s own split ordering, not something this diff
            # introduced, but leaving the frame in a state the operator can actually type
            # into is this launcher's job.
            select_cmd = ["tmux", "-L", SOCKET, "select-pane", "-t", harness_pane]
            select = subprocess.run(select_cmd, env=env, capture_output=True, text=True,
                                    timeout=15)
            if select.returncode != 0:
                _report_tmux_failure("focusing the harness pane", select_cmd, select)

            # No capture_output: this is the operator's own terminal for as long as the
            # harness runs, not an admin command whose output charter should own.
            attach_cmd = ["tmux", "-L", SOCKET, "attach", "-t", fid]
            attach = subprocess.run(attach_cmd, env=env)

            code = state.exit_code(fid)
            if code is None:
                # A second ask, for whatever gap the eager check above could not have
                # seen yet (the harness was still alive at that point and died later —
                # the ordinary case, caught here by the hooks, or a rarer one where they
                # never actually reached the server despite reporting success).
                code = _query_pane_dead_status(SOCKET, harness_pane)

    live_after = _live_sessions(SOCKET)
    state.reap(live_after)
    if code is not None:
        return code
    if refused_to_attach:
        # Nonzero, deliberately distinct from the `still live` detach path below: the
        # harness never ran interactively and charter has no way to learn its real exit
        # code, so a script or `&&` chain must see this as a failure rather than the
        # quiet success an operator's own deliberate detach is allowed to be.
        return 1
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


def cmd_menu(args) -> int:
    """Open this frame's own menu. The hotkey bind's only action — see `conf_text`.

    `args` is unused; this exists purely so `cli.py` has a handler to point `charter
    frame-menu` (registered as a top-level command — see `menu.py`'s own docstring for
    why not `frame menu`) at. `fid` is resolved from `$CHARTER_SESSION_ID`, carried
    session-scoped via `_session_id_env_argv` rather than baked into the bind's own text
    (see `conf_text`'s docstring for why that split is load-bearing, not incidental):
    the SAME bind text is shared by every frame on `SOCKET`, so the frame it opens a menu
    FOR has to be resolved here, at the moment the key actually fires, never earlier.

    Resolves the CLIENT to draw the menu on before ever calling `display-menu` — `-t`
    alone does not choose it (verified by hand against tmux 3.7c with two frames attached
    in two terminals: `-t fid` rendered the WRONG frame's menu on the wrong screen; see
    `menu.menu_argv`'s own docstring). `_menu_clients` asks `list-clients` directly,
    scoped to this session, rather than ever falling back to tmux's own "most recently
    active" default. Zero clients (nothing attached right now — a keypress landing mid
    detach, say) is a quiet no-op: `display-menu` would just fail with tmux's own "no
    current client", and there is no screen left to report that failure on anyway.
    Several clients (the same session open in two terminals at once) picks the FIRST one
    reported — any client actually watching THIS session is correct by construction,
    `display-menu -c` only ever accepts one, and there is no way to show a menu on two
    screens at the same time.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    clients = _menu_clients(SOCKET, fid)
    if not clients:
        return 0
    return subprocess.run(menu.menu_argv(fid, SOCKET, client=clients[0])).returncode


def cmd_action(args) -> int:
    """Run one menu entry by its opaque id. The only path from a menu to a real command.

    `args.action_id` is never anything but `a<N>` shaped text arriving on `charter
    frame-action`'s own command line (see `menu.py`'s module docstring for why that is a
    top-level command rather than `frame action`) — `menu.resolve` is the one place an id
    turns back into the argv it names, and `subprocess.run` below takes that argv as a
    LIST, never a shell string, so nothing an id could ever resolve to is re-parsed by
    anything on the way to running.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    argv = menu.resolve(fid, args.action_id)
    if not argv:
        util.err(f"charter frame-action: unknown action {args.action_id!r}")
        return 2
    return subprocess.run(argv).returncode
