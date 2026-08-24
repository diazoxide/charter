"""`charter <harness>` — run the harness inside charter's frame.

The launcher does NOT exec tmux, and that is measured rather than stylistic: an attached
`tmux new-session` returns 0 whatever its command exited with (tmux 3.7c). So the status
is carried out of band — normally by a pair of `pane-died` hooks, with a direct query as
a fallback for the race described below — and this process waits for tmux, reads the
recorded (or queried) code, and exits with it. `exec` survives only on the bypass path,
where there is no frame in the way.

**There are TWO paths through this module, and almost everything below is about the
first.** Started from a plain terminal, charter runs the frame as a SESSION on a private
server of its own (`SOCKET`) and blocks in `attach`. Started from inside a tmux the
operator already has (`$TMUX`, read by `tmuxctl.operator_server`), it runs the frame as a
WINDOW on THEIR server, writes not one option or key binding of theirs, and watches the
harness pane instead of attaching — see `_launch_in_operator_tmux`, which is where every
difference lives, and `docs/frame.md` for what the second path costs. The paragraphs
below — the private server, `-f` versus `source-file`, both `pane-died` hooks, and the
install race they close — describe the FIRST path only; the second has no hooks at all,
for a reason `_launch_in_operator_tmux` gives.

**Every frame on charter's own server shares one tmux server (`SOCKET`), told apart by
session name (the frame id), not one server per frame.** That single choice is what makes the rest of this module
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

**A command that dies before the frame is drawn is reported AFTER the fact, never
refused before it (#384).** The eager `#{pane_dead}` check above is exactly what makes
that death total: it catches the dead pane, runs `kill-session`, and because `code is
not None` from that moment the entire attach branch — panels, `select-pane`, `attach` —
is skipped, so there is no pane, no attach, and nothing drawn. Measured under a pty
against a real tmux 3.7c: **zero bytes**, both for `charter frame -- nosuchthing` and
for `charter frame -- sh -c 'echo boom >&2; exit 9'` — a far wider class than a missing
binary. A `shutil.which(argv[0])` pre-check would not have covered that class and would
have cost something real, because what `charter frame --` accepts is TMUX's rule, not
charter's: verified against 3.7c that a SINGLE argument is handed to a shell (so
`charter frame -- 'ulimit -n; exit 3'` is a whole command line — builtins, `;`,
redirection) while TWO OR MORE are `execvp`'d directly. `which` over the first form asks
the wrong question of text that is not even one word; over the second it is a prediction
where a real answer arrives for free a few milliseconds later (a binary that resolves
can still exit 127 for its own reasons, or carry a broken shebang). So nothing is
refused, and `early_death_message` builds the report out of what tmux actually did —
quoting the pane (`_pane_last_words`, read BEFORE `kill-session` destroys it) when the
shell already said what was wrong, and answering for itself when a failed `execvp` left
the pane empty and a bare exit 1.

This is the one paragraph in this run that is NOT about the first path alone.
`_launch_in_operator_tmux` makes the same eager ask for the same reason and skips the
same everything after it, so the same command dies just as silently there — more so, in
fact, since the operator is looking at that tmux the whole time and their window never
even changes. Both call sites report, both read the pane before they close it, and both
say nothing for a clean 0.

**Every tmux command here goes through `frame/tmuxctl.py`, and the timeout is the
reason.** `cmd_launch` used to issue eleven `subprocess.run(…, timeout=15)` calls of its
own with nothing catching `subprocess.TimeoutExpired`. Ten of them run AFTER
`new-session` has already started the harness detached, so a wedged tmux server did not
merely fail one command: it raised a traceback out of the launcher, `cli.main` filed a
charter crash report for it, and the operator was left with a live agent session, no
reattach line, and a bug report pointing at the wrong repository. `tmuxctl.run` folds a
timeout into a return code (`tmuxctl.TIMED_OUT`) so all eleven degrade down the paths
they already had for a tmux that merely says no, and `tmuxctl.report_failure` is called
for each by default rather than remembered at each call site.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import time

from . import config, harness, util, workspace
from .frame import gather, layout, menu, state, tmuxctl
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

#: The second value carried the same out-of-band way, for the same reason: the
#: interpreter the hotkey bind and every menu action run charter with. Owned by
#: `tmuxctl` so `frame/menu.py` — which cannot import this module without a cycle —
#: spells the same name; see `_charter_py_env_argv` and `conf_text`.
_CHARTER_PY_ENV = tmuxctl.CHARTER_PY_ENV

#: What `_query_pane_dead_status` returns for a pane confirmed dead (`#{pane_dead}` is
#: `1`) whose `#{pane_dead_status}` tmux itself could not report — measured against tmux
#: 3.7c: EMPTY, not negative, for a harness killed by SIGKILL/SIGTERM/SIGSEGV. `None`
#: means "cannot tell" everywhere in this module (see `_query_pane_dead_status`'s own
#: docstring) and must never be confused with "dead, but the real number isn't known" —
#: the pane genuinely IS gone either way, so returning `None` here would send
#: `cmd_launch` on to `attach` a session with nothing left to end it, recreating the
#: exact hang the eager check exists to close, just from a signal instead of a race.
_UNKNOWN_DEATH_CODE = 1

#: The one format every "is the harness still there?" query asks for, named once so the
#: two readers of its three possible answers (`_pane_state`, and the fake in
#: `tests/test_frame_launcher.py`) cannot drift apart. See `_pane_state` for what an
#: EMPTY answer means, which is neither of the two obvious ones.
_DEAD_FORMAT = "#{pane_dead}:#{pane_dead_status}"

#: The format that reports the size of the WINDOW a pane is in, rather than the pane's
#: own. Used only inside an operator's tmux: `os.get_terminal_size()` there measures the
#: pane `charter` was typed in, which is a fraction of the window the frame gets whenever
#: they have a split open, and sizing the slots from it would drop panels a full-width
#: window has ample room for.
_WINDOW_SIZE_FORMAT = "#{window_width}:#{window_height}"

#: How long `_wait_for_harness` leaves between asks. There is no `attach` to block on
#: inside an operator's tmux — the operator is already attached, to their own server —
#: so the launcher watches the pane instead, and this is the cost of that: one local
#: `display-message` against a unix socket, four times a second, for the life of the
#: frame. Deliberately NOT closed with a `pane-died` hook signalling `wait-for`: the only
#: hook that could wake this launcher would have to fire BEFORE the one that reads the
#: pane's status, and tmux runs a hook array in index order with no way to interleave a
#: read between them, so the wake would race the teardown for the answer it exists to
#: collect. Four asks a second is a cost; a lost exit code is a defect.
_POLL_SECONDS = 0.25


#: tmux's own trailer, drawn INTO a dead pane by `remain-on-exit` and therefore the last
#: line of every capture `_pane_last_words` ever takes (measured against tmux 3.7c: `Pane
#: is dead (status 127, Sun Aug 23 18:15:16 2026)`). Stripped, because repeating it would
#: be charter answering in tmux's words immediately after answering in its own — and
#: because the timestamp makes the line different on every run, which is exactly the kind
#: of noise an operator learns to skip past. Safe by construction rather than by tmux's
#: wording holding still: a trailer that stopped matching this is repeated back, which is
#: merely untidy, never silent.
_PANE_IS_DEAD = re.compile(r"^Pane is dead\b")

#: How many of a dead pane's own lines charter repeats back. A TAIL, not a head:
#: `capture-pane -S -` returns the pane's whole history (up to `[frame] history-limit`,
#: 50 000 by default) and the reason a thing died is at the end of what it printed. Ten
#: covers a shell's one-line `command not found` and a short traceback, and keeps a
#: harness that managed a screenful on its way out from burying charter's own line.
_PANE_LINES_SHOWN = 10


def bypass(argv: list[str]) -> int:
    """Run the harness with no frame at all — `exec`, so the exit code needs no help.

    Correct ONLY here. `cmd_launch`'s frame path never execs tmux, because an attached
    `tmux new-session` returns 0 regardless of what ran inside it (see the module
    docstring) — but there is no tmux in the way on this path, so the exit code an exec'd
    process carries out is already the real one.

    **A missing harness binary is a condition, not a charter bug.** `os.execvp` raises
    `FileNotFoundError` for a `claude` that is not installed — the most likely FIRST-RUN
    state of this whole feature — and an uncaught one reached `cli.main`'s `except
    Exception`, which files a crash report against charter and re-raises a traceback.
    `cli.main` already carves out exactly this class twice (`contain.Refused`,
    `util.ProcTimeout`), both noting that filing such a condition as a bug "sends whoever
    reads it looking in the wrong repository"; a harness charter was asked to start and
    could not find belongs in the same set. Caught HERE rather than as a third clause up
    there, because this is the only place in charter that execs an operator-named binary
    and the message can name what is missing.

    127 and 126 are the shell's own numbers for the two cases, so `charter claude &&
    …` behaves the way `claude && …` would have.
    """
    try:
        os.execvp(argv[0], argv)
    except FileNotFoundError:
        util.err(f"charter: {argv[0]} is not installed, or not on $PATH.\n"
                 f"  charter cannot start a harness it cannot find — install {argv[0]}, "
                 f"or run a different one: charter harness list")
        return 127
    except PermissionError:
        util.err(f"charter: {argv[0]} is not executable — check its permissions")
        return 126
    return 127  # unreachable; execvp either replaces this process or raises


def no_renderer_message(missing: list[str]) -> str:
    """The one sentence for `[frame] slots` naming a slot charter cannot draw yet.

    Shared by `frame_ready` and `doctor.check_frame` rather than written twice, for the
    same reason both of them exist: this is a standing property of the configuration and
    the build, and two copies of a standing fact drift into two different facts.
    """
    return (f"no renderer yet for {', '.join(missing)} — charter sizes and accepts "
            f"{'it' if len(missing) == 1 else 'them'} in `[frame] slots` but draws "
            f"nothing there, so the harness pane keeps that space")


def frame_ready() -> tuple[int, str, str]:
    """Can a frame run on this machine right now, and what will it not be able to do?
    ``(exit code, util.* level, text)`` — read-only: `tmuxctl.version()` and
    `config.FRAME`, nothing started, nothing written.

    Mirrors `cmd_launch`'s own gate exactly, not a stricter one: a few lines into
    `cmd_launch`, tmux below `tmuxctl.FLOOR` warns and the launch CONTINUES, and only
    `tmux` being entirely absent (`version() is None`) makes `cmd_launch` refuse
    outright. Refusing here on anything short of that would report a frame this same
    launcher goes on to draw regardless — a probe that lies about `cmd_launch`'s own
    behaviour is worse than one that runs nothing at all.

    **The three STANDING conditions are reported here and nowhere else.** All three used
    to be `util.warn` calls inside `cmd_launch` (or, for the resize hook, inside
    `_draw_panels`), and all three were measured to be unreadable there: `util.warn` for
    an unimplemented slot lands 86 bytes before tmux's own `\\x1b[?1049h`, so the
    operator's terminal switches to the alternate screen milliseconds later and the line
    is restored to view only when the frame EXITS. A warning printed where it cannot be
    read is worse than silence, because it creates a record that the operator was told.
    None is per-launch news anyway — a tmux below the floor, a tmux below
    `tmuxctl.RESIZE_HOOK_FLOOR`, and a configured slot with no renderer are true on every
    launch on this machine and this plane until something changes. They are capability
    ceilings, so they belong to the two surfaces built to report ceilings on demand: this
    one, and `doctor.check_frame`. (A per-launch notice mechanism, for conditions specific
    to one launch, is deliberately NOT built here.)

    **`RESIZE_HOOK_FLOOR` is the third, and it was the one this function did not know
    about (#387).** It sits ABOVE `FLOOR`, so an operator on tmux 3.2 passed the floor
    cleanly, saw a green tick on both ceiling surfaces, and silently had no resize
    recovery at all — the gap being closed here. It does NOT change this function's exit
    code, for the same reason the other two do not: `cmd_launch` draws the frame
    regardless, and a probe stricter than the launcher lies about the launcher.

    Two callers share this, both read-only for the same reason `charter/news.py`
    requires of a `check:` (reads, never acts; and this module's own tmux calls all go
    through `tmuxctl.run`, which is time-boxed, so neither can hang): `--probe` on every
    `charter <harness>`/`charter frame` launcher (`cmd_launch` below), for an operator
    to run by hand, and the top-level `charter frame-probe` (`cmd_probe`) that a news
    `check:` names — see `cmd_probe`'s own docstring for why the check cannot simply be
    `frame --probe` itself.
    """
    v = tmuxctl.version()
    if v is None:
        return 1, "err", tmuxctl.absent_message()
    head = f"charter frame: tmux {v[0]}.{v[1]} — a frame can run on this machine"
    ceilings = []
    if v < tmuxctl.FLOOR:
        ceilings.append(tmuxctl.below_floor_message(v))
    if v < tmuxctl.RESIZE_HOOK_FLOOR:
        ceilings.append(tmuxctl.below_resize_hook_message(v))
    missing = frame_slots.unimplemented(config.FRAME["slots"])
    if missing:
        ceilings.append(no_renderer_message(missing))
    if not ceilings:
        return 0, "ok", head
    return 0, "warn", "\n".join([head, *(f"  ↳ {c}" for c in ceilings)])


def _report_probe(code: int, level: str, line: str) -> int:
    getattr(util, level)(line)
    return code


def cmd_probe(args=None) -> int:
    """`charter frame-probe` — read-only, one line, starts nothing. The command a news
    `check:` names.

    A TOP-LEVEL command, not `frame --probe` reached through the escape hatch (which
    also exists — see `cmd_launch`'s own `args.probe` branch): `news._PROBEABLE`
    refuses any command whose parser carries a pass-through positional (#317), and
    every parser `cli._wire` builds — `frame` included — carries `rest`
    (`nargs=argparse.REMAINDER`, the harness's own verbatim argv, the entire point of
    `charter frame -- <cmd>`). That makes `("frame",)` exactly the shape #317 fixed,
    `--probe` or not, so it can never be added to `news._PROBEABLE` no matter what flag
    reaches it. This command takes no arguments at all — nothing here could ever carry
    an argv from a caller — so it is not that shape, and can be (see
    `tests/test_news_probeable.py`).
    """
    return _report_probe(*frame_ready())


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

    `hotkey` opens this frame's own menu: `bind -n {hotkey} run-shell '"$CHARTER_PY" -m
    charter frame-menu "#{client_name}"'`. A key BINDING has no per-session form the way
    `status`/`mouse`/`history-limit` above do — key tables are server-wide in tmux, so
    every frame on `SOCKET` ends up sharing this exact bind text, "last launched wins"
    exactly like `escape-time`/`remain-on-exit`/the `WheelUpPane` bind two lines down
    already do. That is only safe here because the action itself carries no frame
    identity: `charter frame-menu` (`cmd_menu`) resolves the CURRENT session from
    `$CHARTER_SESSION_ID` — carried out of band via `set-environment`, see
    `_session_id_env_argv` — at the moment the key actually fires, never from anything
    baked into this text. A bind that embedded one frame's own id here would start
    opening the WRONG frame's menu the instant a second frame launched, the same trap
    this function's own docstring already names for `mouse`/`history-limit`, just
    reached through a binding instead of a session-scoped `set`.

    `"#{client_name}"` is a SECOND thing this same bind carries, for a DIFFERENT
    reason: which of possibly several clients attached to one frame should see the
    menu. Format expansion resolves `#{client_name}` in the context of whoever's
    keypress is firing the bind — verified by hand with two real ptys attached to one
    session, pressing the hotkey from each in turn: each press's own `run-shell`
    resolved its OWN presser's client name, never the other one's, regardless of which
    was attached first. `charter frame-menu` receives it as a plain argv value (`args
    .client` in `cmd_menu`) and hands it straight to `display-menu -c`. Earlier, this
    module queried `list-clients` and guessed the first one reported when several
    clients were attached — confirmed wrong: pressing the hotkey on the SECOND-attached
    client drew the menu on the FIRST client's screen, worse than tmux's own unscoped
    default single-client guess this module was replacing. Carrying the presser by name
    removes the guess entirely rather than making it a better one.

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

    **`"$CHARTER_PY" -m charter`, never a bare `charter`.** The panels already launch
    charter via `util.self_relaunch_argv()` (inside `layout.panel_command`, which owns
    that half of the panel argv for both the launcher and `cmd_respawn`)
    precisely because the `charter` an operator's `$PATH` resolves may be a
    different install, or no install at all — a `uv tool` shim not on the tmux server's
    own PATH, a checkout run as `python -m charter`. This line had kept the bare name,
    and the failure lands in the worst possible place: `run-shell` reports a non-zero
    command by printing `'charter frame-menu "/dev/ttys020"' returned 127` INTO THE
    HARNESS PANE and dropping it into copy-mode — charter drawing in the one rectangle
    ADR 0018 says it never draws. The interpreter is carried out of band via
    `set-environment` (`_charter_py_env_argv`) rather than interpolated here, for the
    same reason `status_path` is (see `_EXIT_PATH_ENV`): an absolute path re-embedded
    inside this nested tmux-quote layer is one apostrophe away from the silent
    corruption the module docstring measures. Verified against tmux 3.7c that a
    session-scoped `CHARTER_PY` reaches the shell this bind spawns and expands there,
    and that this exact bind text survives `source-file` intact (`list-keys` reads it
    back byte for byte).
    """
    return "\n".join([
        f"set -t {session} status off",
        f"set -t {session} mouse {'on' if mouse else 'off'}",
        f"set -t {session} history-limit {int(history_limit)}",
        "set -g escape-time 0",
        "set -g remain-on-exit on",
        f"bind -n {hotkey} run-shell "
        f"'\"${_CHARTER_PY_ENV}\" -m charter frame-menu \"#{{client_name}}\"'",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        "",
    ])


def _charter_py_env_argv(*, socket: str, session: str) -> list[str]:
    """`set-environment`: the interpreter the hotkey and every menu action run charter
    with.

    `sys.executable`, delivered the same out-of-band way `_exit_path_env_argv` delivers
    `status_path` and for the same two reasons — a single argv value nothing re-parses,
    and a bind/action TEMPLATE that stays free of per-machine text. See `conf_text`'s
    own docstring for what a bare `charter` cost, and `frame/menu.py`'s `menu_argv` for
    the second consumer.

    Session-scoped, not `-g`: two planes on one laptop can be two different charter
    installs (`docs/control-plane.md`'s version pin exists for exactly that), and a `-g`
    write would hand frame N's interpreter to frame N-1 — the same "last launched wins"
    trap `conf_text`'s docstring already names for `mouse`/`history-limit`.
    """
    return tmuxctl.server_argv(socket, "set-environment", "-t", session, _CHARTER_PY_ENV,
                               sys.executable)


#: `PYTHONSAFEPATH`'s real name — a genuine interpreter environment variable, read by
#: `python` itself at startup, not a charter-invented one. Carried alongside
#: `CHARTER_PY_ENV`, never folded into it: see `_charter_pythonsafepath_env_argv`.
_PYTHONSAFEPATH_ENV = "PYTHONSAFEPATH"


def _charter_pythonsafepath_env_argv(*, socket: str, session: str) -> list[str]:
    """`set-environment`: closes the same hole `util.self_relaunch_argv`'s `-P` closes
    for every OTHER self-relaunch site (#390), for the one shape that cannot take a flag.

    `"$CHARTER_PY" -m charter ...` — the hotkey bind (`conf_text`) and every menu item's
    own action (`frame.menu.menu_argv`) — is a shell TEMPLATE shared by every session on
    `SOCKET`, built once and never per-invocation; there is nowhere in it to splice a
    `-P` without re-embedding per-machine text, the exact construction `conf_text`'s own
    docstring already bans for `status_path`. `PYTHONSAFEPATH=1` is `-P`'s own
    environment-variable form — carried the identical session-scoped way
    `_charter_py_env_argv` carries `$CHARTER_PY` itself, so it reaches the same shell
    that call already proves reaches: without this, a pane whose cwd happens to be a
    charter checkout would have the hotkey menu import THAT tree the moment it opens,
    same failure shape as the panel argv, just one layer further from the operator.

    A separate `set-environment` call rather than folded into `_charter_py_env_argv`'s:
    that function's contract is "the interpreter", used elsewhere as exactly that value
    (`docs`, `conf_text`'s own text); overloading it to also carry an unrelated variable
    would break that contract for a caller that only wanted the interpreter path.

    `tmuxctl.server_argv` like every other builder here, even though the only caller
    passes `SOCKET`: this module addresses two servers now, one by name and one by socket
    path, and a hand-built `-L` is correct only until somebody reuses the builder from
    `_launch_in_operator_tmux` — where it would quietly aim a `set-environment` at
    charter's own private server instead of failing.
    """
    return tmuxctl.server_argv(socket, "set-environment", "-t", session,
                               _PYTHONSAFEPATH_ENV, "1")


def _exit_path_env_argv(*, socket: str, session: str, status_path: str) -> list[str]:
    """`set-environment`: carries *status_path* to the write hook's shell out of band.

    One argv value, no shell parsing at all on this side — the whole point (see the
    module docstring). `run-shell`'s own spawned shell later reads it back from its
    inherited environment via `$CHARTER_FRAME_EXIT`, verified by hand to work for a
    SESSION-scoped `set-environment` (no `-g`) reaching a hook fired for a pane in that
    session.
    """
    return tmuxctl.server_argv(socket, "set-environment", "-t", session, _EXIT_PATH_ENV,
                               status_path)


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
    return tmuxctl.server_argv(socket, "set-environment", "-t", session,
                               "CHARTER_SESSION_ID", session)


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
    return tmuxctl.server_argv(socket, "set-hook", "-p", "-t", harness_pane, "pane-died",
                               action)


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
    return tmuxctl.server_argv(socket, "set-hook", "-p", "-t", harness_pane,
                               "pane-died[1]", "kill-session")


#: How many times one slot's panel may be brought back before charter stops trying, per
#: frame. The spec's own number ("a dead panel stays visible with its error, respawns
#: with backoff, and gives up after 3 attempts"). The cap is the entire reason a count
#: exists on disk at all: verified against tmux 3.7c that a pane's `pane-died` hook
#: SURVIVES the `respawn-pane` it triggers, so a panel that dies instantly on every
#: start would otherwise respawn forever with nothing anywhere counting.
_RESPAWN_ATTEMPTS = 3

#: What each attempt waits before bringing the pane back, indexed by attempt number.
#: Backoff rather than three immediate retries because a panel that fails AT STARTUP
#: fails in milliseconds: fired back to back, all three attempts land inside the same
#: broken condition and the whole budget is spent before anything transient (a
#: filesystem hiccup, a plane mid-upgrade replacing charter's own files) could clear.
#: The sleep happens in a `run-shell -b` child, never in tmux's own command queue — see
#: `_panel_died_hook_argv` for why the `-b` is load-bearing and not tidiness.
_RESPAWN_BACKOFF = (1.0, 2.0, 4.0)


#: A frame id as `state.frame_id` mints one — the same alphabet, asked here rather than
#: assumed. `_panel_died_hook_argv` interpolates the id into an action string tmux
#: re-parses, so it gets the treatment `_PANE_ID_RE` already gives a pane id: safe
#: because it was CHECKED, not because of where it came from. `cmd_respawn` reads the id
#: back off that same argv, so a frame whose id could not survive the trip is one whose
#: panels are better left unarmed than armed with a hook that names something else.
_FRAME_ID_RE = re.compile(r"[A-Za-z0-9._-]+")

#: Every character that means something to one of the THREE parsers a `pane-died` action
#: passes through, and the whole of that set for those three grammars. Not a list of
#: inputs known to be bad: a list of the only characters that can change what the text
#: says on the way from `set-hook`'s argv to `/bin/sh`'s.
#:
#: The action is stored as a single `set-hook` argument, so nothing parses it until the
#: hook fires. Then, in order:
#:
#: 1. **tmux expands `#{…}` FORMATS in it.** That is not incidental — it is the whole
#:    mechanism `_pane_died_write_hook_argv` uses to get `#{pane_dead_status}` into a
#:    shell. Measured against tmux 3.7c with the same shape a path would have: an action
#:    holding the literal text ``/opt/py#{pane_id}/x`` reached the shell as
#:    ``/opt/py%1/x``. `#` is therefore as load-bearing as a quote, and MORE dangerous
#:    than one: `#{pane_title}` expands to text the program running in that pane sets
#:    for itself with an escape sequence. This is the parser a first version of this
#:    guard missed entirely, having named itself after quotes and then looked only for
#:    quote characters — the exact failure this codebase keeps paying for, a guard
#:    matching a spelling instead of the property.
#: 2. **tmux parses the result as a command line.** Inside `'…'` everything is literal
#:    except `'` itself.
#: 3. **`/bin/sh -c` parses the inner text.** The interpreter path sits inside `"…"`,
#:    where POSIX gives meaning to exactly `$`, `` ` ``, `\` and `"`.
#:
#: Measured against tmux 3.7c, both directions. An interpreter at
#: ``…/a b;c&d(e)f*g-h,i=j+k@l:m[n]o{p}q!r%s^t~u/fake py`` — every other ASCII
#: punctuation character, plus a space — was reached and received exactly
#: ``-P -m charter frame-respawn top --pane %1 --frame demo-1``, byte for byte. An
#: interpreter under ``…/plain $(touch CANARY) dir/py`` created the canary: the `$( )`
#: really does execute, so this refusal is a guard and not a formality.
_ACTION_METACHARACTERS = "#'\"$`\\"


def _action_word_is_safe(word: str) -> bool:
    """Does *word* still say what it says after all three parses
    :data:`_ACTION_METACHARACTERS` describes?

    The property, not a spelling: a word survives if it carries no character any of those
    three grammars reads as anything but itself, and no control character (a newline is a
    tmux command separator; the rest are unprintable in a `show-hooks` a human has to be
    able to read). Whitespace is left to the caller — the interpreter path is
    double-quoted for the shell and may hold spaces; every word after it is bare and may
    not.
    """
    return bool(word) and not any(
        c in _ACTION_METACHARACTERS or ord(c) < 0x20 or ord(c) == 0x7F for c in word)


def _panel_died_hook_argv(*, socket: str, panel_pane: str, slot: str,
                          fid: str) -> list[str] | None:
    """`pane-died`, scoped to ONE PANEL pane: bring that panel back.

    **Not the harness pane's hook array, and this is the property to keep.** The two
    exit-code hooks (`_pane_died_write_hook_argv`, `_pane_died_teardown_hook_argv`) live
    in the HARNESS pane's own `pane-died` array, where an unindexed `set-hook` replaces
    the whole array — the trap their install order exists to work around. This hook is
    `-p -t <panel pane>`: a different pane, a different option array. Verified against
    tmux 3.7c that installing it leaves the harness pane's `pane-died[0]`/`[1]` read back
    byte-identical, and that a panel dying fires only this hook and not the harness
    pane's `kill-session`. It is unindexed because nothing else installs a hook on a
    panel pane at all — there is no `[1]` here to delete.

    **`run-shell -b`, backgrounded, and both halves of that matter.** Un-backgrounded,
    tmux prints a non-zero command's own `'…' returned N` INTO THE HARNESS PANE and
    drops it into copy-mode — charter drawing in the one rectangle ADR 0018 says it
    never draws, which `conf_text`'s docstring records happening for real. Measured with
    `-b` against 3.7c: a command that both writes to stdout and exits non-zero produced
    nothing in the harness pane and left it out of copy-mode. The second half is timing:
    the command deliberately SLEEPS for its backoff, and a blocking `run-shell` in a
    hook stalls tmux's command queue for that whole time.

    **`tmuxctl.server_argv`, and #408 is what a hand-built `["tmux", "-L", socket, …]`
    cost.** This line spelled `-L` itself. Charter reaches two servers — its own by NAME
    and the operator's by SOCKET PATH — so on the inside-a-tmux path (#381) the same
    string that names the operator's socket was being handed to tmux as a server NAME,
    which would arm a hook against a server that may not exist or, worse, may be some
    other frame's private one. Rather than teach a second place the difference, the
    difference is asked of the one place that already answers it. `cmd_respawn` had the
    identical bug at the other end of the same mechanism and now resolves its server the
    same single way (`state.frame_server`, `_frame_is_live`).

    **The interpreter is interpolated, not read from `$CHARTER_PY`, and that is what makes
    the operator's server reachable at all.** `_charter_py_env_argv` delivers it with
    `set-environment -t <session>`, and `_launch_in_operator_tmux` may not write a session
    option — it is the operator's session, and every new shell they opened would carry it.
    Measured against tmux 3.7c: a `run-shell` fired by a PANE-scoped hook sees the SESSION
    environment (`$CHARTER_PY` set that way arrived intact) and does NOT see the pane's own
    `-e` environment (a `CHARTER_PY` carried on `split-window -e` read back empty). So
    there is no out-of-band channel on that server, and the value has to travel in the
    text. `--frame` travels for the same reason: `cmd_respawn` used to read
    `$CHARTER_SESSION_ID` out of its own environment, which is `_session_id_env_argv`'s
    session option and equally unavailable there.

    ``None`` back means charter will not arm this pane, and every value that reaches the
    text is what decides — never where it came from. *panel_pane* must be tmux's own
    `%<digits>` (`_PANE_ID_RE`), *slot* a key of `frame_slots.SLOTS`, *fid* a
    `state.frame_id` (`_FRAME_ID_RE`), and every word including the interpreter path must
    pass :func:`_action_word_is_safe`. `sys.executable` is the one that is genuinely
    machine-shaped: a path holding a space, `;`, `&`, `(` or `*` was measured to survive
    all three parses byte for byte and is armed; one holding `$( )` was measured to
    EXECUTE, and one holding `#{…}` to be rewritten by tmux's own format expansion before
    any shell saw it, and both are refused. `_arm_panel_respawn` reports the refusal rather
    than swallowing
    it — a frame without panel respawn is a frame that still works, and an operator told
    why beats a hook that quietly runs something else.

    **Four clauses, and each is on its own.** `;` is deliberately NOT in
    :data:`_ACTION_METACHARACTERS` — it changes nothing inside the action's single quotes
    — so a `;` in the pane, the slot or the frame id is caught by that value's own shape
    check and by nothing else. Measured on tmux 3.7c with `_FRAME_ID_RE` widened to
    `re.compile(r".+")` and a frame id carrying no whitespace anywhere,
    ``demo-1;>/…/CANARY``: the hook armed, the panel died, and the canary file existed —
    tmux keeps the `;` literal and `/bin/sh` does not. As shipped that input is not armed
    and no canary appears. The clauses are pinned one at a time in
    `tests/test_frame_launcher.py::PanelRespawnHook`, each case built so that no OTHER
    clause could be what refused it; a first version of those tests used hostile values
    that all carried a SPACE, and three of the four clauses could then be deleted outright
    with the whole suite still green.

    **Single-quoted for tmux, double-quoted for the shell.** `_pane_died_write_hook_argv`'s
    docstring measures the opposite case — an unescaped `$` inside a tmux DOUBLE-quoted
    argument is consumed by tmux's own parsing before any shell sees it. Single quotes are
    what `conf_text`'s hotkey bind already uses for the identical job; the inner `"…"` is
    what lets an interpreter path with a space through, and
    :data:`_ACTION_METACHARACTERS` is the complete set of characters any of the three
    parsers involved would read as something other than themselves.
    """
    words = util.self_relaunch_argv("frame-respawn", slot, "--pane", panel_pane,
                                    "--frame", fid)
    if (not _PANE_ID_RE.fullmatch(panel_pane) or slot not in frame_slots.SLOTS
            or not _FRAME_ID_RE.fullmatch(fid)
            or not all(_action_word_is_safe(w) for w in words)
            or any(w.split() != [w] for w in words[1:])):
        return None
    action = f"""run-shell -b '"{words[0]}" {" ".join(words[1:])}'"""
    return tmuxctl.server_argv(socket, "set-hook", "-p", "-t", panel_pane, "pane-died",
                               action)


#: Which `resize-pane` flag re-asserts a slot's dimension: `-y` (rows) for the horizontal
#: strips, `-x` (columns) for the side column — the same axis `layout.py`'s own `-v`/`-h`
#: split direction already encodes for the same slots, read here rather than re-derived a
#: third way. The SIZE that goes with it is `layout.slot_sizes`', not `SLOT_SIZE`'s, since
#: `bottom`'s is a function of its content and of the window (#488).
_RESIZE_FLAG = {"top": "-y", "bottom": "-y", "right": "-x"}

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
#:
#: **`[0-9]`, not `\d`, and the difference is the property.** Python's `\d` is Unicode by
#: default: `re.fullmatch(r"%\d+", "%١٢")` is a MATCH, and so is the fullwidth `"%１１"`.
#: Neither is a pane id tmux ever minted, and neither is dangerous on its own — a
#: Unicode digit carries no meaning to any of the three parsers a hook action passes
#: through — but the check is here to say "this is tmux's own word for a pane", and a
#: class that also admits Arabic-Indic digits is answering a different question. The
#: same spelling-instead-of-the-property gap this module keeps paying for, caught before
#: it cost anything rather than after.
_PANE_ID_RE = re.compile(r"%[0-9]+")

#: tmux's own answer, verbatim, for a `set-hook` call naming an event this binary does
#: not recognise at all — confirmed by hand against a real tmux 3.7c with a fabricated
#: hook name (`invalid option: <name>`, generic `set-hook` argument-parsing text, not
#: specific to any one hook's name). Checked against a FAILED install's own stderr
#: rather than trusted only up front: `RESIZE_HOOK_FLOOR` is a fast path to skip the
#: attempt for a version already KNOWN too old, not the only thing standing between an
#: operator and a loud, recurring error if that constant ever turns out to be wrong —
#: this is what makes the mechanism safe BY CONSTRUCTION rather than by the constant
#: being right (see `tmuxctl.report_failure`'s call site below for how the two combine).
_INVALID_HOOK_NAME = "invalid option"


def _resize_hook_argv(*, socket: str, harness_pane: str, fid: str) -> list[str] | None:
    """`window-resized`: ask charter to re-size this frame's panes, whatever the new
    window turns out to be.

    Measured against tmux 3.7c (see `layout.panel_argvs`'s own docstring): tmux's layout
    engine redistributes EVERY pane proportionally whenever the window's size changes,
    `-l size` notwithstanding — a 120x30 frame grown to 200x50 stretched two one-row
    panels to 8 and 7 rows apiece, and only snapped back to 1 row because that particular
    shrink happened to be an exact round trip of the same grow. So something has to
    re-apply the intended sizes after every resize.

    **This used to be the sizes themselves, as literal text, and #488 is why it cannot
    be.** The action was `resize-pane -t %1 -y 1 ; resize-pane -t %2 -x 22` — a constant,
    computed once when the frame was laid out. `bottom`'s height is no longer a constant:
    it is `min(content, cap)` where the cap is what the window can spare
    (`layout.bottom_rows`), so a stale number is not merely imprecise, it is destructive.
    Measured on 3.7c: a window shrunk from 50 rows to 20 with a hook still asserting
    `-y 40` left the harness pane **1 row tall**. tmux does not refuse an over-large
    height — it grants it out of the neighbour. The hook therefore has to RECOMPUTE, and
    the only thing that can recompute is charter.

    **`run-shell -b`, and both halves matter** — the same construction, for the same two
    measured reasons, as `_panel_died_hook_argv`: un-backgrounded, tmux prints a non-zero
    command's own `'…' returned N` INTO THE HARNESS PANE and drops it into copy-mode,
    which is charter drawing in the one rectangle ADR 0018 says it never draws in; and a
    blocking `run-shell` in a hook stalls tmux's own command queue.

    **What a charter per resize event costs, measured rather than assumed.** One
    `charter frame-resize` child: median **20ms** (5 runs, refused before any tmux call, so
    the interpreter and charter's import graph and nothing else). `window-resized` fires
    once per size change, so a drag of thirty of them is ~0.6 CPU-seconds spread across the
    drag, backgrounded, with tmux's own queue never waiting on it — the same order as the
    `pane-died` respawn hook this construction was copied from. Nothing here is free, and
    the old literal-text action was; that is the price of a size that has to be recomputed.

    **Known remaining case: #501.** Nothing serialises those children and `-b` gives no
    completion ordering, so during a drag one that measured a taller window can apply after
    one that measured the final, shorter one — a pane sized for a window that is already
    gone, until the next resize corrects it.

    **It also removes #475 rather than patching it.** The old action interpolated a pane
    id read back off DISK (`state.panes`, via `_relayout`'s `keep` map, which skipped the
    `_PANE_ID_RE` check for every slot it kept) into text tmux re-parses as a command
    line — so a `%1;kill-server` written into the frame's own state directory armed
    `kill-server` on every resize for the life of the window. No pane id reaches this
    text at all now. `cmd_resize` reads them on the other side and checks each one before
    it becomes a `resize-pane -t` **argv element**, where a `;` is a character in a
    filename rather than a command separator.

    Installed as a WINDOW hook (`-w`, scoped via *harness_pane* — tmux resolves its
    containing window from the pane), not a session or global one, so a sibling frame's
    own window is left untouched. Fires on EVERY resize for the life of the window, not
    only the first, since an operator's terminal can be grown and shrunk any number of
    times. A SINGLE `set-hook` call: nothing else in this codebase installs a
    `window-resized` hook, so there is no existing index to collide with — contrast
    `pane-died`, which needed two INDEXED hooks because two independent actions shared
    one event (see `_pane_died_teardown_hook_argv`).

    ``None`` back means charter will not arm the hook, and — exactly as in
    `_panel_died_hook_argv` — every value that reaches the text is what decides, never
    where it came from: *fid* must be a `state.frame_id` (`_FRAME_ID_RE`) and every word
    including the interpreter path must pass :func:`_action_word_is_safe`. A frame whose
    id could not survive all three parses gets no resize recovery, which costs it
    panels that drift out of shape; arming it anyway would cost it whatever the id
    actually said.
    """
    words = util.self_relaunch_argv("frame-resize", "--frame", fid)
    if (not _FRAME_ID_RE.fullmatch(fid)
            or not all(_action_word_is_safe(w) for w in words)
            or any(w.split() != [w] for w in words[1:])):
        return None
    action = f"""run-shell -b '"{words[0]}" {" ".join(words[1:])}'"""
    return tmuxctl.server_argv(socket, "set-hook", "-w", "-t", harness_pane,
                               "window-resized", action)


def _live_sessions(socket: str) -> set[str]:
    """Every session name `tmux -L socket list-sessions` currently reports.

    Empty rather than raised when nothing has ever run on *socket* — `list-sessions`
    exits non-zero with nothing on stdout in that case, which the plain `splitlines()`
    below already turns into an empty set with no special-casing needed. That ordinary
    "no server yet" answer is also why this is the one caller besides
    `_query_pane_dead_status` that asks `tmuxctl.run` NOT to report: a failure here is
    normally not a fault at all, and reporting it would print an error on every launch
    that happens to be the first one on this machine.
    """
    out = tmuxctl.run("listing the frames already running",
                      tmuxctl.server_argv(socket, "list-sessions", "-F",
                                          "#{session_name}"),
                      timeout=5, report=False)
    return {line.strip() for line in out.stdout.splitlines() if line.strip()}


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
    status, code = _pane_state(socket, harness_pane)
    return code if status == _DEAD else None


#: The three answers `_pane_state` distinguishes. `_GONE` is the one that is easy to miss
#: and the one a wait loop cannot do without — see `_pane_state`.
_ALIVE, _DEAD, _GONE = "alive", "dead", "gone"


def _pane_state(socket: str, harness_pane: str) -> tuple[str, int | None]:
    """Is *harness_pane* still running, dead-but-askable, or no longer there at all?

    Three answers rather than two, and the third is measured rather than defensive:
    against tmux 3.7c, `display-message -p -t <a pane that no longer exists>` does NOT
    fail. It returns 0 and expands every format variable to NOTHING — confirmed for a
    pane id that never existed (`%1234`) and for a real one whose window was killed. A
    caller that only tests `#{pane_dead} == "1"` therefore reads a window the operator
    closed as "still running", which is harmless for the eager check
    (`_query_pane_dead_status`, which means "cannot tell" by `None` anyway) and is a
    spin that never ends for `_wait_for_harness`.

    **The EMPTY answer is the `#{pane_dead}` FIELD, not the whole line — and that
    distinction was a live bug for the length of one test run.** :data:`_DEAD_FORMAT`
    carries a literal `:` between its two variables, and tmux prints the format's own
    literal text whether or not anything expands into it: a gone pane answers `":"`, not
    `""`. A guard written against the whole line being empty passed every unit test
    (whose fake returned what the author expected) and was caught only by
    `tests/test_frame_tmux_integration.py` asking a real server. Testing the FIELD
    covers the whole-line case too — `"".partition(":")` yields an empty `dead` just as
    `":"` does — so there is one check here rather than two, the second of which no test
    could ever have failed.

    `remain-on-exit` is what makes `_DEAD` reachable at all rather than every death
    going straight to `_GONE`: it keeps the dead pane in place, still holding its
    `#{pane_dead_status}`, until charter has read it. It is armed pane-scoped before the
    harness is ever started inside an operator's tmux (see `layout.window_argv`), and
    server-globally on charter's own private server (see `_PLACEHOLDER_CONF`).

    A `_DEAD` pane whose `#{pane_dead_status}` is EMPTY is still `_DEAD`, reported as
    `_UNKNOWN_DEATH_CODE`: measured against tmux 3.7c, that is what a harness killed by
    a signal (SIGKILL/SIGTERM/SIGSEGV) looks like, and calling it "cannot tell" is what
    used to send `cmd_launch` on to attach a session provably over.

    A query that FAILS is `_GONE` too. All three of `tmuxctl.run`'s failure shapes reach
    here (a real tmux error, a timeout folded into `TIMED_OUT`, a tmux that could not be
    started at all), and every one of them means the same thing to a caller: there is
    nothing left here to wait for. `report=False` because both callers handle the answer
    themselves.
    """
    dm = tmuxctl.run("asking whether the harness pane died",
                     tmuxctl.server_argv(socket, "display-message", "-p", "-t",
                                         harness_pane, _DEAD_FORMAT),
                     timeout=5, report=False)
    if dm.returncode != 0:
        return _GONE, None
    dead, _, status = dm.stdout.strip().partition(":")
    if not dead:
        return _GONE, None
    if dead != "1":
        return _ALIVE, None
    status = status.strip()
    if status.lstrip("-").isdigit():
        return _DEAD, int(status)
    return _DEAD, _UNKNOWN_DEATH_CODE


def _live_windows(socket: str) -> set[str] | None:
    """Every window name *socket*'s server currently reports, or ``None`` when it did
    not answer at all.

    The inside-a-tmux counterpart of `_live_sessions`, and the difference in return type
    is the point. A frame on charter's own server is a SESSION named by frame id; a
    frame in an operator's tmux is a WINDOW named by frame id, so this is the list
    `state.reap` has to be given there.

    ``None`` rather than an empty set for a non-zero return, because here that failure
    MEANS something: `$TMUX` can outlive the server it names (a value captured by `env`
    and re-exported, a `tmux kill-server` while a script was still running), and charter
    is then not inside a tmux at all no matter what the variable says. `cmd_launch` uses
    that to fall back to its own private server rather than issue a dozen commands one
    by one at a socket nothing is listening on. An empty SET is a different answer — a
    live server with no windows — and must not be confused with it.
    """
    out = tmuxctl.run("listing the frames already in this tmux",
                      tmuxctl.server_argv(socket, "list-windows", "-a", "-F",
                                          "#{window_name}"),
                      timeout=5, report=False)
    if out.returncode != 0:
        return None
    return {line.strip() for line in out.stdout.splitlines() if line.strip()}


def _frame_is_live(socket: str, fid: str) -> bool:
    """Is the frame *fid* still running on *socket*? One question, one place — #408.

    A frame is a SESSION on charter's own private server and a WINDOW in the operator's,
    so "is it still there" is two different queries and `tmuxctl.is_operator_socket` is
    the same single discriminator that already turns a server into `-L` or `-S`.
    `cmd_respawn` asked `_live_sessions(SOCKET)` unconditionally, which on the operator's
    server is a question about a server that is not theirs: it answers "no such session"
    for a frame that is on screen, so a panel that died there could never have been
    brought back even once the hook reached charter.

    ``False`` for a server that did not answer at all (`_live_windows`'s ``None``), and
    that direction is deliberate: the only caller is about to RESPAWN something, and
    respawning into a server charter could not reach is the outcome with a cost. Not
    respawning costs a panel that was already dead.
    """
    if tmuxctl.is_operator_socket(socket):
        live = _live_windows(socket)
        return live is not None and fid in live
    return fid in _live_sessions(socket)


def _wait_for_harness(socket: str, harness_pane: str) -> int | None:
    """Block until the harness in *harness_pane* is over. Its exit code, or ``None``.

    What `attach` does on charter's own private server, done by watching instead: inside
    an operator's tmux the operator is ALREADY attached, to their own server, so there is
    no client for charter to own and no blocking call to read a code out of. `charter
    claude` still has to behave like a foreground command — it is one — so this is where
    it waits.

    ``None`` is the pane vanishing rather than dying askably: the operator closed the
    window (their own `prefix-&`), or the server went away under it. Charter cannot know
    what the harness would have exited with in that case and says so rather than
    inventing a 0; see `cmd_launch`'s own handling.
    """
    while True:
        status, code = _pane_state(socket, harness_pane)
        if status == _DEAD:
            return code
        if status == _GONE:
            return None
        time.sleep(_POLL_SECONDS)


def _launch_sizes(fid: str, slots: list[str], cols: int, rows: int) -> dict[str, int]:
    """How big each of *slots* is split, in a *cols* x *rows* window — the launch-time
    half of `layout.slot_sizes`, shared by both launch paths so they cannot disagree.

    `bottom` is the only slot whose answer is not a constant (#488): it is sized to the
    repo table it is about to draw, floored at the one row it always was and capped so
    the harness keeps `layout.HARNESS_MIN_ROWS`. `frame_slots.bottom_rows_wanted` is the
    same function the RENDERER's own budget comes from, which is what stops a frame
    coming up with a pane taller than its content or a table cut off with nothing saying
    so.

    **`cols` is not decoration (#500).** The renderer draws NO table below
    `statusline._LEFT_W` (95) and at most `slots._TERSE_ROWS` of one at a `terse`
    density, and neither is a rare shape: `layout.visible_slots` keeps `bottom` down to
    `min_cols // 2`, so an 80-column terminal is one, and `minimal` is a level the F2
    menu offers. Sizing from the repo count alone gave both of them a pane sized for a
    table the panel then refused to draw — up to fourteen blank rows taken off the
    harness. `bottom` is split BEFORE `right`, so the window's width IS the pane's.

    Affordable at launch by construction, and that is not incidental: `cmd_launch` calls
    `gather.discard(fid)` before it draws anything, so this reaches `gather.row_count`
    with no cache and gets the directory-listing answer — an `iterdir`, never the git
    sweep `gather.scan` would run. See `gather.row_count`'s own docstring for both paths.
    """
    return layout.slot_sizes(slots, window_rows=rows,
                             content_rows=frame_slots.bottom_rows_wanted(fid, cols=cols))


def _drawable_slots(cols: int, rows: int, configured: list[str] | None = None) -> list[str]:
    """Which configured slots this frame will actually draw, at *cols* x *rows*.

    *configured* defaults to `config.FRAME["slots"]` — already the density-resolved list
    (`instance.frame_of` expands a declared `[frame] density` into it, so nothing here
    knows presets exist). `cmd_density` passes the list a level expands to instead, so a
    frame re-laid-out by the hotkey menu goes through exactly the same two filters a launch
    does rather than a second copy of them: below the size floors it drops the same slots
    in the same order, and a slot with no renderer is skipped for the same reason.

    `[frame] slots` can accept a slot (`instance.FRAME_SLOTS`, sized by
    `layout.SLOT_SIZE`) that `frame.slots.SLOTS` — the RENDERER registry — has no
    renderer for (as `left`/`right` were until Task 3 (#385) gave them one, and as the
    next slot this frame grows will be on its first day). Left
    unfiltered, `panel_argvs` would still split a real pane for it; `panel.run`
    correctly refuses or exits 2 (Task 7's own "no empty pane" rule), but with
    `remain-on-exit on` keeping that pane alive, the operator is left with a permanently
    dead, wrapped-error 22-column pane and no explanation at the point the frame
    actually came up. Skipping an unimplemented slot here instead means the harness pane
    simply keeps that space — the same degrade `visible_slots` itself already makes
    under a tight terminal.

    Silently, and that is the deliberate half: which slots have a renderer is a property
    of THIS BUILD, not of this launch, so a warning here repeats an unchanging fact on
    every single start — and repeats it into a terminal that is about to switch to
    tmux's alternate screen, where nobody reads it (measured; see `frame_ready`'s own
    docstring). `--probe`, `charter frame-probe` and `charter doctor` all name it on
    demand instead, from the same `frame_slots.unimplemented` this filters on.
    """
    frame = config.FRAME
    slots = layout.visible_slots(frame["slots"] if configured is None else configured,
                                 cols, rows, frame["min_cols"], frame["min_rows"])
    unimplemented = frame_slots.unimplemented(slots)
    return [s for s in slots if s not in unimplemented]


def _frame_env(fid: str, h) -> dict[str, str]:
    """The environment the harness runs in, whichever server it runs on.

    Shared by both paths deliberately, because they deliver it by opposite mechanisms
    and only the CONTENT is the same: on charter's own server it is handed to
    `new-session` as the tmux CLIENT's environment, which the server inherits whole if
    this is the call that starts it; inside an operator's tmux the server is already
    running and is not charter's, so nothing of this reaches a pane except by being
    NAMED (`_guest_harness_env`, `_pane_identity_env`). A second copy of "what a framed
    harness's environment is" would be two answers to one question.

    COLUMNS/LINES go, and that is belt and braces rather than the fix itself: every pane
    (harness or panel) measures its OWN tty (`frame/slots.py`, `frame/panel.py`), so a
    stale value here cannot mislay anything charter draws. But this environment is
    inherited WHOLE by every process tmux starts for this frame — the harness's shell
    among them — and both variables describe the LAUNCHING terminal, not any pane the
    frame creates.

    TMUX/TMUX_PANE go for a sharper reason: they describe the pane `charter` was TYPED
    in. tmux sets both itself for a pane it creates, and carrying the launcher's own
    values across would tell the harness — and `session.terminal()`, which reads
    `TMUX_PANE` — that it is running in a pane it is not. That is the identity collision
    `WINDOWID` was removed for
    (`docs/superpowers/specs/2026-08-21-harness-wrapper-design.md`), reached through a
    different variable.

    **Both removals are now only about charter's own server**, and that is worth saying
    rather than leaving the reader to work out: since #446 nothing on the operator's
    server is handed a variable that was not named, so a stale `COLUMNS` or a borrowed
    `TMUX_PANE` cannot reach a pane there whether or not it is popped here. They stay
    popped because the private-server path still hands this dict to `new-session` as a
    client environment, and the server born from it inherits every name in it.
    """
    env = dict(os.environ, CHARTER_SESSION_ID=fid)
    for stale in ("COLUMNS", "LINES", "TMUX", "TMUX_PANE"):
        env.pop(stale, None)
    if h:
        env["CHARTER_HARNESS"] = h.name
    return env


#: The variables whose value must be THIS frame's rather than whichever launcher
#: happened to start the shared tmux server — the whole of what `_frame_identity_env`
#: puts on a tmux command line.
#:
#: Named one at a time rather than globbed on a `CHARTER_` prefix, because the list is a
#: PROMISE about what reaches `/proc/<pid>/cmdline` and a prefix match would quietly keep
#: that promise for a variable nobody has invented yet. Each is here for a reason a second
#: frame makes real:
#:
#: * ``CHARTER_SESSION_ID`` — the frame's own id, and #411 itself.
#: * ``CHARTER_HARNESS`` — two frames may run two different harnesses.
#: * ``CHARTER_ROOT`` — two frames may be two different PLANES, and a harness reading the
#:   other one's root writes into a control plane nobody in this frame chose.
#: * ``CHARTER_WORKSPACE`` — `workspace.resolve` puts this above every pointer, so an
#:   inherited pin outranks the frame's own answer and `charter ws use` cannot move it.
#: * ``CHARTER_PERSONA`` — the same rung one module over: `persona._resolved` ranks it
#:   above the per-session pointer, the per-terminal pointer AND the active-persona file.
#:   An inherited pin therefore reaches the harness *and the frame's own `right` panel*,
#:   which draws persona chips, and `charter persona use` cannot move either. It is here
#:   because it meets the criterion above, not because anything was observed breaking —
#:   the criterion is what this list is for, and applying it unevenly is how the next
#:   variable gets forgotten.
#:
#: None of the five is ever a credential. That is the property that makes putting them on
#: an argv acceptable at all — see :func:`_frame_identity_env`.
_FRAME_IDENTITY = ("CHARTER_SESSION_ID", "CHARTER_HARNESS", "CHARTER_ROOT",
                   "CHARTER_WORKSPACE", "CHARTER_PERSONA")


def _frame_identity_env(env: dict[str, str]) -> dict[str, str]:
    """Just the part of *env* that has to travel on a tmux COMMAND LINE, and no more.

    **A tmux `-e` is argv, and argv is not private.** `_frame_env` is
    ``dict(os.environ, …)`` — the operator's whole environment, `SSH_AUTH_SOCK`, API
    tokens, vault service-account tokens and all. Handed to `subprocess.run(env=…)` that
    is an ordinary child environment, readable only by the owner via `/proc/<pid>/environ`.
    Expanded into ``-e NAME=VALUE`` argv elements it becomes the tmux client's COMMAND
    LINE: world-readable in `/proc/<pid>/cmdline` on Linux for as long as the client runs,
    visible to `ps` for every local user, and recorded permanently by exec-audit and
    process-accounting tooling. Measured on a real environment: 138 argv elements, 7,696
    bytes, carrying two live service-account tokens. `charter claude` is the default path,
    so that would have been every launch on every machine.

    So only :data:`_FRAME_IDENTITY` travels, and the rest keeps arriving exactly the way
    it always did — through the tmux client's own environment on charter's server, and
    through the operator's already-running server on theirs.

    **Every name is emitted, including the ones that are absent, as ``NAME=``.** An
    inherited value is as wrong as a stale one: a launcher that pinned `$CHARTER_WORKSPACE`
    starts the shared server with it, and the NEXT frame — which pinned nothing — would
    otherwise inherit that pin and have `workspace.resolve` rank it above its own
    pointers. Measured against tmux 3.7c: ``-e FOO=`` leaves the variable set and empty
    rather than unset, which is what charter's own readers already treat as absent
    (`workspace.resolve` and `root.find_root` both test the value for truth, not for
    presence). Being explicit in both directions is what makes a frame's charter identity
    the launcher's answer rather than half of somebody else's.

    **Shadowing is all there is: `-e` is purely ADDITIVE and cannot REMOVE a name.** The
    obvious-looking alternative is a bare `-e NAME` with no `=`, and it is not an unset —
    measured on the same tmux, and the measurement is worse than a refusal would have
    been: tmux ACCEPTS it, returns 0, prints nothing, and leaves the inherited value
    exactly where it was. There is no `-u`/`-r` counterpart on any of the three commands
    charter puts a `-e` on either. So a variable already in the server's or session's
    environment can only be OVERWRITTEN with an explicit empty value, which is what the
    line below emits for every absent name; a future reader reaching for the bare form
    would be choosing inheritance, silently, and would get no error to tell them. Pinned
    in `tests/test_frame_tmux_integration.py::ASecondFrameOnTheSharedServer::
    test_a_bare_e_name_cannot_take_a_variable_away`, which will say so if tmux ever
    grows the unset this wants.
    """
    return {name: env.get(name, "") for name in _FRAME_IDENTITY}


def _guest_harness_env(env: dict[str, str]) -> dict[str, str]:
    """What the HARNESS pane must be told on a server charter does not own — #446.

    :func:`_frame_identity_env` plus `$PATH`, and the whole difference between the two
    servers is in that one extra name. On charter's own private server the base the `-e`
    overlays is a server SOME charter launcher started, so its `$PATH` is a charter
    launcher's. On the operator's it is a server THEY started, possibly weeks ago, in
    another shell — and `cmd_launch` has already resolved the harness binary
    (`shutil.which(h.binary)`) against charter's OWN `$PATH`. Handing the pane a
    different one would make that check a promise charter cannot keep: the frame comes
    up, the exec fails, and the operator gets `_UNKNOWN_DEATH_CODE` for a binary charter
    said it had found.

    **This used to be `dict(os.environ, …)` whole, and that is the defect.** Measured on
    one real environment: 129 argv elements, 7,773 bytes, four live 1Password
    service-account tokens and an npm auth token — in `/proc/<pid>/cmdline`, world-readable
    to every local user on Linux and recorded permanently by exec-audit tooling. See
    :func:`_frame_identity_env` for the argument in full; it is the same argument, and
    #412 closed only the half of it that was on charter's own server.

    **Measured against tmux 3.7c, and the measurement is why `PATH` is belt and braces
    rather than the fix.** A pane's `$PATH` comes from the tmux CLIENT that issued the
    command, not from the server: a server started with `PATH=/server/bin`, a
    `respawn-pane` issued by a client with `PATH=/respawnclient/bin`, produced a pane
    holding `/respawnclient/bin` — and an explicit `-e PATH=/explicit/bin` on that same
    command did NOT survive, tmux overwrites it after applying the `-e` set (read out of
    the pane by `python3`, not by a shell, to rule out any shell's own normalisation).
    So on this tmux the pane already has charter's own `$PATH` and stating it changes
    nothing. It is carried anyway because no measurement says an older tmux applies the
    same rule, and the cost of being wrong in the two directions is not symmetric: one
    redundant argv pair carrying a value that is never a credential, against a harness
    that cannot be executed at all.

    Empty is not carried — `-e PATH=` would state an EMPTY `$PATH`, which is strictly
    worse than inheriting one. A charter running without `$PATH` has nothing to say here.
    """
    values = _frame_identity_env(env)
    if env.get("PATH"):
        values["PATH"] = env["PATH"]
    return values


def _remain_on_exit_argv(*, socket: str, harness_pane: str) -> list[str]:
    """`set-option -p`: keep THIS pane in place after its program exits, and no other.

    PANE-scoped, and armed at the one moment nothing else can cover: between the window
    being created and the harness being started into it. `_panel_remain_on_exit_argv`
    arms the whole of charter's own window later, when the panels are drawn — but that is
    after `layout.respawn_argv` has already put the harness in, and a harness that dies
    in the gap is exactly the early death #384 is about. `-p` here holds the harness pane,
    and its `#{pane_dead_status}`, from before its program exists.

    `-g` is the scope that must never be used on somebody else's server: it would leave
    every pane the OPERATOR closes hanging around as a dead pane, in every window they
    have open. Charter's own private server arms it globally on purpose
    (`_PLACEHOLDER_CONF`), because there is nothing on that server that is not charter's.
    """
    return tmuxctl.server_argv(socket, "set-option", "-p", "-t", harness_pane,
                               "remain-on-exit", "on")


def _panel_remain_on_exit_argv(*, socket: str, harness_pane: str) -> list[str]:
    """`set-option -w`: keep dead panes in CHARTER'S OWN WINDOW, and in no other.

    **Without this a panel's respawn hook cannot fire at all, and #408 was only half
    fixed.** tmux runs `pane-died` only for a pane that DIED AND STAYED — with
    `remain-on-exit` off, the pane is destroyed, its pane-scoped hook is destroyed with
    it, and nothing runs. Measured on tmux 3.7c against a server left at tmux's own
    default (which is what an operator's tmux is): a panel pane armed exactly as
    `_arm_panel_respawn` arms it, dying exactly as a panel dies, reached no shell and
    left no pane — and the same run with this one option set reached the shell and kept
    the pane. Charter's private server never showed it because `_PLACEHOLDER_CONF` sets
    `remain-on-exit` server-globally there, so every pane on it already stays.

    **WINDOW-scoped, not per panel pane, and the reason is a race rather than a
    preference.** A pane option can only be set on a pane that already exists, so a
    `set-option -p` after each `split-window` leaves every panel unprotected for the
    milliseconds between tmux starting its program and charter arming it — and a panel
    that dies in that gap (a bad interpreter, an import error) is precisely the one #382
    is about. The window is armed BEFORE the first split, so a pane created into it is
    born covered. It also covers the panes a later `_relayout` adds without arming
    anything twice.

    The scope is charter's own window and reaches nothing of the operator's — measured,
    not reasoned: `-w -t <a pane id>` resolves to that pane's window, and after this runs
    charter's window reads `on` while the operator's own window still reads the global
    default and their own panes still vanish when their programs exit. `kill-pane` also
    still destroys a pane this is holding, which is what lets `cmd_density` drop a slot
    (see `_disarm_panel_respawn`) rather than leave a corpse behind in the frame.
    """
    return tmuxctl.server_argv(socket, "set-option", "-w", "-t", harness_pane,
                               "remain-on-exit", "on")


def _launch_in_operator_tmux(socket: str, session: str, *, fid: str, argv: list[str],
                             h, v: tuple[int, int]) -> int | None:
    """Build the frame as a WINDOW in the tmux the operator is already in.

    The same layout as the private-server path — harness in the middle, charter's panels
    on the edges — with no second tmux underneath it and no second prefix key on top of
    it. ADR 0018 and the design spec both settle this; what shipped before it was a
    private server started INSIDE the operator's own pane, which works and stacks two
    prefix layers on one terminal.

    ``None`` means "not actually inside a tmux after all" — `$TMUX` named a server that
    did not answer — and `cmd_launch` falls back to its own private server. Any `int` is
    this launch's own exit code.

    **Nothing of the operator's is written.** No `source-file`, no `set -g`, no
    session-scoped `set`, and no key binding. Each of those reaches past charter's own
    window: `status`/`mouse`/`history-limit` are session options, so charter's values
    for them would become theirs in every window they have open; `escape-time` and
    `remain-on-exit` are server options; `set-environment` hands every new shell they
    open a frame id that is not theirs; and a key table is server-wide with no
    per-window form at all. The costs are real and named in `docs/frame.md`: the frame
    inside a tmux keeps THEIR scrollback limit and THEIR mouse setting, and charter
    binds no hotkey there at all.

    The spec allows a PREFIX-scoped binding here rather than none. Charter takes the
    stricter option, for a reason the spec could not have known: the bind's action has
    to resolve which frame it belongs to at the moment the key fires, and the only
    mechanism tmux offers for that is the session-scoped `set-environment` this path
    must not use (see `_session_id_env_argv`). The menu's one entry today is "Detach",
    which an operator already inside tmux has their own prefix key for. A key taken from
    every window on their server to reach a redundant menu is a worse trade than no key
    — `frame/slots.py` drops the hotkey hint from the bottom panel to match.

    Three things that ARE written are charter's own and reach nothing of theirs, because
    every one of them is scoped to a pane charter created or to the window charter
    created: `remain-on-exit` on the harness pane (`_remain_on_exit_argv`, PANE-scoped);
    `remain-on-exit` on the frame's own window, so a dead PANEL stays long enough for its
    hook to run (`_panel_remain_on_exit_argv`, WINDOW-scoped — measured to leave the
    operator's own windows at their own default); and each panel pane's own `pane-died`
    respawn hook (`_arm_panel_respawn`, PANE-scoped, #408 — it refused here until the
    hook could name this server rather than charter's, and then never fired here until
    the window kept the corpse it has to fire from).

    **The harness's exit code travels without hooks here, and that is a real
    difference.** The private-server path needs `pane-died[0]`/`pane-died[1]` because it
    blocks in `attach` and has nothing else watching; this launcher is awake for the whole
    life of the frame (`_wait_for_harness`), so it reads the status and closes the window
    itself. Installing the teardown hook as well would actively LOSE the exit code: a
    hook array runs in index order and `kill-window` would destroy the pane before this
    process's next ask. The cost is honest and bounded — if this launcher is itself
    killed while the harness is running, the harness keeps running and its window is
    left in the operator's own window list, where their own `prefix-&` closes it. That
    is strictly better than the failure it replaces on the other path (a session nothing
    can end, with `attach` blocked on it forever), because the operator is already
    attached and in control of the server it happened on.
    """
    # Doubles as the check that `$TMUX` still names a live server: `list-windows` is
    # both the liveness list `state.reap` needs here (a frame is a WINDOW on this
    # server, named by frame id) and the cheapest possible "is anybody home".
    live_before = _live_windows(socket)
    if live_before is None:
        return None
    state.reap(live_before, server=socket)

    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1
    # The same recycled-pid adoption #383 fixed on the private-server path, and nothing
    # about it is private-server-specific: `fid` is `<workspace>-<launcher pid>` on
    # either server, `reap` keeps a directory for as long as that pid is live, and on a
    # launch it is live because it is ours — so an earlier launcher for this workspace
    # that landed on the same pid leaves its whole directory here to be adopted.
    #
    # `gather.json` is the one that costs something on this path: `gather.read` has no
    # freshness check by design (a panel's hot path) and a panel repaints only on a
    # version bump, so a dead frame's repos and CI would sit beside a live harness until
    # the session's first hook fires. `exit` is not read back as this launch's result
    # here — the code comes off the pane, not the file — but it IS charter's record of
    # how this frame ended, and a launcher killed mid-run would leave a dead frame's
    # number standing as this one's. Cleared for the same reason `bump` moves the
    # version: whatever is under the id predates the frame now claiming it.
    state.clear_exit(fid)
    gather.discard(fid)
    state.clear_shape(fid)
    # Rewritten, not merely written: an adopted directory may name the OTHER server, and
    # a stale marker would make `reap` there skip a frame that is now genuinely ours.
    state.record_server(fid, socket)
    state.bump(fid)

    env = _frame_env(fid, h)
    # The launcher is the only process that knows this frame's own charter identity —
    # see `state.record_identity` for the measurement. Written before any pane exists,
    # because a `run-shell` child fired later reads the SERVER's environment, not this
    # one's.
    state.record_identity(fid, _frame_identity_env(env))
    cwd = os.getcwd()
    opened = tmuxctl.run(
        "opening a window for the frame",
        layout.window_argv(socket=socket, session=session, window=fid, cwd=cwd))
    if opened.returncode != 0:
        return 1
    window_id, _, harness_pane = opened.stdout.strip().partition(" ")
    if not window_id or not _PANE_ID_RE.fullmatch(harness_pane.strip()):
        util.err("charter frame: tmux opened the window but did not report a window "
                 "and pane id — cannot scope the frame to it")
        return 1
    harness_pane = harness_pane.strip()
    # What tells a process inside this frame apart from one that merely inherited its id
    # (ADR 0019, `state.record_harness_pane`). Recorded before the harness is started, so
    # the very first status line it spawns can already answer.
    state.record_harness_pane(fid, harness_pane)

    def _close_window() -> None:
        tmuxctl.run("closing the frame's window",
                    tmuxctl.server_argv(socket, "kill-window", "-t", window_id))

    # Before the harness exists, not after: this is what keeps the pane (and its
    # `#{pane_dead_status}`) in place once the harness exits, and there is no way to
    # set it on a pane that does not exist yet. See `layout.window_argv`.
    armed = tmuxctl.run("keeping the harness pane askable",
                        _remain_on_exit_argv(socket=socket, harness_pane=harness_pane))
    if armed.returncode != 0:
        util.warn("charter frame: continuing without it — this frame's window may "
                  "close before charter can read the harness's exit code")

    started = tmuxctl.run(
        "starting the harness in it",
        layout.respawn_argv(socket=socket, harness_pane=harness_pane,
                            env=_guest_harness_env(env), cwd=cwd, harness_argv=argv))
    if started.returncode != 0:
        # The placeholder is still running in a window the operator never asked for and
        # would never learn the purpose of. Take it back.
        _close_window()
        return 1

    # The same eager ask the private-server path makes right after installing its
    # hooks, for the same reason: a harness that is already over leaves nothing worth
    # building a frame around, and switching the operator's client to it would park
    # them on a dead pane.
    status, code = _pane_state(socket, harness_pane)
    if status != _ALIVE:
        if code is not None:
            state.record_exit(fid, code)
        if code is not None and code != 0:
            # #384 reaches THIS path too, and reaches it harder. Nothing below runs — no
            # panels, no `select-window` — so the operator is never switched to the
            # frame's window at all: it is created, filled with a corpse, and killed
            # again, all before their screen changes. On charter's own private server
            # the silence is at least explained by there being no attach; here the
            # operator is watching a tmux the whole time and still sees nothing.
            #
            # Read the pane BEFORE `_close_window`, for the same reason `cmd_launch`
            # reads it before `kill-session`: afterwards there is nothing left to read.
            # Nonzero only, and by the same argument — `charter frame -- true` lands
            # here with 0, and what it wrote was its own stdout.
            util.err(early_death_message(argv, code,
                                         _pane_last_words(socket, harness_pane)))
        _close_window()
        _reap_this_server(socket)
        return code if code is not None else _UNKNOWN_DEATH_CODE

    cols, rows = _window_size(socket, harness_pane)
    slots = _drawable_slots(cols, rows)
    # *pane_env* is what each panel's own process gets, which on somebody else's server
    # has to be carried explicitly — identity and nothing else, the same five names the
    # private-server path carries, for `_pane_identity_env`'s own reason. It used to be
    # the whole environment here (#446): see `_guest_harness_env`.
    panes = _draw_panels(socket, slots=slots, fid=fid, harness_pane=harness_pane,
                         env=None, v=v, pane_env=_pane_identity_env(env, v),
                         sizes=_launch_sizes(fid, slots, cols, rows))
    _arm_panel_respawn(socket, fid=fid, panes=panes, env=None)
    tmuxctl.run("focusing the harness pane",
                tmuxctl.server_argv(socket, "select-pane", "-t", harness_pane))
    # `select-window`, never `attach`: the operator has a client already, on this very
    # server. A second attach IS the nesting this path exists to remove.
    tmuxctl.run("switching to the frame",
                tmuxctl.server_argv(socket, "select-window", "-t", window_id))

    code = _wait_for_harness(socket, harness_pane)
    if code is None:
        # The pane vanished rather than dying askably — the operator closed the window
        # (their own `prefix-&`), or the server went with it. Nonzero and named: charter
        # cannot know what the harness would have exited with, and reporting a killed
        # agent as a clean 0 is the fabricated success this module refuses everywhere
        # else. Nothing is killed here either; there is nothing left to kill.
        util.err("charter frame: the frame's window is gone — the harness's exit code "
                 "is not something charter can know now.")
        code = _UNKNOWN_DEATH_CODE
    else:
        state.record_exit(fid, code)
        _close_window()
    _reap_this_server(socket)
    return code


def _reap_this_server(socket: str) -> None:
    """Clear out state for frames of *socket* that are gone — and only if it answered.

    An empty window list and NO window list are opposite facts, and `_live_windows`
    keeps them apart for exactly this reason: an empty set means the server is up with
    nothing on it, so every frame directory recorded against it is stale; `None` means
    the server did not answer at all (a wedged one, a timeout), and reaping on that
    would delete a sibling frame's version file and recorded exit code over a question
    charter could not get an answer to. Not reaping costs a directory until the next
    launch; reaping on no information costs a running frame its state.
    """
    live = _live_windows(socket)
    if live is not None:
        state.reap(live, server=socket)


def _window_size(socket: str, harness_pane: str) -> tuple[int, int]:
    """The size of the WINDOW *harness_pane* is in, or `_FALLBACK_SIZE`.

    Never `os.get_terminal_size()` on this path: that measures the pane `charter` was
    typed in, which is a fraction of the window the frame gets the moment the operator
    has a split open — and `_drawable_slots` would then drop panels a full-width window
    has ample room for.
    """
    out = tmuxctl.run("measuring the frame's window",
                      tmuxctl.server_argv(socket, "display-message", "-p", "-t",
                                          harness_pane, _WINDOW_SIZE_FORMAT),
                      timeout=5, report=False)
    w, _, hgt = out.stdout.strip().partition(":")
    if out.returncode != 0 or not w.isdigit() or not hgt.isdigit():
        return _FALLBACK_SIZE
    return int(w), int(hgt)


def _draw_panels(socket: str, *, slots: list[str], fid: str, harness_pane: str,
                 env: dict | None, v: tuple[int, int],
                 pane_env: dict[str, str] | None = None,
                 sizes: dict[str, int] | None = None) -> dict[str, str]:
    """Split one pane per slot off *harness_pane*, then install the resize hook.

    Shared by both paths: a panel is a panel wherever the frame is, the splits target
    the harness pane's `%N` id for the reason `layout.py`'s docstring measures (tmux
    renumbers pane INDICES on every split), and the `window-resized` hook is scoped
    through that same pane to its containing window — so even on the operator's server
    it reaches charter's own window and nothing else of theirs.

    `util.self_relaunch_argv()` (#390), and sharing it is the point: a panel is spawned
    with the PANE's cwd, so `-m` alone prepends whatever directory the operator typed
    `charter` in to the child's `sys.path` — and from a charter checkout that imports
    the checkout instead of the install, which is how both panels once came up reading
    `Pane is dead (status 2)`. Nothing about that is specific to charter's own server;
    inside an operator's tmux the cwd is the same cwd, so the hole is the same hole.
    """
    panes = _split_panels(socket, slots=slots, fid=fid, harness_pane=harness_pane,
                          env=env, pane_env=pane_env, sizes=sizes)
    _install_resize_hook(socket, harness_pane=harness_pane, panes=panes, v=v, env=env,
                         fid=fid)
    # Written down, because a frame's shape can now be CHANGED while it runs (the density
    # menu — `cmd_density`), and nothing else afterwards can say which tmux pane charter
    # meant as which slot. Slots only: `state.record_harness_pane` already owns the
    # harness pane, and one fact recorded twice is one fact free to disagree with itself.
    state.record_panes(fid, panels=panes)
    return panes


def _split_panels(socket: str, *, slots: list[str], fid: str, harness_pane: str,
                  env: dict | None, pane_env: dict[str, str] | None,
                  sizes: dict[str, int] | None = None) -> dict[str, str]:
    """One `split-window` per slot, and the `{slot: pane id}` map that came back.

    The splitting half of :func:`_draw_panels`, separated from the hook-and-record half so
    a live re-layout (`_relayout`) can add panes to a frame that already has some without
    re-installing hooks per batch or overwriting the map it is in the middle of building.

    **The one funnel every panel pane charter creates comes out of, which is why
    `remain-on-exit` is armed here** — before the first `split-window`, so no panel is
    ever born into a window that would throw its corpse away and its respawn hook with it
    (`_panel_remain_on_exit_argv`, #408). Both launch paths and every density change
    reach panels through this function; arming at the call sites instead is what left the
    operator's server covered on one of two.

    Reported but not fatal, like the splits themselves: a frame whose panels cannot be
    respawned is still a frame, and the harness pane's own `remain-on-exit` was armed
    separately and earlier (`_remain_on_exit_argv`), so the exit code does not ride on
    this.
    """
    tmuxctl.run("keeping the frame's own dead panes long enough to bring them back",
                _panel_remain_on_exit_argv(socket=socket, harness_pane=harness_pane),
                env=env)
    panel_cmds = layout.panel_argvs(slots=slots, session=fid, socket=socket,
                                    harness_pane=harness_pane, env=pane_env,
                                    sizes=sizes)
    # Zipped with `slots`, not just iterated: `_resize_hook_argv` needs to know WHICH slot
    # each successfully-created pane belongs to (for its size and its resize-pane flag),
    # and `panel_argvs` returns exactly one command per slot, in the same order (see its
    # own docstring).
    panes: dict[str, str] = {}
    for slot, cmd in zip(slots, panel_cmds):
        # Reported by `tmuxctl.run` but not fatal: one decorative panel failing to draw
        # must not take down a harness pane that is already up and running (correction 2
        # asks for every return code to be CHECKED, not every failure refused).
        p = tmuxctl.run("drawing a panel", cmd, env=env)
        if p.returncode != 0:
            continue
        pane_id = p.stdout.strip()
        if pane_id and _PANE_ID_RE.fullmatch(pane_id):
            panes[slot] = pane_id
    return panes


def _install_resize_hook(socket: str, *, harness_pane: str, panes: dict[str, str],
                         v: tuple[int, int], env: dict | None, fid: str,
                         replacing: bool = False) -> None:
    """Arm (or re-arm) the `window-resized` hook, or remove it when nothing is left to
    resize.

    Split out of :func:`_draw_panels` because a live re-layout (`cmd_density`) changes
    which panes exist without going through a whole launch. *panes* no longer reaches the
    hook's TEXT — since #488 the action names only the frame, and `cmd_resize` reads the
    pane map off disk when it fires — so a stale entry can no longer be armed at all.
    What *panes* still decides here is whether there is anything to arm: an empty map
    means every panel is gone, and one `set-hook` replaces a hook only when there is
    something to replace it with. *replacing* tells the two empty cases apart — a launch
    with no panels has nothing armed to remove and issues no command at all, while a
    re-layout that dropped every slot must actively `set-hook -u`. It is reachable rather
    than theoretical: `_drawable_slots` answers `[]` below half the size floors.

    **Nothing is printed here any more, and that is #387's half of it.** This function
    used to `util.warn` when the tmux was below `RESIZE_HOOK_FLOOR` — a STANDING fact about
    this machine, printed into the pre-attach window measured at 86 bytes before tmux's own
    `\\x1b[?1049h`, where the operator's terminal switches to the alternate screen
    milliseconds later and the line comes back only once the frame exits. It is reported by
    `frame_ready` (`--probe`, `charter frame-probe`) and `doctor.check_frame` instead, which
    are the two surfaces built to report ceilings on demand.

    The two warnings below stay, and the difference is real: those are not standing facts
    but DISCOVERIES made at this launch — a tmux that turned out not to know the hook name
    despite the version gate, or a `set-hook` that failed for some other reason entirely.
    Neither is answerable by `frame_ready`, which starts nothing.
    """
    if v < tmuxctl.RESIZE_HOOK_FLOOR:
        # Below RESIZE_HOOK_FLOOR, `window-resized` is not a hook name THIS tmux
        # recognises at all — `set-hook` fails with `invalid option: <name>` for any
        # name it does not know (see RESIZE_HOOK_FLOOR's own docstring for exactly what
        # was, and was not, confirmed by hand) — skip the attempt rather than printing
        # that confusing text on every single launch. Nothing was ever installed below
        # this line, so there is nothing to remove either.
        return
    if not panes:
        if not replacing:
            # A LAUNCH with no panels: the window is new, nothing has ever been armed on
            # it, and a `set-hook -u` here would be a tmux command issued to remove a hook
            # that does not exist — measurable in the launcher's own command list and
            # asserted against by `Launch.test_no_resize_hook_is_installed_when_no_panel
            # _pane_id_was_learned`. Nothing to do.
            return
        # A RE-LAYOUT that dropped every slot, which is reachable rather than theoretical:
        # `_drawable_slots` answers `[]` below half of `min_cols`/`min_rows`, so shrinking
        # a small window's frame to `minimal` kills all four panes. One `set-hook`
        # REPLACES a hook only when there is something to replace it with — an empty map
        # replaces nothing — so returning here would leave the launch's own hook armed and
        # firing `resize-pane -t %1` at dead panes on every resize for the window's life.
        tmuxctl.run("removing the resize hook",
                    tmuxctl.server_argv(socket, "set-hook", "-w", "-u", "-t",
                                        harness_pane, "window-resized"),
                    env=env, report=False)
        return
    resize_cmd = _resize_hook_argv(socket=socket, harness_pane=harness_pane, fid=fid)
    if resize_cmd is None:
        # A frame id that could not survive the three parses a hook action passes
        # through (`_resize_hook_argv`). Warned rather than silent, and for the same
        # reason a failed install below is: the consequence is visible — panels drift out
        # of shape on every resize — and an operator told why beats a frame that quietly
        # stopped correcting itself. Nothing was armed, so there is nothing to remove.
        util.warn("charter frame: this frame's id cannot be named safely in a tmux "
                  "hook — panels may drift out of shape if this terminal is resized")
        return
    # `report=False`: this is the one call site that reads a failure's own stderr
    # before deciding whether it IS one — an unrecognised hook name here is a
    # capability ceiling to note quietly, not an integration to report loudly, and
    # `tmuxctl.run`'s default would have printed the loud version first regardless
    # of which branch runs below.
    resize = tmuxctl.run("installing the resize hook", resize_cmd, env=env, report=False)
    if resize.returncode == 0:
        return
    if _INVALID_HOOK_NAME in (resize.stderr or ""):
        # RESIZE_HOOK_FLOOR believed this tmux would recognise the hook name and
        # it does not — the constant is wrong, not the launch. Trust what THIS
        # tmux just said over the constant and degrade the same quiet way the
        # version-gate above already does, rather than report it as a broken
        # integration (a capability ceiling, not a failure — see the module's own
        # "belt and braces" framing and `harness.base.Deficit`'s same philosophy
        # for a harness-level capability gap).
        util.warn("charter frame: this tmux does not support the "
                  "resize-recovery hook — panels may drift out of shape if this "
                  "terminal is resized")
    else:
        tmuxctl.report_failure("installing the resize hook", resize_cmd, resize)
        util.warn("charter frame: continuing without it — panels may drift out "
                  "of shape if this terminal is resized")


def _arm_panel_respawn(socket: str, *, fid: str, panes: dict[str, str],
                       env: dict | None) -> None:
    """Give each pane in *panes* its OWN `pane-died` hook, so a dead panel comes back.

    A panel whose process dies outright would otherwise leave a hole for the frame's whole
    life (#382). Scoped to each panel's own pane — never the harness pane, whose
    `pane-died` array carries the exit code and must not gain a third writer (see
    `_panel_died_hook_argv`). Reported but never fatal, like every other decorative tmux
    command here: a panel that cannot be armed for respawn is still a panel that came up.

    **This used to refuse outright on the operator's server, and #408 is that refusal.**
    `cmd_respawn` and `_panel_died_hook_argv` both spelled `-L SOCKET` by hand, so arming
    a panel inside the operator's tmux would have installed a hook aimed at a different
    server; backing out was right at the time and left a panel that dies there dead for
    the life of the frame, with no message, no respawn and no backoff. Both ends now build
    their argv through `tmuxctl.server_argv` and resolve liveness through `_frame_is_live`,
    so the same hook is correct on either server and there is nothing left here to refuse.

    **An armed hook and a hook that can FIRE are two claims, and the first round of #408
    only bought the first.** tmux runs `pane-died` for a pane that died and STAYED; a
    pane on a server left at `remain-on-exit off` — an operator's own tmux — is destroyed
    at death, taking its pane-scoped hook with it, and every argv here can be perfect
    while nothing ever runs. What makes this reachable is `_panel_remain_on_exit_argv`,
    armed on charter's own window before the first panel is split; the measurement is in
    that function's docstring and the end-to-end test is
    `WindowInsideAnOperatorsTmux.test_a_panels_respawn_hook_is_armed_against_this_server_and_fires`,
    which runs against a server nothing has pre-armed.

    A pane charter will not arm (`_panel_died_hook_argv` returning ``None`` — see its own
    docstring for what fails that check) is NAMED rather than skipped in silence: the
    operator loses respawn for that panel and this is the only place that could say so.
    """
    for slot, pane_id in panes.items():
        cmd = _panel_died_hook_argv(socket=socket, panel_pane=pane_id, slot=slot, fid=fid)
        if cmd is None:
            util.warn(f"charter frame: the {slot} panel will not be brought back if it "
                      "dies — charter cannot build a respawn hook it can be sure tmux "
                      "and the shell will read the way it means it")
            continue
        tmuxctl.run(f"arming the {slot} panel for respawn", cmd, env=env)


def _disarm_panel_respawn(socket: str, *, pane_id: str) -> None:
    """Take one panel pane's `pane-died` hook off, BEFORE that pane is killed.

    Without this, changing density is self-undoing: `kill-pane` on a panel charter no
    longer wants fires that pane's own respawn hook, `cmd_respawn` sleeps its backoff,
    finds the session still perfectly alive (only the layout changed), and respawns the
    panel the operator just asked to be rid of — spending one of its three lives on the
    way. Unsetting first removes the question rather than answering it: there is no hook
    left to fire, whichever way this tmux treats a killed pane's `pane-died`.

    `report=False`: a pane charter declined to arm (`_panel_died_hook_argv` returning
    ``None``) has no hook to unset, and a frame launched by a charter that predates #382
    has none either, so a `set-hook -u` for a hook that is not set is an ORDINARY case
    rather than a fault — the same reason `_live_sessions` opts out of reporting for a
    socket no server has run on. Until #408 the ordinary case was the whole of the
    operator's server, where nothing was ever armed at all.
    """
    tmuxctl.run("disarming a panel's respawn hook",
                tmuxctl.server_argv(socket, "set-hook", "-p", "-u", "-t", pane_id,
                                    "pane-died"),
                timeout=5, report=False)


def _pane_last_words(socket: str, harness_pane: str) -> list[str]:
    """Everything a dead pane still holds, tmux's own trailer stripped off.

    **`-S -` is why this is not a bare `capture-pane -p`, and it was measured rather than
    reasoned.** `remain-on-exit` appends its own `Pane is dead (…)` line at the bottom,
    which scrolls the visible screen down by one — so the DEFAULT (visible-screen-only)
    capture of a command that printed exactly one line comes back with that line already
    pushed into history and blanks where it used to be. Verified against tmux 3.7c with
    a missing command: `capture-pane -p` returned tmux's trailer and nothing else, while
    `capture-pane -p -S -` returned `zsh:1: command not found: …` as well. The single
    line that matters here is precisely the one the default form loses.

    `report=False`, for the same shape of reason `_query_pane_dead_status` gives: this
    runs only when `cmd_launch` is already reporting a real failure, and a second,
    louder line about the DIAGNOSTIC having failed would bury the failure it was sent to
    explain. An empty answer needs no special casing — `early_death_message` has a
    complete message either way, which is the property that keeps a failed capture from
    putting the launch back to the zero bytes #384 is about.

    `tmuxctl.server_argv`, never a hand-built `-L`: BOTH launch paths call this now. The
    frame that dies before it is drawn dies exactly the same way inside an operator's own
    tmux (`_launch_in_operator_tmux`), and that server is reached by socket PATH — a
    hard-coded `-L` there would aim the capture at charter's own private server, find no
    such pane, and hand `early_death_message` an empty list. The message would still be
    printed, so nothing would look broken; it would simply have lost the one thing only
    the pane knows.
    """
    out = tmuxctl.run("reading what the harness printed before it died",
                      tmuxctl.server_argv(socket, "capture-pane", "-p", "-S", "-",
                                          "-t", harness_pane),
                      timeout=5, report=False)
    if out.returncode != 0:
        return []
    lines = [ln.rstrip() for ln in out.stdout.splitlines()]
    lines = [ln for ln in lines if ln.strip() and not _PANE_IS_DEAD.match(ln)]
    return lines[-_PANE_LINES_SHOWN:]


def _could_not_have_run(word: str) -> str | None:
    """Why *word* cannot have been a program at all, or ``None`` when it could have.

    Three states, not two, because they take three different remedies and because tmux
    collapses the last two into one indistinguishable answer (see
    `early_death_message`): resolvable on `$PATH` — nothing to say, something ran and the
    exit code is its own; a path that EXISTS but is not executable — `chmod +x`, or name
    the interpreter; and neither — a typo, or something never installed.

    **Asked only AFTER a launch has already failed, never before one.** The same call
    used as a pre-check would decide what `charter frame --` ACCEPTS, and it is not
    entitled to: `shutil.which` answers a question about `execvp`, while tmux hands a
    lone argument to a shell (see `early_death_message`). Used here it decides only how
    an error that has already happened is worded, which narrows nothing.
    """
    if shutil.which(word):
        return None
    if os.path.exists(word):
        return (f"`{word}` is a file that exists but is not executable — `chmod +x` it, "
                f"or put its interpreter first")
    return (f"`{word}` is neither on $PATH nor a path that exists, and with two or more "
            f"words there is no shell in the way to resolve it — so nothing ran")


def early_death_message(argv: list[str], code: int, last_words: list[str]) -> str:
    """What an operator is told when their command died before the frame was ever drawn.

    **The design call #384 left open, and the argument for taking it this way.** `charter
    <harness>` can refuse a missing binary before tmux (see `cmd_launch`'s `if h and not
    shutil.which(h.binary)`), because charter chose that name itself. `charter frame --
    <cmd>` cannot be closed the same way without changing what it ACCEPTS, and what it
    accepts is tmux's rule, not charter's — verified against tmux 3.7c:

    * ONE argument is handed to a shell. `charter frame -- 'ulimit -n; exit 3'` is a
      whole command line: builtins, `;`, pipelines, redirection. `shutil.which` over that
      argument is not a stricter check, it is a check of the wrong thing — the text is
      not even one word.
    * TWO OR MORE arguments are `execvp`'d directly, with no shell anywhere.

    So nothing is refused up front. The failure is reported afterwards, out of what tmux
    actually did — which costs one `capture-pane` on a path that has already failed, and
    is an ANSWER where a pre-check could only ever have been a prediction (a binary that
    resolves can still exit 127 for its own reasons, or have a broken shebang).

    The two forms leave two different residues, which is why this message has two shapes:

    * one argument, missing command — the shell runs, prints its own `command not found`,
      exits **127**. Accurate words already exist; they are just in a pane nobody ever
      attached to, so charter repeats them back rather than inventing worse ones.
    * two or more, missing command — `execvp` fails inside tmux's own child, which exits
      **1** with the pane completely EMPTY. There is nothing to repeat, and a bare 1 is
      indistinguishable from a program that ran and failed. Only there does charter
      answer the resolution question itself (`_could_not_have_run`), and only because
      with no shell involved the inference is sound: a word that resolves to nothing
      provably could not have run.

    Charter fills SILENCE and does not argue with a pane that already spoke — hence the
    early return. A program that printed and then failed plainly did run, so a `$PATH`
    note layered on top of its own words would be wrong as often as not.
    """
    lines = [f"charter frame: `{' '.join(argv)}` exited {code} before the frame was "
             f"drawn — you were never attached, so nothing it printed was ever on screen."]
    if last_words:
        lines.append("  the pane still had this in it:")
        lines.extend(f"    {ln}" for ln in last_words)
        return "\n".join(lines)
    lines.append("  the pane was empty — it printed nothing at all.")
    note = _could_not_have_run(argv[0]) if len(argv) > 1 else None
    if note:
        lines.append(f"  {note}")
    return "\n".join(lines)


def cmd_launch(args) -> int:
    """One launcher, shared by every registered harness and by `charter frame --`."""
    if getattr(args, "probe", False):
        # First, and read-only: nothing below this line — resolving a harness, touching
        # the workspace, reaping a sibling frame's state, ever calling `subprocess.run` —
        # may run before a `--probe` caller gets its one-line answer and nothing else.
        # `getattr` rather than `args.probe`: every other caller of `cmd_launch` in this
        # codebase (tests included) constructs its own `args` without a `probe` field,
        # and none of them means "probe".
        return _report_probe(*frame_ready())
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

    # A REGISTERED harness whose binary is not installed never reaches tmux — and the
    # irony is worth recording, because it is what hid this for a whole review round:
    # `--no-frame` and piped output, the two paths that bypass the frame, already
    # printed charter's own message and exited 127 correctly. Only the terminal path —
    # the NORMAL one, the first thing a new operator types — was silent, and silent in
    # the most complete way available: `new-session` starts, the exec fails instantly,
    # the eager `_query_pane_dead_status` below catches the dead pane and runs
    # `kill-session`, and because `code is not None` from that moment on, the entire
    # `if code is None:` block — panels, `select-pane`, and `attach` — is skipped. There
    # is no pane left and no attach, so nothing is ever drawn. Measured against a real
    # tmux 3.7c under a pty, with `claude` genuinely off `$PATH`: **zero bytes** of
    # output, exit 127, no alternate-screen switch. `charter claude` before installing
    # `claude` returned instantly with no explanation of any kind.
    #
    # Scoped to `if h` deliberately, and this is the whole reason the check sits here
    # rather than over `argv[0]`. A registered harness's binary comes from charter's own
    # registry (`harness.base.binary`), so `shutil.which` is asking about a name charter
    # chose. `charter frame -- <cmd>` is the opposite: `argv[0]` is the operator's own
    # verbatim word, and it is allowed to be a shell builtin, a relative path, or
    # anything else tmux's own resolution accepts — a `which` check over THAT would
    # narrow what the escape hatch accepts, which is a design change and not this fix.
    #
    # The escape hatch's ACCEPTANCE is still exactly that: nothing is refused for it, and
    # #384 deliberately did not change that either (see the module docstring, and
    # `early_death_message`'s own). Its SILENCE is what changed — the same death is now
    # legible from the other end, once tmux has already answered.
    if h and not shutil.which(h.binary):
        return bypass(argv)

    # ONE call (correction 5): asking `tmux -V` twice on a path that branches on the
    # answer once is two subprocesses for one unchanging fact.
    v = tmuxctl.version()
    if v is None:
        util.err(tmuxctl.absent_message())
        return 1
    # Below `tmuxctl.FLOOR` the frame still launches — degrade, never refuse. Nothing is
    # printed here, deliberately: this is a STANDING condition, true on every launch on
    # this machine, and `util.warn` was measured landing 86 bytes before tmux's own
    # `\x1b[?1049h` — the operator's terminal switches to the alternate screen
    # milliseconds later and the line only comes back into view once the frame exits.
    # It is reported by `frame_ready` (`--probe`, `charter frame-probe`) and by
    # `doctor.check_frame` instead; see `frame_ready`'s own docstring.

    ws = workspace.resolve()
    fid = state.frame_id(ws, os.getpid())

    # Inside a tmux the operator already has, the frame is a WINDOW in THEIR server —
    # same layout, no second tmux, no second prefix key (ADR 0018 and the design spec
    # both settle this; `docs/frame.md` describes what it costs). Read from `$TMUX`,
    # which tmux exports into every process it starts in a pane, and CONFIRMED against
    # the server before anything is built on it: that variable outlives the server it
    # names often enough to matter (`env` captures, a `tmux kill-server` under a running
    # script), and charter is then not inside a tmux whatever it says. `None` back means
    # exactly that, and the private-server path below is the right one after all.
    inside = tmuxctl.operator_server()
    if inside is not None:
        rc = _launch_in_operator_tmux(inside[0], inside[1], fid=fid, argv=argv, h=h, v=v)
        if rc is not None:
            return rc

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
        tmuxctl.run("arming remain-on-exit ahead of an already-running server",
                    tmuxctl.server_argv(SOCKET, "set", "-g", "remain-on-exit", "on"))
    state.reap(live_before, server=SOCKET)

    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        # `frame_dir` refuses rather than raises (see charter/frame/state.py) — an id
        # `contain.child` cannot shape into a directory (or a name so long `mkdir` hits
        # ENAMETOOLONG) must not be treated as a Path here just because it usually is one.
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1
    # The pid this launch was handed may have belonged to an earlier launcher for the
    # same workspace, which mints the SAME `fid` — and since #383 `reap` keeps that
    # earlier directory for as long as the pid in its name is live, which right now it
    # is, because it is ours. Everything recorded under this id therefore predates this
    # frame, and a launch beginning is the one moment that can be certain of it.
    #
    # Three things, because three readers inherit. `state.exit_code(fid)` below would
    # read a dead frame's `exit` back as this launch's own return value. `gather.read(fid)`
    # (no freshness check, by design — it is a panel's hot path) would serve a dead
    # frame's scan to every panel until the session's first hook bump. And
    # `state.respawn_attempt` never resets (#382), so the dead frame's respawn counts —
    # at least one per slot, since every panel's `pane-died` hook fired during ITS
    # teardown, and possibly already at `_RESPAWN_ATTEMPTS` — would be charged to this
    # frame's panels, which would then die once and stay dead. `version` is deliberately
    # left: it is a counter panels compare against their last reading, and moving it is
    # `state.bump`'s job, one line below.
    state.clear_exit(fid)
    gather.discard(fid)
    # And the shape this frame starts at is this frame's own: an adopted `density` would
    # be another session's keypress silently overriding this plane's `[frame] density`,
    # and an adopted `panes` would name tmux panes that no longer exist. See
    # `state.clear_shape`.
    state.clear_shape(fid)
    # And the `server` marker is rewritten for the same reason: an adopted directory
    # may name the OTHER server, and a stale marker would make `reap` on this one skip
    # a frame that is now genuinely ours.
    state.record_server(fid, SOCKET)
    state.clear_respawn(fid)
    state.bump(fid)

    try:
        cols, rows = os.get_terminal_size()
    except OSError:
        # `os.get_terminal_size()` raises even when `isatty()` said yes — a tty with
        # nothing behind it to answer `TIOCGWINSZ`. Falling back rather than propagating
        # is the deliberate choice; see `_FALLBACK_SIZE`'s own docstring for why 80x24.
        cols, rows = _FALLBACK_SIZE

    slots = _drawable_slots(cols, rows)
    env = _frame_env(fid, h)
    # Same as the operator's-tmux path above, and needed harder here: charter's private
    # server is SHARED, so a `run-shell` child on it reads whichever launcher's
    # environment started the server. See `state.record_identity`.
    state.record_identity(fid, _frame_identity_env(env))

    conf_path = fdir / "tmux.conf"
    status_path = fdir / "exit"
    conf_path.write_text(_PLACEHOLDER_CONF)

    # `env=` and not merely `tmuxctl.run(env=env)` below: the second only sets what the
    # tmux CLIENT runs with, and a client's environment reaches the new pane ONLY when
    # this call is what starts the server. Every frame after the first on `SOCKET` finds
    # it already running, and its harness would inherit the FIRST frame's
    # `$CHARTER_SESSION_ID` — #411, measured against tmux 3.7c; see `layout.session_argv`.
    #
    # `_frame_identity_env(env)` and NEVER `env`: a `-e` is argv, and argv is world-
    # readable. The rest of the environment still reaches the harness the way it always
    # did, through the client this call is run with. See `_frame_identity_env`.
    #
    # Withheld below `SESSION_ENV_FLOOR`, where `-e` is a parse error rather than a
    # missing feature and would take the whole launch down with it.
    session_cmd = layout.session_argv(
        session=fid, conf=str(conf_path), socket=SOCKET, cols=cols, rows=rows,
        harness_argv=argv,
        env=_frame_identity_env(env) if v >= tmuxctl.SESSION_ENV_FLOOR else None)
    proc = tmuxctl.run("starting the frame", session_cmd, env=env)
    if proc.returncode != 0:
        return 1
    harness_pane = proc.stdout.strip()
    if not harness_pane:
        util.err("charter frame: tmux started the session but did not report a pane id "
                 "— cannot scope the exit-code hook to it")
        return 1
    # See the identical call on the operator's-tmux path: this is what lets a status line
    # tell "I am this frame's harness" from "I inherited this frame's id", which below
    # `SESSION_ENV_FLOOR` a second frame on this shared server genuinely does.
    state.record_harness_pane(fid, harness_pane)

    frame = config.FRAME
    conf_path.write_text(conf_text(hotkey=frame["hotkey"], mouse=frame["mouse"],
                                   history_limit=frame["history_limit"], session=fid))
    src = tmuxctl.run("loading the frame's config",
                      tmuxctl.server_argv(SOCKET, "source-file", str(conf_path)),
                      env=env)
    if src.returncode != 0:
        util.warn("charter frame: continuing without it — mouse/history-limit/hotkey "
                  "settings may not be in effect for this frame")

    env_set = tmuxctl.run(
        "carrying the exit-status path",
        _exit_path_env_argv(socket=SOCKET, session=fid, status_path=str(status_path)),
        env=env)
    if env_set.returncode != 0:
        util.warn("charter frame: continuing without it — the exit code may not be "
                  "recorded for this frame")

    # Ties this session to its own id BEFORE anything else can ask for it — the hotkey
    # bind's action (`charter frame-menu`) and every menu item's own action (`charter
    # frame-action <id>`) both resolve `$CHARTER_SESSION_ID` from a `run-shell`-spawned
    # process's environment, and without this call a frame beyond the first sharing
    # `SOCKET` would silently resolve the FIRST frame's id instead of its own (see
    # `_session_id_env_argv`'s own docstring for what was verified by hand).
    sid_set = tmuxctl.run("carrying the frame id to its own hotkey menu",
                          _session_id_env_argv(socket=SOCKET, session=fid), env=env)
    if sid_set.returncode != 0:
        util.warn("charter frame: continuing without it — the hotkey menu may not find "
                  "this frame's own actions")

    # The second value the same mechanism carries: which interpreter runs charter when
    # the hotkey (or a menu item) fires. Without it both fall back to a bare `charter`
    # on the tmux server's own `$PATH`, and `run-shell` reports the resulting 127 by
    # printing it INTO THE HARNESS PANE — see `conf_text`'s own docstring.
    py_set = tmuxctl.run("carrying charter's own interpreter to the hotkey menu",
                         _charter_py_env_argv(socket=SOCKET, session=fid), env=env)
    if py_set.returncode != 0:
        util.warn("charter frame: continuing without it — the hotkey menu may not open "
                  "on this frame")

    # #390: the same "-m prepends the cwd to sys.path" hole `util.self_relaunch_argv`'s
    # `-P` closes for the panel argv below, closed here as `PYTHONSAFEPATH=1` because
    # the hotkey/menu template above has no room for a per-invocation flag — see
    # `_charter_pythonsafepath_env_argv`'s own docstring.
    safepath_set = tmuxctl.run("carrying PYTHONSAFEPATH to the hotkey menu",
                               _charter_pythonsafepath_env_argv(socket=SOCKET, session=fid),
                               env=env)
    if safepath_set.returncode != 0:
        util.warn("charter frame: continuing without it — the hotkey menu may import "
                  "the wrong charter if this pane's directory has its own `charter/` "
                  "package")

    # The menu itself: what the hotkey actually opens.
    menu.record(fid=fid, entries=_menu_entries(fid, SOCKET, current=_current_density(fid)))

    write_hook = tmuxctl.run(
        "installing the exit-status hook",
        _pane_died_write_hook_argv(socket=SOCKET, harness_pane=harness_pane), env=env)
    if write_hook.returncode != 0:
        util.warn("charter frame: continuing without it — the exit code may not be "
                  "recorded for this frame")

    teardown_hook = tmuxctl.run(
        "installing the session-teardown hook",
        _pane_died_teardown_hook_argv(socket=SOCKET, harness_pane=harness_pane), env=env)

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
        if code != 0:
            # The one path on which NOTHING is ever drawn (#384): no panels, no
            # `select-pane`, no `attach` — the whole `if code is None:` block below is
            # skipped from here on — so this is the only chance the operator has of
            # learning anything at all. Read the pane BEFORE `kill-session` destroys it;
            # afterwards there is nothing left to read.
            #
            # Only for a FAILURE. A command that finished cleanly before the frame came
            # up (`charter frame -- true`) reaches this same branch with 0, and whatever
            # it wrote was its stdout — charter repeating that onto stderr would be
            # inventing output on the wrong stream. Its exit code is the whole message.
            util.err(early_death_message(argv, code,
                                         _pane_last_words(SOCKET, harness_pane)))
        tmuxctl.run("ending the frame after an early death",
                    tmuxctl.server_argv(SOCKET, "kill-session", "-t", fid), env=env)

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
            # `pane_env` on this path too, since #411: a panel splits off a server that
            # may have been started by ANOTHER launcher, so `$CHARTER_WORKSPACE` and
            # `$CHARTER_ROOT` reach it from that launcher unless charter states them.
            # Identity only — a `-e` is argv (see `_frame_identity_env`) — and withheld
            # below `PANE_ENV_FLOOR`, where `split-window` cannot parse the flag.
            panes = _draw_panels(
                SOCKET, slots=slots, fid=fid, harness_pane=harness_pane, env=env, v=v,
                pane_env=_pane_identity_env(env, v),
                sizes=_launch_sizes(fid, slots, cols, rows))
            _arm_panel_respawn(SOCKET, fid=fid, panes=panes, env=env)

            # `split-window` makes the newly created pane the ACTIVE one by default, so
            # after every slot has been drawn, the LAST panel drawn — not the harness —
            # has focus, and an interactive harness never receives a keystroke (measured:
            # `%2 active=1, %0 active=0` after two splits). Pre-existing in
            # `layout.panel_argvs`'s own split ordering, not something this diff
            # introduced, but leaving the frame in a state the operator can actually type
            # into is this launcher's job.
            tmuxctl.run("focusing the harness pane",
                        tmuxctl.server_argv(SOCKET, "select-pane", "-t", harness_pane),
                        env=env)

            # `tmuxctl.interact`, not `tmuxctl.run`: no capture and no timeout — this
            # IS the operator's own terminal for as long as the harness runs, not an
            # admin command whose output (or lifetime) charter should own.
            attach_cmd = tmuxctl.server_argv(SOCKET, "attach", "-t", fid)
            attach = tmuxctl.interact(attach_cmd, env=env)

            code = state.exit_code(fid)
            if code is None:
                # A second ask, for whatever gap the eager check above could not have
                # seen yet (the harness was still alive at that point and died later —
                # the ordinary case, caught here by the hooks, or a rarer one where they
                # never actually reached the server despite reporting success).
                code = _query_pane_dead_status(SOCKET, harness_pane)

    live_after = _live_sessions(SOCKET)
    state.reap(live_after, server=SOCKET)
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
        tmuxctl.report_failure("attaching to the frame", attach_cmd, attach)
        return attach.returncode
    return 0


#: What marks the level a frame is currently at, in the hotkey menu, and the blank that
#: keeps the other rows aligned with it.
#:
#: **ASCII, deliberately.** The obvious `•` (U+2022) is East-Asian *Ambiguous*: one column
#: in most terminals and two under a CJK locale or an ambiguous-width setting, which would
#: shift that one row against its neighbours in a `display-menu` tmux does not pad.
#: `statusline._persona_chips` carries a comment saying ambiguous glyphs "have broken this
#: layout twice", and writing one in a fresh line under that rule is not a trade worth
#: making for a prettier dot. `*` is the marker `git branch` already uses for "the one you
#: are on", and it is unambiguously narrow everywhere.
_DENSITY_MARK = ("*", " ")


def _current_density(fid: str) -> str:
    """The density *fid* is at right now: its own recorded override, else the configured
    default. The same two-source order `frame/slots.verbosity` reads for verbosity, so the
    menu's dot and the panels' own content can never disagree about which level is on."""
    from . import instance
    return (instance.density_level(state.density(fid))
            or instance.density_level(config.FRAME["density"])
            or "normal")


def _menu_entries(fid: str, socket: str, *, current: str) -> list[tuple[str, list[str]]]:
    """Every row of this frame's hotkey menu, in the order it is drawn.

    **"Detach" and the three densities, and no separate key for any of them.** #387 asks
    that a keypress change the density of the RUNNING frame; a second `bind -n` would have
    been the obvious way and is the wrong one, because a tmux key table is server-wide with
    no per-session form (`conf_text`'s own docstring measures what that costs). Every frame
    on `SOCKET` would share whatever key was chosen, and an operator inside their own tmux
    — where charter binds nothing at all — would get no density control regardless. The
    hotkey charter already binds opens this menu, so the density lives here: one bind, one
    server-wide cost, already paid.

    **The argv is `charter frame-density <level>`, not tmux's own commands.** Re-laying out
    a frame is several tmux calls that have to be ordered and whose results have to be
    written back down (`_relayout`); a menu entry is one argv, run once, by
    `cmd_action`'s plain `subprocess.run` of a LIST. `util.self_relaunch_argv()` for the
    interpreter half, for #390's reason — a menu action starts in whatever directory the
    pane is in, and from a charter checkout a bare `-m charter` imports the checkout.

    *current* is marked, not filtered out: an operator who selects the level they are
    already on gets a re-layout that produces the same frame, which is the harmless
    outcome, and a menu whose rows move around depending on state is a menu nobody learns.
    """
    from . import instance
    entries: list[tuple[str, list[str]]] = [
        # The spec's own words, "Detach is allowed and prints how to reattach". `-s fid`
        # (not `-t`): `detach-client`'s `-s` targets every client attached to a SESSION,
        # `-t` a single CLIENT — this frame normally has exactly one attached client, but
        # `-s` is correct even if it ever has more than one.
        ("Detach", tmuxctl.server_argv(socket, "detach-client", "-s", fid)),
    ]
    on, off = _DENSITY_MARK
    for level in instance.FRAME_DENSITY:
        mark = on if level == current else off
        entries.append((f"{mark} density: {level}",
                        util.self_relaunch_argv("frame-density", level)))
    return entries


def _pane_identity_env(env: dict[str, str], v: tuple[int, int]) -> dict[str, str] | None:
    """What a newly split panel pane must be TOLD, or ``None`` when tmux cannot be told.

    One line of logic, named once, because there are now two callers that create panel
    panes — `cmd_launch` and `_relayout` — and the two halves of this are each a decision
    somebody already paid for:

    * :func:`_frame_identity_env`, never the whole environment. A tmux ``-e`` is argv, and
      argv is world-readable: measured on a real environment, `_frame_env` expanded to 138
      argv elements and 7,696 bytes carrying two live service-account tokens. Only the five
      names in :data:`_FRAME_IDENTITY` travel; everything else keeps arriving the way it
      always did.
    * :data:`tmuxctl.PANE_ENV_FLOOR`, below which ``split-window`` cannot parse the flag at
      all — and a flag tmux refuses takes the whole command with it, so the pane is created
      without it rather than not created.

    Written as a wrapper rather than repeated at each call site so a name added to
    `_FRAME_IDENTITY` — it has grown from four to five already — reaches every pane charter
    creates, not just the ones somebody remembered.
    """
    return _frame_identity_env(env) if v >= tmuxctl.PANE_ENV_FLOOR else None


def _relayout_pane_env(fid: str, v: tuple[int, int]) -> dict[str, str] | None:
    """What a pane split by a LIVE RE-LAYOUT must be told. Five names, and the frame's
    OWN values for them — never this process's.

    **`os.environ` here is not this frame's identity, and that is #411 arriving on the one
    command added since.** `cmd_density` normally runs as a `subprocess.run` child of
    `cmd_action`, which is a `run-shell` child of the tmux server. Only
    `CHARTER_SESSION_ID` is session-scoped (`_session_id_env_argv`); `CHARTER_ROOT`,
    `CHARTER_WORKSPACE`, `CHARTER_HARNESS` and `CHARTER_PERSONA` all reach that child from
    the SERVER's environment, and charter's private server is shared between every frame
    on the machine. Measured against tmux 3.7c, the second frame's `run-shell` reported
    the first frame's workspace and harness alongside its own id — so building a `-e`
    payload from `os.environ` would pin ANOTHER frame's plane onto the panes this keypress
    creates, where `$CHARTER_ROOT` and `$CHARTER_WORKSPACE` win outright over every other
    source. The frame's new panels would draw a different plane from the survivors.

    So the values come from `state.identity`, which the LAUNCHER wrote — the one process
    that knew. Recorded in the frame's own directory rather than pushed onto the tmux
    session, because the operator's-tmux path may not write a session option at all (ADR
    0018), and one mechanism that works on both servers beats two that each work on one.

    **A frame with no recorded identity gets the four unknown names as EMPTY, not
    omitted.** Omitting them lets the pane inherit the server's — which is the bug. Empty
    is what every charter reader already treats as absent (`workspace.resolve` and
    `root.find_root` test for truth, not presence), so the pane falls back to resolving
    from its own cwd, which it inherits from the harness pane and which is correct.
    `_frame_identity_env`'s own docstring makes the identical argument for the launch path.

    `CHARTER_SESSION_ID` is forced to *fid* rather than read back: it is the one name this
    process can be certain of — it is how it found the frame at all — and a recorded value
    disagreeing with it would mean the directory belongs to a different frame.

    There is no matching CLIENT environment to decide here. `cmd_launch` passes one
    because it is the call that STARTS charter's server; by the time a density change
    runs, the server is up on either socket and a pane's environment comes from the server
    and this `-e`, never from the tmux client's own process. A client env would have been
    a distinction no test could tell from its absence, so the callers pass `env=None`.
    """
    known = state.identity(fid)
    values = {name: known.get(name, "") for name in _FRAME_IDENTITY}
    values["CHARTER_SESSION_ID"] = fid
    return values if v >= tmuxctl.PANE_ENV_FLOOR else None


def _relayout(socket: str, *, fid: str, harness_pane: str, panels: dict[str, str],
              want: list[str], v: tuple[int, int],
              window_cols: int, window_rows: int) -> dict[str, str]:
    """Make the running frame's panes match *want*, and return the map that resulted.

    Kill what is no longer wanted, split what is newly wanted, re-arm the hooks, re-assert
    every size. In that order, and each step is here for a measured reason:

    * **Disarm before killing.** See :func:`_disarm_panel_respawn` — otherwise the panel
      charter just closed comes straight back, one respawn life poorer.
    * **Split off the HARNESS pane, never off a sibling panel.** `_draw_panels` already
      does this and it is `frame/layout.py`'s own module-docstring measurement: tmux
      renumbers pane INDICES on every split, and a target derived from an earlier split is
      the bug that shipped a four-slot frame with one panel. The harness pane's `%N` is
      read back out of the frame's own state (`state.panes`), which is why that file
      exists at all.
    * **Re-assert every size afterwards, not only the new panes'.** A `split-window -l` or
      a `kill-pane` makes tmux redistribute the remaining panes proportionally — the same
      engine the `window-resized` hook exists to correct after a window resize. The panes
      that merely SURVIVED a density change are the ones that get stretched by it, so
      they are exactly the ones a `-l` on the new pane cannot fix. :func:`_reassert_sizes`
      does it, and *window_cols*/*window_rows* are why it takes a measurement rather
      than a constant: `bottom`'s height depends on the window it is in — on its rows
      (#488) and, since #500, on its columns and this frame's density too, because a
      panel narrower than `statusline._LEFT_W` draws no table and `terse` draws at most
      `slots._TERSE_ROWS` of one.

    Slots whose `split-window` fails are simply absent from the returned map, as at launch:
    a decorative panel that could not be drawn must not take down a frame that is running.

    **What the ordering does and does not buy.** Kill-then-split-then-re-arm means an
    interruption cannot leave a pane armed for respawn that no longer exists, and cannot
    leave the resize hook naming a pane that has been killed *while* a later step is still
    running — each step's own invariant holds the moment it returns. It is NOT a
    transaction: charter is several tmux commands here, tmux has no way to run them as
    one, and a process killed midway can genuinely leave a frame with the old `left` gone
    and the new `right` not yet split. That state is inconsistent but not corrupt — every
    pane in it is real, the recorded map is rewritten from what actually came back, and
    the next density change (or the next launch) resolves it. Claiming more than that
    would be claiming an atomicity nothing here has.
    """
    # `env=None` throughout: see `_relayout_pane_env` for why a live re-layout has
    # no client environment to decide.
    pane_env = _relayout_pane_env(fid, v)
    keep: dict[str, str] = {}
    for slot, pane_id in panels.items():
        # Checked ABOVE the `want` branch, not inside the kill branch — #475. `panels` is
        # `state.panes(fid)`, read back out of the frame's directory on disk, so it is
        # not tmux's own word for a pane any more whichever branch it takes. The guard
        # used to sit below the `continue`, which meant every slot the new density KEPT
        # went into `keep` unexamined and straight on to a `resize-pane -t` and a hook —
        # a `%1;kill-server` in that file armed `kill-server` on every window resize for
        # the life of the window. The property is that nothing off disk is used as a pane
        # id until it has tmux's own shape, and a guard on one branch of a loop is a
        # guard on the wrong thing.
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        if slot in want:
            keep[slot] = pane_id
            continue
        _disarm_panel_respawn(socket, pane_id=pane_id)
        tmuxctl.run(f"closing the {slot} panel",
                    tmuxctl.server_argv(socket, "kill-pane", "-t", pane_id))

    missing = [s for s in want if s not in keep]
    if missing:
        # Split in `want`'s order, which is the density level's own — see
        # `instance.FRAME_DENSITY` for why that order is geometry rather than reading
        # order. Every split targets the harness pane, so a slot added to a frame that
        # already has the others lands exactly where a launch would have put it.
        keep.update(_split_panels(
            socket, slots=missing, fid=fid, harness_pane=harness_pane, env=None,
            pane_env=pane_env,
            sizes=layout.slot_sizes(
                want, window_rows=window_rows,
                content_rows=frame_slots.bottom_rows_wanted(fid, cols=window_cols))))
        _arm_panel_respawn(socket, fid=fid,
                           panes={s: keep[s] for s in missing if s in keep}, env=None)

    _install_resize_hook(socket, harness_pane=harness_pane, panes=keep, v=v,
                         env=None, fid=fid, replacing=True)
    _reassert_sizes(socket, fid=fid, panes=keep,
                    window_cols=window_cols, window_rows=window_rows)
    # `split-window` makes each new pane the ACTIVE one, so without this the operator is
    # left typing into a panel. The same correction `cmd_launch` makes after its own
    # splits, for the same reason.
    tmuxctl.run("focusing the harness pane",
                tmuxctl.server_argv(socket, "select-pane", "-t", harness_pane))
    return keep


def _reassert_sizes(socket: str, *, fid: str, panes: dict[str, str],
                    window_cols: int, window_rows: int) -> None:
    """Re-apply every pane in *panes* the size `layout.slot_sizes` says it should have in
    a *window_cols* x *window_rows* window.

    The one place a pane is told its size after it exists, shared by the two callers that
    need it and for two different reasons: :func:`_relayout`, because a `split-window -l`
    or a `kill-pane` makes tmux redistribute the SURVIVING panes proportionally (so the
    panes a density change merely kept are exactly the ones a `-l` on the new pane cannot
    fix); and :func:`cmd_resize`, because the whole window just changed size under all of
    them.

    **The sizes are recomputed here, never carried in.** `bottom`'s height depends on the
    window it is in (`layout.bottom_rows`), so a caller passing a launch-time number
    would re-assert a height that was right for a window that no longer exists — measured
    on tmux 3.7c, asserting `-y 40` in a 20-row window leaves the harness 1 row.
    *window_rows* is therefore a measurement the CALLER just took, not a remembered one.

    **And *window_cols* alongside it (#500), for the same reason and a sharper one.** A
    resize changes the window's WIDTH too, and how tall `bottom` should be depends on it:
    below `statusline._LEFT_W` the panel draws no table, so it wants one row. Dropping the
    width here — `cmd_resize` measured it and threw it away as `_cols` — is what made a
    narrowed terminal keep a table-sized pane it could no longer draw a table in, on every
    step of the drag rather than only at launch.

    Every pane id is checked before it is used, even though both callers already checked
    theirs. `_panel_died_hook_argv`'s own rule ("every value that reaches the text is
    what decides, never where it came from") applies to a `-t` argument too, and #475 was
    exactly a builder that documented "safe because my caller checked" growing a second
    caller.

    `report=False`: a pane that has since died (the operator closed it, a panel crashed
    between the map being read and this running) makes `resize-pane` fail, and that is
    not an integration failure worth printing over the agent's own screen.
    """
    sizes = layout.slot_sizes(
        list(panes), window_rows=window_rows,
        content_rows=frame_slots.bottom_rows_wanted(fid, cols=window_cols))
    for slot, pane_id in panes.items():
        if slot not in _RESIZE_FLAG or slot not in sizes:
            continue
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        tmuxctl.run(f"restoring the {slot} panel's size",
                    tmuxctl.server_argv(socket, "resize-pane", "-t", pane_id,
                                        _RESIZE_FLAG[slot], str(sizes[slot])),
                    report=False)


def cmd_resize(args) -> int:
    """`charter frame-resize --frame <fid>` — re-apply this frame's pane sizes for the
    window's CURRENT size. Fired by the `window-resized` hook, never typed.

    **Why this is a command and not a string in a hook.** tmux's layout engine
    redistributes every pane proportionally on a resize, so the intended sizes have to be
    re-applied afterwards; until #488 the hook carried them as literal text, computed once
    at layout time. `bottom`'s height is content-and-window dependent now
    (`layout.bottom_rows`), and a literal is not merely stale then — measured on tmux
    3.7c, a hook still asserting `-y 40` after the window shrank to 20 rows left the
    harness pane **1 row tall**. Recomputing needs charter, so the hook calls charter.

    **Always 0, and every refusal is a quiet no-op** — `cmd_respawn`'s reason exactly:
    this is a `run-shell -b` child, nothing reads its status, and a non-zero exit is what
    makes tmux print into the harness pane on the un-backgrounded path. There is no
    screen left to report on that is not the agent's own.

    Refusals, in order:

    * no frame on the argv and no `$CHARTER_SESSION_ID` — not fired by a frame at all;
    * a harness pane that is not tmux's own `%<digits>` — it arrives off disk, and
      `_window_size` is about to target it;
    * no panes recorded — a frame whose panels all failed to draw has nothing to resize.

    The frame comes off the argv first and the environment only as a fallback, matching
    `cmd_respawn`: on the operator's own server there IS no `$CHARTER_SESSION_ID` to
    read, because that variable is a session option charter does not write there.

    **Both dimensions are used, not just the rows (#500).** The window's WIDTH decides
    whether `bottom` can draw its table at all (`statusline._LEFT_W`), so narrowing a
    terminal below 95 columns has to shrink the pane to the one-row strip it can actually
    fill. This measured the width and discarded it as `_cols`, which left a narrowed
    frame re-asserting a table-sized pane it drew one line into — on every step of the
    drag, so the harness stayed pinned at `layout.HARNESS_MIN_ROWS` for as long as the
    terminal stayed narrow.

    **This is the hot path of the whole feature.** A terminal drag fires `window-resized`
    once per size change, so this runs repeatedly and in the background while the
    operator is still dragging. Everything it reads is cheap by construction:
    `state.harness_pane`/`state.panes` are two small files, `_window_size` is one
    `display-message`, and `frame_slots.bottom_rows_wanted` goes through
    `gather.row_count`, which answers from the frame's cache and never runs a git sweep
    (see its own docstring) — and at a width below `_LEFT_W` it does not even ask, since
    the answer is one row whatever the count is.
    """
    fid = getattr(args, "frame", None) or os.environ.get("CHARTER_SESSION_ID", "")
    if not fid:
        return 0
    harness_pane = state.harness_pane(fid) or ""
    if not _PANE_ID_RE.fullmatch(harness_pane):
        return 0
    panes = state.panes(fid)
    if not panes:
        return 0
    socket = state.frame_server(fid) or SOCKET
    cols, rows = _window_size(socket, harness_pane)
    _reassert_sizes(socket, fid=fid, panes=panes, window_cols=cols, window_rows=rows)
    return 0


def cmd_density(args) -> int:
    """`charter frame-density <level>` — re-lay-out THIS frame, and write nothing else.

    Fired by a hotkey-menu selection (`_menu_entries`), and typeable by hand from inside a
    frame. The frame is resolved from `$CHARTER_SESSION_ID` exactly as `cmd_menu` and
    `cmd_respawn` resolve theirs — never from anything baked into a menu action, since one
    bind and one action template are shared by every frame on `SOCKET`.

    **charter.toml is not touched, and that is the whole design.** `[frame] density` sets
    what a frame STARTS at; this changes what one running frame IS. Charter's rule is that
    machine-written config belongs somewhere a machine may rewrite whole, and a
    hand-maintained, committed file that carries an operator's comments is the opposite of
    that — so the override goes in the frame's own state directory (`state.record_density`),
    which `state.reap` deletes entire when the frame ends. Relaunch, and the configured
    default is back.

    **Always 0, and every refusal is a quiet no-op**, for `cmd_respawn`'s reason: this
    normally runs as a `run-shell` child of a menu selection, where the only screen left to
    report on is the agent's own — the one rectangle ADR 0018 says charter never draws in.

    Refusals, in order:

    * no `$CHARTER_SESSION_ID` — not fired from inside a frame at all;
    * a *level* outside `instance.FRAME_DENSITY` — a closed set of three constants charter
      wrote itself, so this can only be a hand-typed argument;
    * a harness pane that is not tmux's own `%<digits>` — a frame launched by a charter
      that predates `state.record_harness_pane`, or a corrupted file. There is nothing to
      split off, and guessing at a pane id is the one thing `frame/layout.py`'s module
      docstring measures the cost of. (An empty PANEL map is not a refusal: a frame whose
      panels all failed to draw can still be given some.)

    The density is recorded BEFORE the panes move, so a re-layout that fails halfway still
    leaves the panels that survive drawing at the density the operator asked for. The
    version is bumped afterwards: that is what makes every surviving panel repaint with the
    new verbosity, and a bump before the layout settled would repaint into the old shape.
    """
    from . import instance
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    level = instance.density_level(getattr(args, "level", None))
    if not fid or level is None:
        return 0
    # The harness pane comes from `state.harness_pane`, the record ADR 0019's own
    # `is_live` reads — not from a second copy of its own. `_PANE_ID_RE` still guards it:
    # it arrives off disk here rather than off `split-window`'s stdout, and it is about to
    # be interpolated into a hook action and a split target.
    harness_pane = state.harness_pane(fid) or ""
    panels = state.panes(fid)
    if not _PANE_ID_RE.fullmatch(harness_pane):
        return 0
    socket = state.frame_server(fid) or SOCKET
    v = tmuxctl.version()
    if v is None:
        return 0

    state.record_density(fid, level)
    cols, rows = _window_size(socket, harness_pane)
    want = _drawable_slots(cols, rows, instance.density_slots(level))
    panes = _relayout(socket, fid=fid, harness_pane=harness_pane, panels=panels,
                      want=want, v=v, window_cols=cols, window_rows=rows)
    state.record_panes(fid, panels=panes)
    # Re-recorded so the menu's own mark moves with the frame. `menu.record` rewrites the
    # table whole, and every action id is minted from position, so the ids the operator's
    # currently-open menu is holding stay valid for the same rows.
    #
    # Private server only, matching `cmd_launch`, which records no menu inside an
    # operator's tmux either: charter binds no key there, so there is nothing to open the
    # menu with — and the "Detach" row it would grow targets `detach-client -s <fid>`, a
    # SESSION name that does not exist on that server, where a frame is a window.
    if not tmuxctl.is_operator_socket(socket):
        menu.record(fid=fid, entries=_menu_entries(fid, socket, current=level))
    state.bump(fid)
    return 0


def cmd_menu(args) -> int:
    """Open this frame's own menu, on the screen of whoever actually pressed the hotkey.

    `fid` is resolved from `$CHARTER_SESSION_ID`, carried session-scoped via
    `_session_id_env_argv` rather than baked into the bind's own text (see
    `conf_text`'s docstring for why that split is load-bearing, not incidental): the
    SAME bind text is shared by every frame on `SOCKET`, so the frame it opens a menu
    FOR has to be resolved here, at the moment the key actually fires, never earlier.

    `args.client` is `#{client_name}`, expanded by tmux INSIDE the bind's own
    `run-shell` text before this process ever starts (see `conf_text`'s docstring) — not
    queried here. An earlier version queried `list-clients` and picked the first client
    reported when several were attached to one frame; confirmed WRONG by observation,
    not merely suboptimal: with two real ptys attached to one session, pressing the
    hotkey on the SECOND-attached client drew the menu on the FIRST client's screen,
    and the presser saw nothing — worse than tmux's own unscoped single-client default
    this module was built to replace, which happened to guess right in that same
    two-client, one-frame setup when given no `-c` at all (also verified by hand,
    separately). Carrying the presser's own name through the bind removes the guess
    rather than making it a better one — confirmed against the same two-pty setup:
    each press now resolves to its OWN presser, never the other one's, regardless of
    attach order (see `tests/test_frame_tmux_integration.py`'s
    `MenuClientIntegration`). This is the one property actually verified for the
    ORIGINAL single-frame bug this whole mechanism exists to fix: nobody ever directly
    reproduced tmux's default guess failing for a single frame with several clients —
    only the two-FRAME case (`display-menu -t <session>` alone, no `-c`, resolving to
    whichever session was "current" server-wide) was. `#{client_name}` closes both
    regardless, by construction, rather than leaving the single-frame case resting on
    an inference.

    A missing client (`args.client` empty — `#{client_name}` failed to expand, or this
    was invoked some other way) is a quiet no-op: there is no screen to draw on, and no
    screen left to report that on either. So is an EMPTY menu (`not menu.build(fid)`):
    `display-menu` requires at least one `name key command` triple and fails outright
    (rc 1, `not enough arguments` — tmux's own text, confirmed against 3.7c with a real
    attached client) without one — reachable with a genuine `$CHARTER_SESSION_ID`
    whose frame has recorded nothing yet, and, before this guard, WOULD have been
    reachable with none at all (`menu.build("")` is always empty — `state.frame_dir`
    refuses an empty id — so a keypress arriving with no session id would otherwise
    have gone on to build a zero-item `display-menu` call and fail loudly for no
    reason the operator could see).
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    if not args.client or not menu.build(fid):
        return 0
    # `tmuxctl.interact`: `display-menu` draws on an attached client and does not return
    # until the operator chooses or dismisses it, so it belongs with `attach` on the
    # no-capture, no-timeout side of `tmuxctl` — time-boxing it would close a menu for
    # the crime of being read slowly.
    return tmuxctl.interact(menu.menu_argv(fid, SOCKET, client=args.client)).returncode


def cmd_respawn(args) -> int:
    """`charter frame-respawn <slot> --pane %N` — bring one dead panel back, or stop.

    Fired only by a panel pane's own `pane-died` hook (`_panel_died_hook_argv`), never
    typed. It exists as a charter command because the two things standing between "a
    panel died" and "respawn it forever" — a count, and a wait — are both things tmux
    itself cannot do.

    **Which failures reach here at all.** `frame/panel.py` now holds its pane open for
    every failure charter's own Python can see, painting the reason into the pane rather
    than exiting into tmux's `Pane is dead (status N)` message (#382's first half). So a
    panel that reaches this hook is one whose PROCESS is gone — the interpreter failing
    to start, a SIGKILL, an OOM — which is the only kind restarting could ever help.

    **Always 0.** This is a `run-shell -b` child; nothing reads its status, and a
    non-zero exit is exactly what makes tmux print into the harness pane on the
    un-backgrounded path (see `_panel_died_hook_argv`). Every refusal below is a quiet
    no-op for the same reason: there is no screen left to report it on that is not the
    agent's own.

    **Which server, and #408.** This resolved nothing: it spelled `["tmux", "-L", SOCKET,
    …]` and asked `_live_sessions(SOCKET)`, so a hook fired inside the operator's own tmux
    would have talked to charter's private server about a session that is not there — the
    other half of the mismatch `_panel_died_hook_argv` had. The server comes from
    `state.frame_server(fid)`, which the launcher records for every frame on either socket,
    and both the liveness question and the respawn argv are built from it through the one
    place that knows `-L` from `-S` (`_frame_is_live`, `tmuxctl.server_argv`).

    **The frame comes off the argv first, and the environment only as a fallback.**
    `--frame` is what the hook passes now: on the operator's server there IS no
    `$CHARTER_SESSION_ID` to read, because that variable is a session option
    (`_session_id_env_argv`) and charter writes none there. The fallback is kept for a
    frame armed by a charter that predates this, whose hooks are already installed and
    outlive the upgrade.

    Refusals, in the order they are checked:

    * no frame on the argv and no `$CHARTER_SESSION_ID` — not fired by a frame at all, so
      there is no frame to resolve or count against (`cmd_menu` treats the same gap the
      same way);
    * a *slot* with no renderer — bringing it back would recreate exactly the
      permanently-dead pane `cmd_launch`'s `unimplemented` filter exists to prevent;
    * a *pane* that is not tmux's own `%<digits>` — the value arrived through text tmux
      re-parsed, so it gets the same treatment `_PANE_ID_RE` already gives the resize
      hook's target;
    * a count `state.respawn_attempt` could not record (`None`) — treating that as
      "attempt 1" would respawn forever, the one outcome the cap exists to prevent;
    * the cap itself, `_RESPAWN_ATTEMPTS`. Giving up is not a failure here: the pane
      stays dead with tmux's own message in it, which is the spec's own "a dead panel
      stays visible with its error".

    **The liveness check is AFTER the backoff, deliberately.** Every panel pane dies
    when the frame is torn down, so every panel's hook fires on the way out — the
    ordinary end of every frame, not an edge case. Asking before the sleep would race
    `kill-session`; asking after it means the teardown has had the whole backoff to
    finish, and a respawn into a session that no longer exists (one failure report per
    panel, for nothing being wrong) is avoided rather than reported.
    """
    fid = getattr(args, "frame", None) or os.environ.get("CHARTER_SESSION_ID", "")
    if not fid or args.slot not in frame_slots.SLOTS:
        return 0
    if not _PANE_ID_RE.fullmatch(args.pane or ""):
        return 0
    socket = state.frame_server(fid) or SOCKET
    attempt = state.respawn_attempt(fid, args.slot)
    if attempt is None or attempt > _RESPAWN_ATTEMPTS:
        return 0
    # `min(...)`: the two constants are the same length today and `attempt` is already
    # capped above, so this changes nothing now — it is here so raising
    # `_RESPAWN_ATTEMPTS` alone degrades to "wait the longest backoff again" instead of
    # an IndexError inside a tmux hook, where nothing would print it.
    time.sleep(_RESPAWN_BACKOFF[min(attempt, len(_RESPAWN_BACKOFF)) - 1])
    if not _frame_is_live(socket, fid):
        return 0
    tmuxctl.run(
        f"bringing the {args.slot} panel back",
        tmuxctl.server_argv(socket, "respawn-pane", "-t", args.pane, "--",
                            *layout.panel_command(slot=args.slot, session=fid)))
    return 0


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
