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

from . import config, contain, harness, instance, tui, util, workspace
from .frame import (builtin_actions, choose, component, gather, layout, overlay,
                    palette, picker, state, switch, tmuxctl)
# Aliased: `cmd_launch` already has a local variable named `slots` (the VISIBLE slot
# list `layout.visible_slots` returns) — importing the renderer registry under its own
# name would be shadowed by that local the moment it's assigned, and a `slots.SLOTS`
# lookup after that point would silently resolve to the wrong thing (an `AttributeError`
# on a `list`, not a helpful one).
from .frame import slots as frame_slots
# Aliased for the same reason: `_split_panels` names its resolved `[frame] chrome` word
# `chrome`, and `_surface_argvs` takes it as a parameter of that name — the module and
# the value are two different things and the value is the one the reader is following.
from .frame import chrome as chrome_mod

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
#: interpreter the hotkey bind runs charter with. Owned by `tmuxctl` so every module
#: that spells the name reaches one definition; see `_charter_py_env_argv` and
#: `conf_text`.
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

#: The format that reports ONE PANE's own width, which is the question
#: :func:`_variable_pane_cols` asks and which the window's width above is not the answer to
#: (#510). A `[frame] slots` naming `right` before the table insets that pane by 23
#: columns, so charter has always had to turn the one into the other by arithmetic
#: (`layout.repos_cols`); this is the number tmux itself holds. Separate from
#: :data:`_WINDOW_SIZE_FORMAT` and not a third field appended to it, because they are
#: asked at different moments and of different targets — the window before anything moves,
#: the pane after the side panels have been put back where they belong.
_PANE_WIDTH_FORMAT = "#{pane_width}"

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


def conf_text(*, hotkey: str, mouse: bool, history_limit: int, session: str,
              toggles: dict | None = None) -> str:
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

    **`focus-events` is the THIRD genuine server option here, and it is written `-g` for
    exactly `escape-time`'s reason.** Spec §4f closes the component event kinds at six,
    two of which are `focus` and `blur`; tmux ships `focus-events` OFF, and with it off
    tmux writes `\\x1b[?1004l` to the client and never delivers a pane focus transition,
    so those two kinds do not exist at all until this line runs. The Phase 2 plan asked
    for `set -t <session> focus-events on`; measured on this machine, that spelling is a
    lie about scope and the `-g` above is the true one. tmux 3.7c AND tmux 3.2 —
    charter's own `tmuxctl.FLOOR`, built from the release tarball and run — both answer::

        $ tmux -L t7 show-options -s | grep focus-events
        focus-events off                 # server table
        $ tmux -L t7 show-options -g | grep focus-events
        (absent from -g)                 # NOT a session option, on either version
        $ tmux -L t7 set -t one focus-events on
        $ tmux -L t7 show-options -t two focus-events
        focus-events on                  # the SIBLING session, which nobody set

    `mouse`, run through the identical probe, answers `''` for the sibling — that is what
    a genuinely session-scoped option looks like, and `focus-events` is not one. So
    `set -t <session>` here would not contain anything; it would only read as though it
    did, sitting in a list whose first three lines are session-scoped precisely so frame
    N cannot rewrite frame N-1's settings. Nothing is actually lost by writing it
    globally — every frame wants the identical `on`, like `escape-time` and
    `remain-on-exit` — and tmux's own source says the same thing outright: 3.2's
    `options-table.c` carries `.name = "focus-events"` with `.scope =
    OPTIONS_TABLE_SERVER` and `.default_num = 0`.

    **Do not replace this with a runtime check against `#{client_flags}`.** That format
    reports `focused` whether or not focus events are being delivered, so a guard written
    against it passes with the feature dead. Measured on 3.7c and 3.2, one attached pty
    client, flipping the option underneath it::

        focus-events off (feature DEAD)  client_flags='attached,focused,UTF-8'
        focus-events on  (feature LIVE)  client_flags='attached,focused,UTF-8'
        focus-events off again           client_flags='attached,focused,UTF-8'

    Identical in all three states. The only readable evidence is the OPTION
    (`show-options -t <session> focus-events`), which is why this line — not a probe — is
    what makes §4f's `focus`/`blur` exist. Pinned by
    `tests/test_frame_tmux_integration.py`'s `FocusEventsIntegration`.

    Neither `pane-died` hook lives here — see `_pane_died_write_hook_argv` and
    `_pane_died_teardown_hook_argv` for why they are issued as their own tmux commands
    instead of text baked into this string.

    *session* is the frame id, which `state.frame_id` already sanitises (see
    `charter/frame/state.py`) before this function ever sees it — interpolated into
    plain `set -t <session> ...` config text, never into a shell command string, so it
    carries none of the risk `_EXIT_PATH_ENV`'s docstring describes for `status_path`.

    `hotkey` opens this frame's own palette (§4h — `F2` IS the palette; there is no menu
    beside it): `bind -n {hotkey} run-shell '"$CHARTER_PY" -m charter frame-palette'`. A
    key BINDING has no per-session form the way `status`/`mouse`/`history-limit` above do
    — key tables are server-wide in tmux, so every frame on `SOCKET` ends up sharing this
    exact bind text, "last launched wins" exactly like `escape-time`/`remain-on-exit`/the
    `WheelUpPane` bind two lines down already do. That is only safe here because the
    action itself carries no frame identity: `charter frame-palette` (`cmd_palette`)
    resolves the CURRENT session from `$CHARTER_SESSION_ID` — carried out of band via
    `set-environment`, see `_session_id_env_argv` — at the moment the key actually fires,
    never from anything baked into this text. A bind that embedded one frame's own id
    here would start opening the WRONG frame's palette the instant a second frame
    launched, the same trap this function's own docstring already names for
    `mouse`/`history-limit`, just reached through a binding instead of a session-scoped
    `set`.

    `"#{client_name}"` is a SECOND thing this same bind carries, for a DIFFERENT
    reason: which of possibly several clients attached to one frame should be TOLD when
    an action refuses. Format expansion resolves `#{client_name}` in the context of
    whoever's keypress is firing the bind — verified by hand with two real ptys attached
    to one session, pressing the hotkey from each in turn: each press's own `run-shell`
    resolved its OWN presser's client name, never the other one's, regardless of which
    was attached first. `charter frame-palette` receives it as a plain argv value (`args
    .client` in `cmd_palette`), carries it to the palette's own pane, and hands it to
    `display-message -c`. Earlier, this module queried `list-clients` and guessed the
    first one reported when several clients were attached — confirmed wrong, on the menu
    this replaced: pressing the hotkey on the SECOND-attached client drew on the FIRST
    client's screen, worse than tmux's own unscoped default single-client guess this
    module was replacing. Carrying the presser by name removes the guess entirely rather
    than making it a better one.

    **What the palette does NOT carry it for, and this is a real difference from the
    menu.** A `display-menu` was drawn per client; the palette is a PANE (§4k), and a
    pane belongs to the window, so with two clients attached to one frame both of them
    see it. That is the price of the surface being something charter draws rather than
    something tmux draws, and it is the same price every other panel already pays.

    The last line is `frame/overlay.hatch_bind()` — **the escape hatch**, and it is here
    rather than beside the hotkey because it is the one bind that must keep working when
    charter's own code does not. It carries no frame identity at all (the pane ids live
    in a window option the presser's own window answers for, see that module), and it
    runs `run-shell -C`, which is tmux executing tmux commands with no shell and no
    second charter process. `run-shell -C` first exists in tmux 3.2 — `tmuxctl.FLOOR`
    exactly — so below the floor this one line fails to parse where the rest of this text
    still applies. That is the same band `below_floor_message` already warns about, and
    it is why the line goes LAST: whatever a tmux too old to parse it does with the rest
    of the file, everything charter needs has already been applied by the time it gets
    there.

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
    command by printing `'charter frame-palette "/dev/ttys020"' returned 127` INTO THE
    HARNESS PANE and dropping it into copy-mode — charter drawing in the one rectangle
    ADR 0018 says it never draws. The interpreter is carried out of band via
    `set-environment` (`_charter_py_env_argv`) rather than interpolated here, for the
    same reason `status_path` is (see `_EXIT_PATH_ENV`): an absolute path re-embedded
    inside this nested tmux-quote layer is one apostrophe away from the silent
    corruption the module docstring measures. Verified against tmux 3.7c that a
    session-scoped `CHARTER_PY` reaches the shell this bind spawns and expands there,
    and that this exact bind text survives `source-file` intact (`list-keys` reads it
    back byte for byte).

    *toggles* is ``{component name: key}`` (`instance.frame_toggles`) and each pair
    becomes one more `bind -n`, running `charter frame-toggle <name>` — which shows or
    hides that one component on the running frame. Empty for every plane spelled with
    `slots` or `density`, because there is nowhere in those to write a key and charter
    binds none by default: a `bind -n` is server-wide and intercepts the key BEFORE the
    harness pane sees it, so a shipped default would quietly take keys away from Claude
    Code (or codex, or whatever the operator ran) on every plane that has a charter.toml.

    These are frame-agnostic for the same reason the hotkey bind above is, and they carry
    no `#{client_name}`: a toggle changes the FRAME, not what one client is looking at, so
    unlike the palette there is nothing here to draw on a particular presser's screen.
    `cmd_toggle` resolves which frame from `$CHARTER_SESSION_ID` when the key fires.

    **Both halves of a toggle line are refused rather than escaped, and each on its own
    line below.** A component name and its key arrive from the same committed file as
    `hotkey`, land in the same `source-file` text, and a newline in either ends the `bind`
    line and starts a second tmux command with no keypress — the incident
    `instance._HOTKEY_RE`'s docstring measures. The key is asked of `instance.toggle_key`,
    which IS that constant; the name is asked of `frame/component.py`'s `usable_id`, which
    is the alphabet a component id already travels to tmux under. Neither is a new
    validator, and both are asked HERE as well as at the config boundary because
    `instance.component_tables` accepts a provider's id on the strength of an installed
    entry point NAME — a word charter did not choose and cannot un-install.

    A refused pair costs its key and nothing else: the panel is still placed, still split
    for and still drawn, and the rest of the frame's keys still bind. That is the one
    degrade available, since the alternative is a `source-file` that fails and takes
    `mouse`, `history-limit` and the palette's own hotkey down with it.

    Verified against tmux 3.7c, sourcing exactly what this returns for a two-key
    arrangement — `source-file` returns 0 and `list-keys -T root` reads all four binds
    back, the palette's, both toggles' and the hatch's, byte for byte::

        bind-key -T root F2  run-shell "\\"\\$CHARTER_PY\\" -m charter frame-palette \\"#{client_name}\\""
        bind-key -T root F7  run-shell "\\"\\$CHARTER_PY\\" -m charter frame-toggle repos"
        bind-key -T root F12 run-shell -C "#{@charter_hatch}"
        bind-key -T root M-s run-shell "\\"\\$CHARTER_PY\\" -m charter frame-toggle right"

    **They sit between the hotkey bind and the escape hatch, and both ends of that are
    load-bearing.**

    *After the palette's bind*, because that is the order that keeps the config boundary's own
    collision refusal honest rather than masked. tmux has no notion of a key conflict — a
    later `bind -n` replaces an earlier one — so a component that bound `F2` here would
    take the palette away, and that is exactly the consequence `instance.component_tables`
    refuses to allow and its test asserts. Emitting these first would have made the same
    deletion harmless-looking and the guard unpinnable, which is how `layout.py`'s own
    masked guard got there (#553).

    *Before `overlay.hatch_bind()`*, because that line's own reason for going last (see
    above) is that a tmux below `tmuxctl.FLOOR` cannot parse `run-shell -C` and everything
    charter needs must already have been applied by the time `source-file` reaches it. A
    toggle bind appended after it would be the first thing such a tmux dropped, and the
    operator would lose their keys to a version skew that costs nothing else. Pinned by
    `test_the_escape_hatch_is_still_the_last_line`.
    """
    lines = [
        f"set -t {session} status off",
        f"set -t {session} mouse {'on' if mouse else 'off'}",
        f"set -t {session} history-limit {int(history_limit)}",
        "set -g escape-time 0",
        "set -g remain-on-exit on",
        "set -g focus-events on",
        f"bind -n {hotkey} run-shell "
        f"'\"${_CHARTER_PY_ENV}\" -m charter frame-palette \"#{{client_name}}\"'",
        "bind -n WheelUpPane if-shell -F -t = '#{mouse_any_flag}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
    ]
    for name, key in (toggles or {}).items():
        if not component.usable_id(name):
            continue
        if instance.toggle_key(key) is None:
            continue
        lines.append(f"bind -n {key} run-shell "
                     f"'\"${_CHARTER_PY_ENV}\" -m charter frame-toggle {name}'")
    # The escape hatch stays the LAST line, which is this file's own invariant and not
    # this loop's convenience: `frame/overlay.py`'s `hatch_bind` is the one bind that has
    # to keep working when charter's code does not, and it uses `run-shell -C`, which
    # first parses in tmux 3.2 (`tmuxctl.FLOOR`). Below the floor that single line fails
    # to parse, and everything charter needs must already have been applied by the time
    # `source-file` reaches it — a toggle bind appended after it would be the first thing
    # such a tmux dropped. So the toggles are inserted BEFORE it, never appended after.
    lines.append(overlay.hatch_bind())
    lines.append("")
    return "\n".join(lines)


def _charter_py_env_argv(*, socket: str, session: str) -> list[str]:
    """`set-environment`: the interpreter the hotkey runs charter with.

    `sys.executable`, delivered the same out-of-band way `_exit_path_env_argv` delivers
    `status_path` and for the same two reasons — a single argv value nothing re-parses,
    and a bind/action TEMPLATE that stays free of per-machine text. See `conf_text`'s
    own docstring for what a bare `charter` cost.

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

    `"$CHARTER_PY" -m charter ...` — the hotkey bind (`conf_text`) — is a shell TEMPLATE
    shared by every session on
    `SOCKET`, built once and never per-invocation; there is nowhere in it to splice a
    `-P` without re-embedding per-machine text, the exact construction `conf_text`'s own
    docstring already bans for `status_path`. `PYTHONSAFEPATH=1` is `-P`'s own
    environment-variable form — carried the identical session-scoped way
    `_charter_py_env_argv` carries `$CHARTER_PY` itself, so it reaches the same shell
    that call already proves reaches: without this, a pane whose cwd happens to be a
    charter checkout would have the hotkey palette import THAT tree the moment it opens,
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

    `cmd_palette` (the hotkey bind's own action) and every action it starts read
    `$CHARTER_SESSION_ID` back out of a `run-shell`-spawned process's environment — the same out-of-band shape `_exit_path_env_argv` already uses for
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
    unfixed, every frame after the first would open the FIRST frame's own palette and
    run its actions against the wrong session's state.

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
    `%<digits>` (`_PANE_ID_RE`), *slot* a name `frame_slots.drawable` resolves to a
    component — one of charter's own under either spelling, or one an installed
    distribution supplies, and never merely a name SHAPED like one: `top.` is as
    namespaced-looking as `acme.metrics` and is refused, because the property being
    checked is "charter can draw this", not "this has a dot in it" — *fid* a
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
    if (not _PANE_ID_RE.fullmatch(panel_pane) or not frame_slots.drawable(slot)
            or not _FRAME_ID_RE.fullmatch(fid)
            or not all(_action_word_is_safe(w) for w in words)
            or any(w.split() != [w] for w in words[1:])):
        return None
    action = f"""run-shell -b '"{words[0]}" {" ".join(words[1:])}'"""
    return tmuxctl.server_argv(socket, "set-hook", "-p", "-t", panel_pane, "pane-died",
                               action)


#: Which `resize-pane` flag re-asserts a slot's dimension, **for every slot
#: :func:`_reassert_sizes` asserts one for**: `-y` (rows) for the horizontal strips, `-x`
#: (columns) for the side column — the same axis `layout.py`'s own `-v`/`-h` split
#: direction already encodes for the same slots, read here rather than re-derived a third
#: way. The SIZE that goes with it is `layout.slot_sizes`', not `SLOT_SIZE`'s, since
#: `repos`' is a function of its content and of the window (#488).
#:
#: **`repos` is deliberately absent, and #515 is the whole of it.** The issue asked for
#: the second bottom pane to be in this map and that would be wrong — not because its
#: axis is in doubt (it is rows; its width is whatever the harness column leaves it, and
#: no `resize-pane -x` ever set that), but because tmux's `resize-pane` moves exactly ONE
#: boundary. In a vertical stack of N panes only N-1 heights can be asserted; assert them
#: all and the outcome depends on the order they are asserted in. Measured on tmux 3.7c
#: at 200x50, asserting `top`, `bottom` and `repos` in split order: the table came back
#: **1** row tall and the attention strip **6** — the two sizes swapped panes, and the
#: harness kept whatever tmux's own proportional redistribution had left it.
#:
#: So the pane left out is the one whose height is already a function of all the others
#: (`layout.VARIABLE_ROW_SLOTS`), the HARNESS is told its height explicitly
#: (`layout.harness_rows`), and the table lands on exactly `layout.repos_rows`' answer
#: without anything naming it. Putting `repos` back in this map is a mutation
#: `tests/test_frame_density.py::ResizeRecomputesForBothDimensions
#: ::test_the_fixed_strips_are_re_asserted_at_their_own_constant_height` turns red — it
#: asserts the whole set of `-y`s issued, not merely that each one present is right.
_RESIZE_FLAG = {"top": "-y", "bottom": "-y", "right": "-x"}

#: Every real pane id tmux ever reports, held in `tmuxctl` because a SECOND module
#: now interpolates one into text tmux re-parses (`frame/overlay.py`'s escape
#: hatch), and two copies of one guard is the drift this repo keeps paying for.
#: The reasoning — including why the class is `[0-9]` and not `\d` — moved with it.
_PANE_ID_RE = tmuxctl.PANE_ID_RE

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
    computed once when the frame was laid out. The table pane's height is not a constant:
    it is `min(content, cap)` where the cap is what the window can spare
    (`layout.repos_rows`), so a stale number is not merely imprecise, it is destructive.
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


def _spawn_gather(fid: str, ws: str) -> None:
    """Fill *fid*'s gather cache in a DETACHED child, and bump the frame when it lands.

    **The launcher empties the cache and nothing used to refill it — #512.** `cmd_launch`
    calls `gather.discard(fid)` before it draws anything (a recycled pid must not adopt a
    dead frame's rows; see that function), and the only other caller of `gather.refresh`
    in charter is `frame/notify.plane_changed`, reached from a `posttooluse*` hook. So a
    frame's repo table was filled by the operator's FIRST TOOL CALL and by nothing else —
    reported as "the repos appear after I resize", because a resize happens to coincide
    with one.

    **Detached rather than inline, and that is the whole design.** A cold `gather.scan()`
    is ~35ms and three git invocations per repo; the launch path is a person waiting for a
    harness to appear, and `charter claude` is the default way charter is started. So this
    is the shape charter already uses twice for exactly this problem — `update.maybe_spawn`
    and `glstate.maybe_spawn`: draw immediately with whatever is there, kick a child, let
    the answer arrive a beat later. `slots._repos` says "gathering" in the meantime rather
    than drawing an empty table (`slots._unknown_lines`), so the window this opens is
    visible and honest rather than a silent lie.

    **`--workspace` is passed, not left to the child, for `glstate.maybe_spawn`'s own
    reason**: the launcher resolves the workspace for the FRAME — from its own terminal,
    its own cwd, its own pointers — while the child would resolve it for ITSELF, and a
    detached process started with `start_new_session` is exactly as far from the
    operator's terminal as a panel is (#512 again: `session.terminal()` would answer for
    the child, not for the frame). A refresh keyed to a different workspace than the frame
    it is refreshing is the defect, not a stale value.

    **The child bumps, because a panel repaints on nothing else.** `panel.run` polls
    `state.version`; a cache written with no bump behind it would sit on disk unread until
    something unrelated moved the version — which on an idle session is the same "first
    tool call" wait this function exists to end. `notify.plane_changed`'s order is kept
    verbatim (refresh, THEN bump) and for its stated reason: a poller that saw the new
    version must never then read the old cache.

    **Inline as a last resort, never as the plan.** `util.detach_self` reports whether the
    spawn happened; when it did not there is no other filler on this path, and a pane
    saying "gathering" for the rest of the session would be a promise charter had already
    broken. Paying ~35ms once, on a launch that has just failed to fork, is the cheaper
    of the two — and it is the only path in this function that costs the operator anything.

    Never raises: a frame must launch whether or not its rows can be gathered.
    """
    try:
        if util.detach_self(["frame-gather", "--session", fid, "--workspace", ws]):
            return
    except Exception:
        pass
    try:
        gather.refresh(fid, workspace=ws)
        state.bump(fid)
    except Exception:
        return


def _launch_sizes(fid: str, slots: list[str], *,
                  window_cols: int, window_rows: int) -> dict[str, int]:
    """How big each of *slots* is split, in a *window_cols* x *window_rows* window —
    the launch-time half of `layout.slot_sizes`, shared by both launch paths so they
    cannot disagree.

    `repos` is the only slot whose answer is not a constant (#488, moved off `bottom` by
    #515): it is sized to the repo table it is about to draw, floored at one row and
    capped so the harness keeps `layout.HARNESS_MIN_ROWS`.
    `frame_slots.repos_rows_wanted` is the same function the RENDERER's own budget comes
    from, which is what stops a frame coming up with a pane taller than its content or a
    table cut off with nothing saying so.

    **The width is not decoration (#500).** The renderer draws NO table below
    `statusline._LEFT_W` (95) and at most `slots._TERSE_ROWS` of one at a `terse`
    density, and `minimal` is a level the F2 palette offers. Sizing from the repo count
    alone gave both of them a pane sized for a table the panel then refused to draw — up
    to fourteen blank rows taken off the harness. (Since #515 `layout.visible_slots`
    drops `repos` outright below `statusline._LEFT_W` rather than splitting a pane its
    renderer will not draw in, so the narrow case now costs no rows at all.)

    **And it is the PANE's columns, which are not always the window's.** *slots* is the
    split order and the split order is the geometry (`instance.frame_of` keeps an
    operator's own `[frame] slots` verbatim), so a `right` split before `repos` has
    already taken 23 columns off the pane `repos` is carved from — 97 in a 120-column
    window, measured on tmux 3.7c. `layout.repos_cols` is that arithmetic; handing
    `repos_rows_wanted` *window_cols* straight through was #500's own second half, and
    it put six blank rows into exactly the frame this function exists to size. Both
    measurements are keyword-only and named for the WINDOW here for that reason: this
    function's whole job is turning a window into panes, and the two names that reach it
    must not be mistakable for either a row count or a pane's own width.

    Affordable at launch by construction, and that is not incidental: `cmd_launch` calls
    `gather.discard(fid)` before it draws anything, so this reaches `gather.row_count`
    with no cache and gets the directory-listing answer — an `iterdir`, never the git
    sweep `gather.scan` would run. See `gather.row_count`'s own docstring for both paths.
    """
    return layout.slot_sizes(
        slots, window_rows=window_rows,
        content_rows=frame_slots.repos_rows_wanted(
            fid, pane_cols=layout.repos_cols(slots, window_cols=window_cols)))


def _drawable_slots(cols: int, rows: int, configured: list[str] | None = None) -> list[str]:
    """Which configured slots this frame will actually draw, at *cols* x *rows*.

    *configured* defaults to `config.FRAME["slots"]` — already the density-resolved list
    (`instance.frame_of` expands a declared `[frame] density` into it, so nothing here
    knows presets exist). `cmd_density` passes the list a level expands to instead, so a
    frame re-laid-out from the palette goes through exactly the same two filters a launch
    does rather than a second copy of them: below the size floors it drops the same slots
    in the same order, and a slot with no renderer is skipped for the same reason.

    `[frame] slots` can accept a slot (`instance.FRAME_SLOTS`, sized by
    `layout.SLOT_SIZE`) that nothing can draw (as `left`/`right` were until Task 3 (#385)
    gave them one, and as the next slot this frame grows will be on its first day). Left
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

    **"Has a renderer" is not "is one of charter's four" any more.** A name an installed
    distribution supplies is drawable too (`frame_slots.drawable`), which is what lets a
    placed provider survive as far as `panel_argvs` — this filter is where every one of
    them was dropped before a pane could be split for it. On a machine with no component
    providers installed the answer is exactly what it always was.
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

    `tui.TERMINAL_SIZE_VARS` go, and that is belt and braces rather than the fix itself:
    every pane (harness or panel) measures its OWN tty (`frame/slots.py`,
    `frame/panel.py`), so a stale value here cannot mislay anything charter draws. But
    this environment is inherited WHOLE by every process tmux starts for this frame — the
    harness's shell among them — and both variables describe the LAUNCHING terminal, not
    any pane the frame creates.

    Asked of `tui`, which is the module that READS ``$COLUMNS``, rather than spelled here
    a second time. Two copies of "which variables describe the launching terminal" is two
    answers to one question, and the second copy is the one the test harness could not
    ask: `tests/_envguard.py` scrubs before `charter.config` loads, and this module pulls
    `config` in at import (#544).

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
    for stale in (*tui.TERMINAL_SIZE_VARS, "TMUX", "TMUX_PANE"):
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


#: The style every rule in the frame is drawn in — both borders, on both servers.
#:
#: `dim` over the terminal's own default foreground, which is `statusline._boxed`'s own
#: `\033[2m` said in tmux's style language: charter's chrome is one shade of the
#: operator's own palette everywhere it is drawn, never a colour charter picked out of
#: the 256 and imposed on a theme it cannot see. A terminal too old to honour SGR 2
#: renders both borders in the plain default instead, which is still ONE colour — the
#: property here is sameness, and it survives the attribute being dropped.
_CHROME_STYLE = "fg=default,dim"

#: Every window option tmux consults to draw a pane border, pinned to charter's own
#: answer. #514: the frame's rules came out in two colours, and neither of them was
#: charter's choice — tmux's own `pane-active-border-style` default is `fg=green` while
#: `pane-border-style` is `default`, so a rule running past the active pane's corner
#: changed colour mid-line (measured on 3.7c: `\033[32m` for the 77 cells over the
#: harness, `\033[39m` for the 22 over the sidebar, in one horizontal rule).
#:
#: **Both styles, and that is the whole first half of the fix**: setting one leaves the
#: other at tmux's default, which is how the two came to disagree in the first place.
#:
#: The other three are the same defect wearing different options, and they bite on the
#: OPERATOR'S server where their own `.tmux.conf` is what charter would otherwise
#: inherit (measured, with a hostile config on a real 3.7c): `pane-border-indicators
#: arrows` puts `↑`/`←` glyphs on the active pane's borders and no others, so one rule
#: carries a glyph its neighbour does not; `pane-border-lines double`/`heavy`/`number`
#: redraws every rule in a different weight (`number` writes pane NUMBERS into them);
#: and `pane-border-status top` is the worst of the three — it turns every border into a
#: title bar carrying `#{pane_title}` (the machine's hostname, by default) AND adds a
#: border row above the topmost pane, a row `layout._BORDER_ROWS` never budgeted for.
#:
#: `pane-border-format` is deliberately NOT here: it is inert while `pane-border-status`
#: is `off`, and pinning a format nothing renders would be pinning a spelling rather than
#: the property.
_CHROME: tuple[tuple[str, str], ...] = (
    ("pane-border-style", _CHROME_STYLE),
    ("pane-active-border-style", _CHROME_STYLE),
    ("pane-border-indicators", "off"),
    ("pane-border-lines", "single"),
    ("pane-border-status", "off"),
)


def _chrome_argvs(*, socket: str, harness_pane: str) -> list[list[str]]:
    """`set-option -w`: charter's own answer for every option tmux draws a border from.

    **Charter owns the frame's chrome, and tmux draws all of it.** #514 named two
    candidate causes — tmux's own two-colour default, and a panel drawing its own box
    inside a pane tmux is already bordering. Only the first is real: no frame renderer
    draws a box, because `statusline._boxed` is the only thing in charter that does and
    `statusline.a_frame_owns_this_surface` suppresses the whole status line inside a
    frame (ADR 0019). So the fix is not to make a second drawer stop; it is to stop
    tmux drawing charter's chrome from an answer charter never gave.

    **WINDOW-scoped, targeting the harness pane, exactly as `_panel_remain_on_exit_argv`
    is** — `-w -t <a pane id>` resolves to that pane's window, which is charter's own on
    either server. Measured on 3.7c with a hostile global config in place: after this
    runs, charter's window reads its own five values back and the operator's own windows
    still resolve to theirs. Every one of these is a window option, so there is no
    session- or server-scoped form that could reach past charter's window even by
    mistake, and `-g` never appears here for the reason `_remain_on_exit_argv`'s
    docstring gives.

    **One place, both servers, and that is the point rather than a convenience.** The
    defect this closes is chrome styled in two places that could never agree; a fix that
    put the private server's answer in `conf_text` (which `_launch_in_operator_tmux` must
    never `source-file`) and the operator server's answer here would rebuild exactly that
    shape one layer down. `_split_panels` is the one funnel every panel pane charter
    creates comes out of — both launch paths and every density change — which is why
    `remain-on-exit` is already armed there, and it is where these go too.

    Five `set-option` calls rather than one config file, and the cost is measured: 22.2ms
    per launch on this machine (ten runs against a real 3.7c), alongside the roughly
    fifteen tmux calls a launch already makes. Nothing here runs on a panel's repaint —
    the idle tick this must not touch is `panel.py`'s one `stat`, and these are issued at
    launch and at a density change, never per paint.
    """
    return [tmuxctl.server_argv(socket, "set-option", "-w", "-t", harness_pane, name,
                                value)
            for name, value in _CHROME]


def _surface_argvs(*, socket: str, pane_id: str, chrome) -> list[list[str]]:
    """`set-option -p`: the pane surface `[frame] chrome` asked for, on ONE panel pane.

    **tmux paints the background; charter sets an option.** `window-style` and
    `window-active-style` are settable pane-scoped on 3.7c, and tmux fills the pane's
    whole rectangle from them — the cells no renderer wrote included, on resize, on
    reattach. Measured, three panes with styles on the two panels and not on the harness,
    and what tmux then put on an attached client's wire::

        HARNESS : b'...\\x1b(B\\x1b[m\\x1b[1;1HHARNESS'        <- no colour at all
        PANELA  : b'...\\x1b[K\\x1b[48;5;24m\\x1b[2BPANELA'    <- the ACTIVE style
        PANELB  : b'...\\x1b[K\\x1b[48;5;236m\\x1b[2BPANELB'   <- the INACTIVE style

    So nothing here is on the repaint path (constraint 3 is untouched because nothing new
    repaints), the fill has no width to get wrong and therefore cannot wrap a pane
    (constraint 5, #553), and the focused/unfocused split is drawn by tmux from its own
    pane focus — it needs no `focus-events` and works inside an operator's own tmux where
    charter writes no config at all.

    **PANE-scoped, and that is the whole of the harness boundary.** `-p -t <pane id>`
    reaches exactly the pane charter just split off for a panel; the harness pane is never
    an argument here, so ADR 0018's line — charter "never decides what a cursor or a
    colour means inside it" — holds by construction rather than by care. Read back rather
    than intended: `show -p -t <harness> -v window-style` answers `''`.

    **It survives a panel respawn**, which a renderer-side fill would not: `pane-died`
    respawns a dead panel into the SAME pane, and these are properties of the rectangle
    rather than of the process in it.

    *chrome* is the word from `[frame] chrome`, and `instance.chrome_options` is what
    turns it into styles — so what reaches tmux is charter's own constant whatever the
    word was. An unknown word yields no commands at all, which is `off`.

    All four of charter's panes, not only the two bars: two chrome-coloured panes beside
    two uncoloured ones is a frame that does not match itself.

    **`NO_COLOR` refuses this, and that is the half that is easy to miss.** The fill is
    tmux's paint, not charter's, so gating only the panels' own SGR would leave an
    operator who asked for no colour looking at a coloured frame — charter having asked
    somebody else to paint it. `NO_COLOR` means no colour on their screen caused by
    charter, whichever process puts the bytes there. Asked through `chrome.no_colour` so
    there is one reading of the variable and not a second one out here (#547).
    """
    if chrome_mod.no_colour():
        return []
    return [tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane_id, name, value)
            for name, value in instance.chrome_options(chrome)]


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


def _launch_in_operator_tmux(socket: str, session: str, *, fid: str, ws: str,
                             argv: list[str], h, v: tuple[int, int]) -> int | None:
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
    must not use (see `_session_id_env_argv`). What the palette would offer there is
    "Detach", which an operator already inside tmux has their own prefix key for, and the
    densities, which `[frame] density` sets. A key taken from every window on their server
    to reach that is a worse trade than no key — `frame/slots.py` drops the hotkey hint
    from the bottom panel to match.

    **And `frame/overlay.py`'s escape hatch is bound here no more than the hotkey is**,
    for the identical reason and at a higher cost worth stating rather than discovering.
    A root key table is server-wide with no per-window form, so `bind -n F12` here would
    take F12 from every window the operator has open — which is precisely the sentence
    above, unchanged. The cost is that the hatch's promise ("one key back to the harness
    from any state, even a wedged one") is a promise of charter's OWN server, and on this
    path the way out of a pane that has stopped answering is the operator's own prefix
    key, which charter has not taken and cannot take. `docs/frame.md`'s list of what this
    path costs says it in the operator's words; this is the reason it is on that list.

    Four things that ARE written are charter's own and reach nothing of theirs, because
    every one of them is scoped to a pane charter created or to the window charter
    created: `remain-on-exit` on the harness pane (`_remain_on_exit_argv`, PANE-scoped);
    `remain-on-exit` on the frame's own window, so a dead PANEL stays long enough for its
    hook to run (`_panel_remain_on_exit_argv`, WINDOW-scoped — measured to leave the
    operator's own windows at their own default); each panel pane's own `pane-died`
    respawn hook (`_arm_panel_respawn`, PANE-scoped, #408 — it refused here until the
    hook could name this server rather than charter's, and then never fired here until
    the window kept the corpse it has to fire from); and the frame's own pane-border
    chrome (`_chrome_argvs`, WINDOW-scoped, #514).

    **That fourth one is here BECAUSE this is somebody else's server, not in spite of
    it.** On charter's private server the borders came out in tmux's own two default
    colours; here they come out in whatever the operator's `.tmux.conf` says, which
    measurably includes a `pane-border-status top` that writes their hostname into every
    rule and costs the frame a row. Charter's window is charter's to draw, and pinning
    five window options on it is the same WINDOW-scoped move `_panel_remain_on_exit_argv`
    already makes — measured the same way, and leaving their own windows at their own
    values.

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
    # And the workspace this frame is FOR, for the same rewrite-don't-merge reason and a
    # sharper one of its own: a panel process cannot resolve it (#512 — see
    # `state.record_workspace`), so this launcher is the only thing that ever knows.
    state.record_workspace(fid, ws)
    state.bump(fid)
    # Kicked here, before a single tmux split — the child gathers while tmux is still
    # carving panes, so on an ordinary launch the cache is already on disk by the time
    # the first panel process paints. See `_spawn_gather`.
    _spawn_gather(fid, ws)

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
                         sizes=_launch_sizes(fid, slots, window_cols=cols,
                                             window_rows=rows))
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


def _measure_window(socket: str, harness_pane: str) -> tuple[int, int] | None:
    """The size of the WINDOW *harness_pane* is in, or ``None`` when tmux would not say.

    Never `os.get_terminal_size()` on this path: that measures the pane `charter` was
    typed in, which is a fraction of the window the frame gets the moment the operator
    has a split open — and `_drawable_slots` would then drop panels a full-width window
    has ample room for.

    **``None`` rather than a fallback, and :func:`_window_size` is where the fallback still
    lives.** A launcher that cannot measure the window still has to draw a frame, so it
    takes `_FALLBACK_SIZE` and gets on with it. A `window-resized` child is the opposite
    case: it exists only to apply a decision made from a measurement, and "charter could
    not read the window" is not a measurement — applying an 80x24 layout to a window that
    is very probably not 80x24 is the same destructive move as applying a stale one, with
    less excuse. Telling the two apart is what lets `cmd_resize` refuse (#501) while
    `cmd_launch` proceeds.
    """
    out = tmuxctl.run("measuring the frame's window",
                      tmuxctl.server_argv(socket, "display-message", "-p", "-t",
                                          harness_pane, _WINDOW_SIZE_FORMAT),
                      timeout=5, report=False)
    w, _, hgt = out.stdout.strip().partition(":")
    if out.returncode != 0 or not w.isdigit() or not hgt.isdigit():
        return None
    return int(w), int(hgt)


def _window_size(socket: str, harness_pane: str) -> tuple[int, int]:
    """:func:`_measure_window`, with `_FALLBACK_SIZE` for a window tmux would not report.

    The launch-shaped half: every caller here has already decided it is going to lay a
    frame out and needs a number to do it with. See :func:`_measure_window` for why the
    resize path takes the other one.
    """
    return _measure_window(socket, harness_pane) or _FALLBACK_SIZE


def _window_still(socket: str, harness_pane: str,
                  measured: tuple[int, int]) -> bool:
    """Is the window still the size *measured* said it was?

    **The whole of #501's fix, and it is a re-READ rather than a lock.** `window-resized`
    fires once per size change and each event starts its own backgrounded `charter
    frame-resize` (`_resize_hook_argv`). Nothing serialises them and `run-shell -b` gives
    no completion ordering, so during a drag several children are in flight at once, each
    having measured the window at a different instant — and a child that measured a
    TALLER window computes a taller `bottom`. Applied after the child that measured the
    final, shorter one, that hands a pane a size for a window which no longer exists;
    tmux does not refuse an over-large `-y`, it grants it and takes the difference out of
    the neighbour, and the neighbour is the agent's own session (measured on 3.7c:
    `resize-pane -y 40` in a 20-row window left the harness pane **1 row tall**).

    A lock file the newest child STEALS would give last-writer-wins, which is the correct
    semantics; a plain mutex would give first-writer-wins, which is the wrong end. This
    gets last-writer-wins without either, because the window itself is the record: the
    newest measurement is by definition the one that still matches, and every child
    holding an older one asks this and finds it does not. Cheaper than a lock, and there
    is no file to leave behind when a child is killed mid-drag.

    **It narrows the window rather than closing it, and that is the honest claim.** The
    size can still change between this answer and the `resize-pane` that follows it —
    what is gone is the case the whole issue is about, a child sitting on a measurement
    the window left milliseconds ago while a newer child has already applied the right
    one. What remains is self-correcting in the direction that was always safe: the
    change that beat this check fired its own `window-resized`, and that event's child
    measures the window as it now is.

    An unmeasurable window answers ``False`` — see :func:`_measure_window`. That is the
    same direction as a stale one, and for a stronger reason.
    """
    return _measure_window(socket, harness_pane) == measured


#: How long a `window-resized` child waits for the window to STOP moving before it will
#: add or remove a pane, rather than merely re-size the ones this frame already has.
#:
#: **Re-sizing out of order is self-correcting; re-splitting out of order is not** (#536,
#: quoting #501). The next resize event fixes a pane given the wrong height. A pane killed
#: and split back costs a new interpreter, a cold charter import and a first paint, and
#: spends one of the three lives `_arm_panel_respawn` gives it — so a drag dragged through
#: the width where the repo table stops fitting must not thrash panes in and out at every
#: step of it.
#:
#: **A trailing-edge wait, because there is no timer and no shared state to debounce
#: with.** `notify._last` is a module-level dict, which works there because one process
#: makes every call; every one of these children is a separate `run-shell -b` process, so
#: the only thing they share is the window itself. A child that wants to change the shape
#: therefore sleeps, then asks whether the window is still where it was
#: (:func:`_window_still`). The one whose size the drag actually ended on is the only one
#: that gets a yes.
#:
#: **The number is a floor with a measurement under it, not a taste.** It has to exceed the
#: time one re-layout takes, or a second crossing lands inside the first. Measured on tmux
#: 3.7c on this machine, the command list `_relayout` issues to grow a frame from two panes
#: to four — two `set -p pane-died`, two `split-window`s, the `window-resized` hook, the
#: window and pane measurements, four `resize-pane`s and a `select-pane` — took a **median
#: 72ms over 5 runs** (69–72). That is the tmux side only: `split-window` returns as soon
#: as the pane exists, so each new panel's own cold charter import and first paint (of the
#: order of #501's measured 20ms per child, and more for one that actually renders) lands
#: after it. 400ms clears the measured floor five times over and sits under the ~500ms at
#: which an operator reads a delay as the frame having missed the resize entirely.
_SETTLE_SECONDS = 0.4


def _window_settled(socket: str, harness_pane: str,
                    measured: tuple[int, int]) -> bool:
    """Has the window held the size *measured* for :data:`_SETTLE_SECONDS`?

    :func:`_window_still` with a wait in front of it, which is the whole mechanism — see
    :data:`_SETTLE_SECONDS` for why a wait and not a debounce, and `cmd_resize` for what is
    gated on it.

    The sleep is in a backgrounded child nothing waits on (`_resize_hook_argv`'s
    `run-shell -b`), so what it costs is one idle process per size change for the length of
    the wait, and only for the size changes that would actually move a pane. tmux's own
    command queue never blocks on it.
    """
    time.sleep(_SETTLE_SECONDS)
    return _window_still(socket, harness_pane, measured)


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
    # palette — `cmd_density`), and nothing else afterwards can say which tmux pane charter
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

    **The frame's own chrome is armed at the same point, for that same reason** (#514,
    `_chrome_argvs`). A pane border belongs to the window, not to the pane that was just
    split off, so every rule in the frame is drawn from one set of window options — and
    the only way for the two servers to agree about them is for one call site to set them
    on both.

    **And the pane SURFACE is set here too, one pane at a time** (`_surface_argvs`). It
    is the same funnel and the same argument, one scope down: `window-style` is a PANE
    option, so unlike the border it has to be set on each panel as it appears — which is
    also exactly why the harness pane never gets it. Both launch paths and every density
    change reach panels through this function, so a frame cannot come out surfaced on one
    server and bare on the other, and a pane added by a later `_relayout` is surfaced when
    it is created rather than by a second pass that could forget it.

    Reported but not fatal, like the splits themselves: a frame whose panels cannot be
    respawned is still a frame, and the harness pane's own `remain-on-exit` was armed
    separately and earlier (`_remain_on_exit_argv`), so the exit code does not ride on
    this.
    """
    tmuxctl.run("keeping the frame's own dead panes long enough to bring them back",
                _panel_remain_on_exit_argv(socket=socket, harness_pane=harness_pane),
                env=env)
    # And the chrome those panes will be bordered with, armed at the same moment and for
    # the same reason: this is the one funnel both launch paths and every density change
    # reach panels through, so a rule cannot come out styled one way on charter's own
    # server and another way on the operator's (#514, `_chrome_argvs`). Reported but not
    # fatal, like the splits themselves — a frame whose borders kept tmux's own colours
    # is a frame that looks wrong, not one that fails.
    for chrome in _chrome_argvs(socket=socket, harness_pane=harness_pane):
        tmuxctl.run("styling the frame's own rules", chrome, env=env)
    panel_cmds = layout.panel_argvs(slots=slots, session=fid, socket=socket,
                                    harness_pane=harness_pane, env=pane_env,
                                    sizes=sizes)
    # Zipped with `slots`, not just iterated: `_resize_hook_argv` needs to know WHICH slot
    # each successfully-created pane belongs to (for its size and its resize-pane flag),
    # and `panel_argvs` returns exactly one command per slot, in the same order (see its
    # own docstring).
    # The word, resolved once for the whole batch: `config.FRAME` is a plane's own file
    # and cannot change between two splits, and `instance.chrome_options` is what turns
    # it into styles — so a value charter does not know produces no commands at all,
    # which is `off`. `.get` rather than `[...]`: a frame relaunched by a charter that
    # predates this key has a resolved config without it.
    chrome = config.FRAME.get("chrome")
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
            # The pane surface, on the pane that was just created and on no other — the
            # one place charter has a panel's `%N` in hand. The harness pane is never an
            # argument (`_surface_argvs`), which is how ADR 0018's boundary holds by
            # construction. Not fatal, like the splits: a frame whose panes kept the
            # terminal's own background is a frame that looks plainer, not one that fails.
            for surface in _surface_argvs(socket=socket, pane_id=pane_id, chrome=chrome):
                tmuxctl.run("painting a panel's surface", surface, env=env)
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


#: What `cmd_launch` returns when the operator cancelled the picker. 130 is the shell's
#: own "ended by SIGINT" convention, which is what cancelling is — and it is deliberately
#: not 0: a script that ran `charter claude` and got 0 back would take a frame that was
#: never started for one that ran and exited cleanly.
_PICKER_CANCELLED = 130


def _picker_wanted(args, chosen: str | None) -> bool:
    """Should this launch stop and ask? #518's whole gating rule, in one place.

    **A non-interactive launch must never block, and that is a property here, not a
    promise.** `charter claude` runs from scripts and from other agents; a prompt waiting
    on a pipe is a hang, not a question. Two gates close that by construction:

    * `cmd_launch` has already returned through `bypass` for `--no-frame` and for a
      non-tty stdout, so nothing reaches here with its output redirected;
    * stdin is checked HERE, because the two can differ — `charter claude < /dev/null` on
      a real terminal has a tty for output and nothing to read from.

    * **`--workspace` and `$CHARTER_WORKSPACE` skip it outright.** They are the top two
      rungs of `workspace.resolve`'s precedence and they mean "this one, I have decided" —
      putting a picker in front of an answer already given would be the same silence #518
      complains about, wearing a prompt.

    * **`--pick` forces it**, for the launch where an operator wants to move and has a
      pointer saying otherwise.

    * Otherwise the question is whether **anything chose**: `workspace.chosen` is
      `resolve`'s ladder minus its built-in fallback, so ``None`` means every rung came
      back empty and the launch was about to answer `default` — a name nobody picked.
      That is the launch #518 is about, and it is a PROPERTY of the resolution rather than
      a spelling of `workspace.source`'s human-facing label.
    """
    if not (sys.stdin.isatty() and sys.stdout.isatty()):
        return False
    if getattr(args, "workspace", None) or os.environ.get("CHARTER_WORKSPACE"):
        return False
    if getattr(args, "pick", False):
        return True
    return chosen is None


def _choose_workspace(args) -> tuple[str, int | None, bool]:
    """``(workspace, exit code or None, whether the operator picked it)`` — #518.

    The launch's workspace, either resolved exactly as it always was or chosen at a
    prompt. A non-``None`` exit code means the launch is over and nothing was started.

    **Creating happens here and nowhere else.** `frame/picker.py` renders and reads; it
    returns a decision. That split is what makes "a cancelled picker creates nothing" a
    fact about the code — there is no create call inside the picker to reach — and it is
    why the confirmation (`[y/N]`, defaulting to no) can live next to the prompt while the
    side effect lives next to the launch.

    `workspace.ensure` is the validating creator (`charter workspace create` calls the
    same one), so a name that got past the picker's own `valid_name` and still cannot be
    made raises `ValueError` here and ends the launch with a message, rather than being
    carried into a frame for a workspace that does not exist.

    The clone count each row carries is `workspace.clones` — one directory listing per
    workspace, paid once, before tmux starts. It is what makes the list worth reading
    (#512: the repo table is empty until the first gather, so the count is the only thing
    on screen at pick time that distinguishes them), and it is on no repaint path, so the
    idle-cost property is untouched.
    """
    explicit = getattr(args, "workspace", None)
    chosen = workspace.chosen(explicit)
    if not _picker_wanted(args, chosen):
        return workspace.resolve(explicit), None, False

    current = chosen or config.DEFAULT_WORKSPACE

    def _read() -> str | None:
        # `EOFError` is a closed stdin and `KeyboardInterrupt` is the operator pressing
        # ^C at the prompt: both are "no answer", which the picker reads as cancel. Caught
        # at the seam rather than around the whole launch, so a ^C means "not this
        # workspace" and never leaves a half-built frame behind — nothing has been created
        # or started at this point in `cmd_launch`.
        try:
            return input()
        except (EOFError, KeyboardInterrupt):
            return None

    def _write(text: str) -> None:
        sys.stdout.write(text)
        sys.stdout.flush()

    got = picker.ask(
        picker.rows(switch.workspaces(), lambda n: len(workspace.clones(n))),
        current, read=_read, write=_write, name_ok=workspace.valid_name,
        width=tui.term_width())

    if got.action == picker.CANCEL:
        util.info("charter: nothing started.")
        return "", _PICKER_CANCELLED, False
    if got.action == picker.CREATE:
        try:
            wd = workspace.ensure(got.name)
        except ValueError as e:
            util.err(str(e))
            return "", 1, False
        workspace.scaffold(got.name)
        util.ok(f"Workspace '{contain.one_line(got.name)}' created → "
                f"{wd.relative_to(config.ROOT)}/")
    return got.name, None, True


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

    ws, rc, picked = _choose_workspace(args)
    if rc is not None:
        return rc
    fid = state.frame_id(ws, os.getpid())
    if picked:
        # **Only when the operator actually picked.** A launch that resolved silently
        # writes no pointer today and must keep writing none — starting to would move
        # every framed session's workspace on a path nobody asked anything on.
        #
        # `session_id=fid`, so the per-session pointer lands under the FRAME's id: inside
        # a frame the frame IS the charter session (ADR 0019), which is what makes the
        # choice reach the panels and the agent's own shell alike (`state.workspace_for`
        # rung 1). `set_active` also writes the pointer for the LAUNCHER's terminal, and
        # that is the half that answers #518's "must not answer a prompt every launch":
        # the next launch from this terminal finds `workspace.chosen` already answered
        # and never asks.
        #
        # `force=True` because `fid` is minted from the workspace and this process's pid,
        # so an earlier launcher with the same pid and the same workspace can have left a
        # lock under this very id (`cmd_launch` reaps exactly that case a few lines down).
        # Being refused by a dead frame's lock, on a name the operator just typed, is not
        # a refusal worth having.
        workspace.set_active(ws, session_id=fid, force=True)
        # **Picking IS the confirmation that locks, and #518 asks that this be decided and
        # SAID.** `set_active`'s contract is that confirming a workspace locks the session
        # to it, and the picker is a confirmation — the alternative would be a launch that
        # writes the pointer and leaves the lock off, which is a third behaviour for
        # `charter workspace use` to disagree with. What makes it liveable is that the
        # frame has its own way out: `F2 → workspace` overrides the lock and says so
        # (`frame/switch.py`), so the operator is not sent back to the shell for a choice
        # they just made at a prompt. Printed here rather than in the picker, because it
        # describes what the LAUNCH did with the answer, not what the answer was.
        util.info(f"Workspace '{contain.one_line(ws)}' — 🔒 locked for this frame's "
                  f"session. F2 → workspace changes it; `charter workspace unlock` "
                  f"releases it.")

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
        rc = _launch_in_operator_tmux(inside[0], inside[1], fid=fid, ws=ws, argv=argv,
                                      h=h, v=v)
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
    # And the workspace this frame is FOR — rewritten for the same reason, and the one
    # thing here no process inside the frame can work out for itself (#512; see
    # `state.record_workspace`).
    state.record_workspace(fid, ws)
    state.clear_respawn(fid)
    state.bump(fid)
    # Kicked before tmux is asked for anything, so the gather runs alongside the session
    # start rather than in front of it. See `_spawn_gather`.
    _spawn_gather(fid, ws)

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
                                   history_limit=frame["history_limit"], session=fid,
                                   toggles=instance.frame_toggles(frame)))
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
    # bind's action (`charter frame-palette`) and every action the palette starts resolve
    # `$CHARTER_SESSION_ID` from a `run-shell`-spawned process's environment, and without
    # this call a frame beyond the first sharing
    # `SOCKET` would silently resolve the FIRST frame's id instead of its own (see
    # `_session_id_env_argv`'s own docstring for what was verified by hand).
    sid_set = tmuxctl.run("carrying the frame id to its own palette",
                          _session_id_env_argv(socket=SOCKET, session=fid), env=env)
    if sid_set.returncode != 0:
        util.warn("charter frame: continuing without it — the palette may not find "
                  "this frame's own actions")

    # The second value the same mechanism carries: which interpreter runs charter when
    # the hotkey fires. Without it both fall back to a bare `charter`
    # on the tmux server's own `$PATH`, and `run-shell` reports the resulting 127 by
    # printing it INTO THE HARNESS PANE — see `conf_text`'s own docstring.
    py_set = tmuxctl.run("carrying charter's own interpreter to the palette",
                         _charter_py_env_argv(socket=SOCKET, session=fid), env=env)
    if py_set.returncode != 0:
        util.warn("charter frame: continuing without it — the palette may not open "
                  "on this frame")

    # The escape hatch's other half. `conf_text`'s bind carries no identity — it reads a
    # WINDOW option, and this is what puts this frame's own answer in it, so a key table
    # every frame on `SOCKET` shares still returns each presser to their OWN harness (see
    # `frame/overlay.py`). Armed the moment the harness pane exists and before any panel
    # is split, because a frame with panes and no way back to the harness is exactly the
    # state the hatch is for.
    hatch = overlay.arm_hatch_argv(SOCKET, harness=harness_pane)
    if hatch is None:
        util.warn("charter frame: tmux did not report this frame's harness as a pane id "
                  f"— the {overlay.HATCH_KEY} escape hatch will not be armed for it")
    else:
        armed_hatch = tmuxctl.run("arming the frame's escape hatch", hatch, env=env)
        if armed_hatch.returncode != 0:
            util.warn(f"charter frame: continuing without it — {overlay.HATCH_KEY} may "
                      "not return to the harness in this frame")

    # #390: the same "-m prepends the cwd to sys.path" hole `util.self_relaunch_argv`'s
    # `-P` closes for the panel argv below, closed here as `PYTHONSAFEPATH=1` because
    # the hotkey template above has no room for a per-invocation flag — see
    # `_charter_pythonsafepath_env_argv`'s own docstring.
    safepath_set = tmuxctl.run("carrying PYTHONSAFEPATH to the palette",
                               _charter_pythonsafepath_env_argv(socket=SOCKET, session=fid),
                               env=env)
    if safepath_set.returncode != 0:
        util.warn("charter frame: continuing without it — the palette may import "
                  "the wrong charter if this pane's directory has its own `charter/` "
                  "package")

    # Nothing is recorded for the hotkey to open. The menu was a TABLE on disk that every
    # density change and every switch had to rewrite or it went stale (`_rerecord_menu`,
    # deleted with it); the palette is built from live state each time it opens
    # (`frame/builtin_actions.build`), so there is no snapshot left to keep in step.

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
                sizes=_launch_sizes(fid, slots, window_cols=cols,
                                    window_rows=rows))
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


def _current_density(fid: str) -> str:
    """The density *fid* is at right now: its own recorded override, else the configured
    default. The same two-source order `frame/slots.verbosity` reads for verbosity, so the
    palette's mark and the panels' own content can never disagree about which level is
    on."""
    from . import instance
    return (instance.density_level(state.density(fid))
            or instance.density_level(config.FRAME["density"])
            or "normal")


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
      than a constant: `repos`' height depends on the window it is in — on its rows
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
    # The table pane's width is not the window's, and a re-layout is where the two come
    # apart (#500 round 3, #515). `want` is the LEVEL's slot list, which is the order a
    # fresh launch would split in; the panes this frame actually has are `keep`'s, and a
    # `right` already sitting beside the harness insets anything split off it afterwards.
    # `_drawable_slots` filtered `want` against the level's order and so believed the
    # table fits; asked with the order the panes are really in, it may not — and a pane
    # split too narrow for its table is the bordered rectangle #515 exists to stop.
    if "repos" in missing and not layout.repos_fits(list(keep) + missing,
                                                    window_cols=window_cols):
        missing = [s for s in missing if s != "repos"]
    if missing:
        # Split in `want`'s order, which is the density level's own — see
        # `instance.FRAME_DENSITY` for why that order is geometry rather than reading
        # order. Every split targets the harness pane, so a slot added to a frame that
        # already has the others lands exactly where a launch would have put it.
        #
        # `list(keep) + missing` and NOT `want`, for `repos`' width (#500): the panes
        # that survived this re-layout are wherever their own launch put them, and the
        # new ones are split after all of them. That concatenation is the order the panes
        # are actually in — which is what `layout.repos_cols` needs, and what `want`'s
        # order is not. A frame launched with `["right", "top", "bottom", "repos"]` and
        # then sent to the `full` density keeps its inset 97-column table pane, because
        # nothing was killed or re-split; sizing it from `want`'s own order would say 120
        # and hand it the seven-row pane this fix exists to stop.
        keep.update(_split_panels(
            socket, slots=missing, fid=fid, harness_pane=harness_pane, env=None,
            pane_env=pane_env,
            sizes=layout.slot_sizes(
                want, window_rows=window_rows,
                content_rows=frame_slots.repos_rows_wanted(
                    fid, pane_cols=layout.repos_cols(list(keep) + missing,
                                                      window_cols=window_cols)))))
        _arm_panel_respawn(socket, fid=fid,
                           panes={s: keep[s] for s in missing if s in keep}, env=None)

    _install_resize_hook(socket, harness_pane=harness_pane, panes=keep, v=v,
                         env=None, fid=fid, replacing=True)
    _reassert_sizes(socket, fid=fid, panes=keep, harness_pane=harness_pane,
                    window_cols=window_cols, window_rows=window_rows)
    # `split-window` makes each new pane the ACTIVE one, so without this the operator is
    # left typing into a panel. The same correction `cmd_launch` makes after its own
    # splits, for the same reason.
    tmuxctl.run("focusing the harness pane",
                tmuxctl.server_argv(socket, "select-pane", "-t", harness_pane))
    return keep


def _apply_sizes(socket: str, *, panes: dict[str, str], sizes: dict[str, int],
                 flag: str) -> None:
    """`resize-pane <flag>` every slot in *panes* that *sizes* has a number for and
    :data:`_RESIZE_FLAG` gives exactly *flag*.

    One loop, called twice by :func:`_reassert_sizes` — once for the columns and once for
    the rows — rather than one loop over both, because the two are separated by a
    MEASUREMENT that only the first one makes truthful (:func:`_variable_pane_cols`).

    `layout.VARIABLE_ROW_SLOTS` is not in :data:`_RESIZE_FLAG` at all, which is what leaves
    the table pane as the stack's dependent one — see that constant's own comment for the
    tmux measurement, and note that a second check for it here would be a guard no mutation
    could turn red, because this one already catches it.

    Every pane id is checked before it is used, even though both callers of
    :func:`_reassert_sizes` already checked theirs. `_panel_died_hook_argv`'s own rule
    ("every value that reaches the text is what decides, never where it came from") applies
    to a `-t` argument too, and #475 was exactly a builder that documented "safe because my
    caller checked" growing a second caller.

    `report=False`: a pane that has since died (the operator closed it, a panel crashed
    between the map being read and this running) makes `resize-pane` fail, and that is not
    an integration failure worth printing over the agent's own screen.
    """
    for slot, pane_id in panes.items():
        if _RESIZE_FLAG.get(slot) != flag or slot not in sizes:
            continue
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        tmuxctl.run(f"restoring the {slot} panel's size",
                    tmuxctl.server_argv(socket, "resize-pane", "-t", pane_id,
                                        flag, str(sizes[slot])),
                    report=False)


def _variable_pane_cols(socket: str, *, panes: dict[str, str], window_cols: int) -> int:
    """How wide the variable-row pane (`repos`) actually is — **asked, not derived** (#510).

    `layout.repos_cols` turns the WINDOW's width into the PANE's by walking the order those
    panes were split in and subtracting every side panel split before it. That derivation
    is correct, and it is a derivation where a measurement is available: the pane's id is
    right there in *panes*, and one `display-message -p -t <pane> '#{pane_width}'` asks the
    only authority there is. The derivation stays as the answer for a pane that cannot be
    asked, which keeps `layout.repos_cols` the launcher's answer too — at launch the pane
    does not exist yet, so nothing there can measure anything.

    **The two part company in exactly the ways #510 names, and both are silent.** The
    order comes out of `state.panes`, a JSON file in the frame's own state directory:
    `state.panes` validates the VALUES (`isinstance(v, str)`) and says nothing about the
    order, so a truncated write, a hand edit, or a charter that wrote a different shape all
    reach here as a plausible-looking map whose order is fiction. And a future re-layout
    that MOVED a surviving pane rather than killing and re-splitting it would change the
    geometry without changing the map. In both cases the derivation's failure is an
    over-tall pane re-asserted on every step of a drag with the rows taken off the harness
    — measured on tmux 3.7c at 110x40 in the `right`-first order: 15 rows granted, 1 drawn,
    and the harness left 22 where it should have had 36.

    **The order this is called in is the whole of why the answer is trustworthy, and it was
    measured rather than assumed.** tmux redistributes every pane proportionally on a window
    resize, so a sidebar mid-drag is not 22 columns wide and the pane beside it is not the
    width it is about to be. On tmux 3.7c, a 120x40 frame with `right` split first, grown to
    200x40: `right` came back **62** columns and the table pane read **137** — where the
    truth, one `resize-pane -x 22` later, is **177**. Measuring first would therefore have
    been WORSE than the derivation, not better. `_reassert_sizes` applies the columns before
    it calls this, and with that ordering tmux's own answer agrees with `repos_cols`
    exactly, at 60, 110, 200 and 300 columns. The measurement is the authority and the
    derivation is what has to agree with it.

    One extra `display-message` on the `window-resized` path, which already runs one
    (`_window_size`). That is the cost #510 filed itself over rather than assuming; it is
    one round trip on a path whose own docstring is explicit that everything it reads is
    cheap by construction, and it buys the difference between a geometry charter believes
    and a geometry tmux reports.
    """
    # `next` and not a loop with a `break` in it. There is exactly ONE variable-row slot
    # by construction — `layout.VARIABLE_ROW_SLOTS` is derived from charter's own six
    # components, and a `[[frame.component]]` this plane places is always a FIXED row
    # (`layout._is_fixed_row`'s second branch reads its edge, and the config boundary
    # refuses a table without a number) — so a loop here had two ways to leave it that
    # could never differ, which `tools/sweep.py` correctly reported as one line doing
    # nothing.
    pane_id = next((p for s, p in panes.items()
                    if layout._key(s) in layout.VARIABLE_ROW_SLOTS), "")
    # #475's rule, applied to the one value on this path that arrives off disk: `panes`
    # is JSON in the frame's own state directory and `state.panes` validates that a value
    # is a STRING, not that it is a pane. Nothing off disk becomes a `-t` until it has
    # tmux's own shape, whatever the argv it is going into.
    if _PANE_ID_RE.fullmatch(pane_id):
        out = tmuxctl.run("measuring the table panel's own pane",
                          tmuxctl.server_argv(socket, "display-message", "-p", "-t",
                                              pane_id, _PANE_WIDTH_FORMAT),
                          timeout=5, report=False)
        cols = out.stdout.strip()
        # Three separate ways not to have an answer, and the first is not covered by the
        # other two: `tmuxctl.run` folds a TIMEOUT into a return code (`tmuxctl.TIMED_OUT`)
        # and hands back whatever the killed process had already written, so a partial read
        # can be a failure whose stdout still parses as a number. A zero is the third: not
        # an answer tmux has for a live pane, and telling `repos_rows_wanted` the table has
        # no room at all would floor the pane for a reason that is a read failure rather
        # than a geometry.
        if out.returncode == 0 and cols.isdigit() and int(cols) > 0:
            return int(cols)
    return layout.repos_cols(list(panes), window_cols=window_cols)


def _reassert_sizes(socket: str, *, fid: str, panes: dict[str, str], harness_pane: str,
                    window_cols: int, window_rows: int) -> None:
    """Re-apply every pane in *panes* the size `layout.slot_sizes` says it should have in
    a *window_cols* x *window_rows* window.

    The one place a pane is told its size after it exists, shared by the two callers that
    need it and for two different reasons: :func:`_relayout`, because a `split-window -l`
    or a `kill-pane` makes tmux redistribute the SURVIVING panes proportionally (so the
    panes a density change merely kept are exactly the ones a `-l` on the new pane cannot
    fix); and :func:`cmd_resize`, because the whole window just changed size under all of
    them.

    **The sizes are recomputed here, never carried in.** `repos`' height depends on the
    window it is in (`layout.repos_rows`), so a caller passing a launch-time number
    would re-assert a height that was right for a window that no longer exists — measured
    on tmux 3.7c, asserting `-y 40` in a 20-row window leaves the harness 1 row.
    *window_rows* is therefore a measurement the CALLER just took, not a remembered one.

    **And *window_cols* alongside it (#500), for the same reason and a sharper one.** A
    resize changes the window's WIDTH too, and how tall `repos` should be depends on it:
    below `statusline._LEFT_W` the panel draws no table, so it wants one row. Dropping the
    width here — `cmd_resize` measured it and threw it away as `_cols` — is what made a
    narrowed terminal keep a table-sized pane it could no longer draw a table in, on every
    step of the drag rather than only at launch.

    **And the window's width is not the PANE's.** A frame whose `[frame] slots` names
    `right` before `repos` has a 97-column table pane in a 120-column window, and this is
    the site that re-applied the wrong height for it on every step of a drag — the
    launch-time defect's longer-lived half.

    **So the pane is ASKED how wide it is, and the columns are applied first so that the
    answer is true (#510).** `layout.repos_cols` derived the pane's width from the order
    *panes* is in, and that order is a JSON file's insertion order — right until it is not
    (a truncated write, a hand edit, a future re-layout that moves a pane instead of
    re-splitting it), at which point the derivation is silently fiction. tmux holds the
    real number. It cannot simply be asked for it, though: a window resize leaves every
    pane proportionally scaled until this function corrects it, so the table pane read
    **137** columns in the 200-column window where the truth is **177** (measured on 3.7c).
    Hence two passes with the measurement between them — the columns, then
    :func:`_variable_pane_cols`, then the rows. See that function for the full measurement
    and for what happens when the pane cannot be asked.

    Every pane id is checked before it is used, even though both callers already checked
    theirs — see :func:`_apply_sizes`, which is where that check now lives for both passes.

    **The HARNESS is told its height too, and the variable slot is left as the remainder
    — #515.** tmux's `resize-pane -y` moves exactly ONE boundary: the one below the pane,
    or the one above it when the pane is last in its stack. With two strips that always
    traded with the harness, so asserting both was enough and the harness never had to be
    named. Three strips have two of them BELOW the harness, and those two trade with each
    other: measured on tmux 3.7c at 200x50, asserting `top`, `bottom` and `repos` in split
    order left the table 1 row tall and the attention strip 6 — the two sizes swapped
    panes, and the harness kept whatever tmux's own redistribution had given it.

    So this asserts every slot :data:`_RESIZE_FLAG` names — which is every slot whose size
    is a CONSTANT, and deliberately not `layout.VARIABLE_ROW_SLOTS` — plus
    `layout.harness_rows` on the harness itself. The table's size is already a function of
    all the others, so it lands on exactly `repos_rows`' answer without being asserted. In
    a stack of N panes only N-1 heights are free; asserting all N is what made the result
    depend on the order.

    *harness_pane* is checked like every other id here and for the same reason: it is read
    off disk (`state.harness_pane`) by `cmd_resize`, which is exactly the shape #475 was.

    `report=False` throughout: a pane that has since died (the operator closed it, a panel
    crashed between the map being read and this running) makes `resize-pane` fail, and that
    is not an integration failure worth printing over the agent's own screen.
    """
    # `list(panes)`, and the ORDER in it is a fact rather than an artefact: both callers
    # hand a map whose insertion order is the order those panes were split in —
    # `_draw_panels` records a launch in split order, `_relayout` keeps survivors ahead of
    # new splits, and `state.panes` is JSON, which round-trips a dict's order. `column_sizes`
    # and `slot_sizes` both read it, and `_variable_pane_cols` reads it again for the
    # derivation it falls back to when the pane cannot be measured.
    order = list(panes)
    # Pass one: the COLUMNS. A side panel's width is a constant and depends on nothing
    # here, and it has to land before anything is measured — see :func:`_variable_pane_cols`
    # for the tmux measurement that makes this an order rather than a preference.
    _apply_sizes(socket, panes=panes, sizes=layout.column_sizes(order), flag="-x")
    sizes = layout.slot_sizes(
        order, window_rows=window_rows,
        content_rows=frame_slots.repos_rows_wanted(
            fid, pane_cols=_variable_pane_cols(socket, panes=panes,
                                               window_cols=window_cols)))
    # Pass two: the ROWS, and the harness below them.
    _apply_sizes(socket, panes=panes, sizes=sizes, flag="-y")
    if _PANE_ID_RE.fullmatch(harness_pane or ""):
        tmuxctl.run("restoring the harness pane's height",
                    tmuxctl.server_argv(
                        socket, "resize-pane", "-t", harness_pane, "-y",
                        str(layout.harness_rows(sizes, window_rows=window_rows))),
                    report=False)


def cmd_resize(args) -> int:
    """`charter frame-resize --frame <fid>` — re-apply this frame's pane sizes for the
    window's CURRENT size. Fired by the `window-resized` hook, never typed.

    **Why this is a command and not a string in a hook.** tmux's layout engine
    redistributes every pane proportionally on a resize, so the intended sizes have to be
    re-applied afterwards; until #488 the hook carried them as literal text, computed once
    at layout time. The table pane's height is content-and-window dependent now
    (`layout.repos_rows`), and a literal is not merely stale then — measured on tmux
    3.7c, a hook still asserting `-y 40` after the window shrank to 20 rows left the
    harness pane **1 row tall**. Recomputing needs charter, so the hook calls charter.

    **Always 0, and every refusal is a quiet no-op** — `cmd_respawn`'s reason exactly:
    this is a `run-shell -b` child, nothing reads its status, and a non-zero exit is what
    makes tmux print into the harness pane on the un-backgrounded path. There is no
    screen left to report on that is not the agent's own.

    Refusals, in order:

    * no frame on the argv and no `$CHARTER_SESSION_ID` — not fired by a frame at all;
    * a harness pane that is not tmux's own `%<digits>` — it arrives off disk, and
      `_measure_window` is about to target it;
    * no panes recorded — a frame whose panels all failed to draw has nothing to resize;
    * **a window tmux would not report a size for** (#501) — this used to take
      `_FALLBACK_SIZE` and re-assert an 80x24 layout over a window that is very probably
      not 80x24, which is the same destructive move as applying a stale measurement with
      less excuse. A launcher still falls back, because it has to draw SOMETHING; this has
      the option of doing nothing and letting the next resize try again;
    * **a window that has moved since this child measured it** (#501), checked immediately
      before the sizes are applied — see :func:`_window_still` for the race and for why a
      re-read gives last-writer-wins where a lock would give the wrong end of it.

    The frame comes off the argv first and the environment only as a fallback, matching
    `cmd_respawn`: on the operator's own server there IS no `$CHARTER_SESSION_ID` to
    read, because that variable is a session option charter does not write there.

    **A resize can now add and remove panes, not only re-size them (#536).** Which slots a
    frame had used to be decided once, at launch: `_drawable_slots` ran against the
    terminal the frame started in and nothing afterwards re-ran it except a density change.
    So a frame launched at 200 columns and dragged to 80 kept a sidebar the same frame
    would have refused to draw had it started there, and — since #515 gave the repo table
    its own pane — kept a `repos` pane that could only say `⋯ too narrow for the repo
    table`, honestly, because it could not be un-split. The other direction was worse: a
    frame launched at 80 and widened to 200 never gained the panes it was now big enough
    for, until the next launch or the next `F2`.

    So the drawable set is recomputed here and handed to `_apply_arrangement` — the ONE
    live re-layout path, the same one a density level and a component's toggle key go
    through, rather than a second answer to "what does this frame look like now". Two rules
    keep that from being destructive, and each is a measured hazard rather than caution:

    * **It waits for the window to stop** (:data:`_SETTLE_SECONDS`). Re-applying a size out
      of order is self-correcting on the next event; killing and re-splitting panes out of
      order is not, and a drag through the width where the table stops fitting would
      otherwise thrash panes in and out with a `charter panel` process started and killed
      at each step, spending respawn lives on nothing.
    * **It never drops the LAST pane.** `_relayout` with an empty want-list makes
      `_install_resize_hook` remove the `window-resized` hook itself (correctly — see there)
      and that is a one-way door on this path specifically: the hook is the only thing that
      would notice the terminal being widened again. Below half the size floors, where
      `layout.visible_slots` answers `[]`, this frame therefore keeps the panes it has,
      exactly as it did before #536. A density change reaching the same state is a
      keypress, and the operator who pressed it can press another.

    **What the operator chose is never quietly undone.** The recompute starts from
    :func:`_visible_now` — this frame's own arrangement minus its own hidden set — not from
    `config.FRAME["slots"]`, so a panel hidden with a toggle key or by a `minimal` density
    stays hidden across every resize. Only the SIZE filter is re-run.

    **Both dimensions are used, not just the rows (#500).** The window's WIDTH decides
    whether `repos` can draw its table at all (`statusline._LEFT_W`), so narrowing a
    terminal below 95 columns has to shrink the pane to the one-row strip it can actually
    fill. This measured the width and discarded it as `_cols`, which left a narrowed
    frame re-asserting a table-sized pane it drew one line into — on every step of the
    drag, so the harness stayed pinned at `layout.HARNESS_MIN_ROWS` for as long as the
    terminal stayed narrow.

    **This is the hot path of the whole feature.** A terminal drag fires `window-resized`
    once per size change, so this runs repeatedly and in the background while the
    operator is still dragging. Everything it reads is cheap by construction:
    `state.harness_pane`/`state.panes` are two small files, `frame_slots.repos_rows_wanted`
    goes through `gather.row_count`, which answers from the frame's cache and never runs a
    git sweep (see its own docstring) — and at a width below `_LEFT_W` it does not even
    ask, since the answer is one row whatever the count is.

    **Three `display-message` calls now where there was one**, and the cost is stated
    rather than waved at: the window before, the pane in the middle (#510), the window
    again before anything is applied (#501). Measured on this machine against a real tmux
    3.7c, one round trip is **~5ms** — dominated by spawning the `tmux` CLIENT process, not
    by the server — so this child grows from #501's measured median of 20ms to roughly
    35ms. That is a real 75%, on a path that runs once per size change during a drag, and
    it buys the two things the extra reads are: a geometry tmux reports instead of one
    charter believes, and a refusal to apply a measurement the window has already left. The
    expensive addition is the settle wait, and it is paid only by a resize that would
    change the frame's SHAPE.
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
    measured = _measure_window(socket, harness_pane)
    if measured is None:
        return 0
    cols, rows = measured
    # Read ONCE and used twice — to decide whether the shape has to change, and as what
    # the re-layout is then given. A second `_visible_now` call for the second question
    # would be a second answer to "what is this frame showing", and it would be the wrong
    # kind of safe: a `want` computed from the config while `_apply_arrangement` got this
    # would still lay the frame out correctly, so nothing would go red — it would only
    # spend a settle wait and a version bump on a resize that had nothing to do.
    visible = _visible_now(fid, config.FRAME)
    # The SHAPE first, because a re-layout re-asserts every size on its way out
    # (`_relayout`) and doing both would size the panes twice — once for the set this
    # frame is about to stop having.
    want = _drawable_slots(cols, rows, visible)
    if want and set(want) != set(panes) and _window_settled(socket, harness_pane, measured):
        where = _relayout_target(fid)
        if where is not None:
            _apply_arrangement(fid, where=where, want=visible, window=measured)
            return 0
    if _window_still(socket, harness_pane, measured):
        _reassert_sizes(socket, fid=fid, panes=panes, harness_pane=harness_pane,
                        window_cols=cols, window_rows=rows)
    return 0


def cmd_gather(args) -> int:
    """`charter frame-gather --session <fid> --workspace <ws>` — gather one frame's repo
    state into its cache and bump it. Internal: fired detached by `_spawn_gather` at
    launch, never typed.

    **Its own command rather than a thread in the launcher**, for the reason
    `update.maybe_spawn` and `glstate.maybe_spawn` are both separate processes: the work
    has to outlive the call that started it *and* not be on its stack. A launcher blocks
    in `attach`/`_wait_for_harness` for the whole life of the frame on one path, and a
    background thread there would be a git sweep running inside the process the operator's
    terminal is currently handed to.

    **`--workspace` is required and never inferred here.** A detached child started with
    `start_new_session` has no terminal of the operator's, and `$CHARTER_SESSION_ID` names
    the FRAME rather than any agent session — the two rungs `workspace.resolve` would
    otherwise land on (#512). The launcher resolved this frame's workspace already; asking
    a second process to guess at it is the whole bug.

    **The order is `notify.plane_changed`'s, verbatim: refresh, then bump.** A panel's poll
    reads `state.version` first and the cache second, so bumping first opens a window where
    a poller sees the new version and still reads the old cache. Refreshing first closes it.

    **Always 0, and every refusal is a quiet no-op** — the same posture `cmd_respawn` and
    `cmd_resize` keep, for the same reason: nothing reads this process's status, and the
    only screen it could complain to is the agent's own. `gather.refresh` and `state.bump`
    each already promise not to raise; the `try` is here so that a future change to either
    — or to anything this grows between them — cannot turn a background gather into a
    traceback nobody will ever read.
    """
    fid = getattr(args, "session", None) or ""
    ws = getattr(args, "workspace", None) or ""
    if not fid or not ws:
        return 0
    try:
        gather.refresh(fid, workspace=ws)
        state.bump(fid)
    except Exception:
        return 0
    return 0


def cmd_density(args) -> int:
    """`charter frame-density <level>` — re-lay-out THIS frame, and write nothing else.

    Started by a palette row (`frame/builtin_actions.py`), and typeable by hand from
    inside a frame. The frame is resolved from `$CHARTER_SESSION_ID` exactly as
    `cmd_palette` and `cmd_respawn` resolve theirs — never from anything baked into an
    action, since one bind is shared by every frame on `SOCKET`.

    **charter.toml is not touched, and that is the whole design.** `[frame] density` sets
    what a frame STARTS at; this changes what one running frame IS. Charter's rule is that
    machine-written config belongs somewhere a machine may rewrite whole, and a
    hand-maintained, committed file that carries an operator's comments is the opposite of
    that — so the override goes in the frame's own state directory (`state.record_density`),
    which `state.reap` deletes entire when the frame ends. Relaunch, and the configured
    default is back.

    **Always 0, and every refusal is a quiet no-op**, for `cmd_respawn`'s reason: this
    normally runs detached from a palette row, where the only screen left to report on is
    the agent's own — the one rectangle ADR 0018 says charter never draws in. The palette
    says the refusal BEFORE the keypress instead: `builtin_actions._laid_out` asks the
    same question this function's third refusal asks, and the row carries the answer.

    **A level is a NAME for one arrangement of visibility, not a mechanism of its own**
    (Phase 2, Task 5; see `instance.FRAME_DENSITY`). This writes a hidden SET and then
    hands it to `_apply_arrangement`, which is the identical path a single component's
    toggle key takes — so pressing `minimal` and then a component's own key are two edits
    to one thing rather than two things arguing. The one axis a level still owns by itself
    is ``verbosity``, which is how much each panel SAYS: no per-component key can express
    it, which is exactly why the level is still recorded.

    Refusals, in order:

    * no `$CHARTER_SESSION_ID` — not fired from inside a frame at all;
    * a *level* outside `instance.FRAME_DENSITY` — a closed set of three constants charter
      wrote itself, so this can only be a hand-typed argument;
    * anything `_relayout_target` refuses — a harness pane that is not tmux's own
      `%<digits>`, or a tmux whose version charter could not read. Shared with
      `cmd_toggle` rather than spelled twice; see that function.

    The density and the hidden set are recorded BEFORE the panes move, so a re-layout that
    fails halfway still leaves the panels that survive drawing at the density the operator
    asked for. The version is bumped afterwards (`_apply_arrangement`): that is what makes
    every surviving panel repaint with the new verbosity, and a bump before the layout
    settled would repaint into the old shape.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    level = instance.density_level(getattr(args, "level", None))
    if not fid or level is None:
        return 0
    where = _relayout_target(fid)
    if where is None:
        return 0
    state.record_density(fid, level)
    # The whole of "density is a named arrangement over visibility". A level is turned
    # into a HIDDEN SET over this frame's own arrangement and then handed to the same
    # `_apply_arrangement` a single component's toggle key uses — it does not drive a
    # layout of its own. Two consequences, and both are the point:
    #
    # * a toggle afterwards composes with the level instead of fighting it, because they
    #   are edits to one set rather than two ideas about what the frame is;
    # * the SPLIT ORDER is the plane's own, not the level's. `instance.FRAME_DENSITY`'s
    #   lists are in charter's shipped order because they had to name one; an operator's
    #   `[frame] slots` order is a promise `instance.frame_of` keeps verbatim, and it is
    #   the order this frame's panes are actually in. A level names a SET of components;
    #   the arrangement is what says where they go.
    arrangement = instance.frame_arrangement(config.FRAME)
    want = instance.density_slots(level)
    state.record_hidden(fid, [n for n in arrangement if n not in want])
    _apply_arrangement(fid, where=where, want=[n for n in arrangement if n in want])
    return 0


def _relayout_target(fid: str):
    """Where a live re-layout of *fid* would act — ``(socket, harness pane, version)`` —
    or ``None`` if this frame cannot be re-laid-out at all.

    The refusals :func:`cmd_density` and :func:`cmd_toggle` share, held here so the two
    keypresses cannot come to disagree about what a frame has to have before its panes may
    be moved. Each is a quiet no-op for `cmd_respawn`'s reason: both callers normally run
    as a `run-shell` child, where the only screen left to report on is the agent's own —
    the one rectangle ADR 0018 says charter never draws in.

    * A harness pane that is not tmux's own `%<digits>`. It comes from
      `state.harness_pane` — the record ADR 0019's own `is_live` reads, never a second
      copy — which means it arrives off DISK rather than off `split-window`'s stdout, and
      it is about to be interpolated into a hook action and used as a split target.
      `_PANE_ID_RE` is #475's rule applied to it. A frame launched by a charter that
      predates `state.record_harness_pane`, or a corrupted file, has nothing to split off,
      and guessing at a pane id is the one thing `frame/layout.py`'s module docstring
      measures the cost of. (An empty PANEL map is not a refusal: a frame whose panels all
      failed to draw can still be given some.)
    * A tmux whose version charter could not read. Every builder below takes it.
    """
    harness_pane = state.harness_pane(fid) or ""
    if not _PANE_ID_RE.fullmatch(harness_pane):
        return None
    v = tmuxctl.version()
    if v is None:
        return None
    return (state.frame_server(fid) or SOCKET), harness_pane, v


def _apply_arrangement(fid: str, *, where, want: list[str],
                       window: tuple[int, int] | None = None) -> None:
    """Make *fid*'s panes match *want*, and let every surviving panel know.

    **The one live re-layout path**, and having exactly one is Task 5's actual
    requirement: a density level and a component's toggle key are two ways of deciding
    which components are visible, and a second resize path for the second of them would be
    two answers to "what does this frame look like now" — the shape #500 and #547 both
    cost. So a caller decides visibility and this does the rest, which is measurement,
    the same two filters a LAUNCH goes through (`_drawable_slots`), the same `_relayout`,
    and the same record-and-bump.

    The version is bumped LAST: that is what makes every surviving panel repaint, and a
    bump before the layout settled would repaint into the old shape. The caller records
    its decision BEFORE calling here for the mirror-image reason — a re-layout that fails
    halfway still leaves the panels that survive drawing the arrangement that was asked
    for.

    *window* is a measurement the caller has ALREADY taken and already checked, and only
    `cmd_resize` has one (#501): it measured the window, waited for it to stop moving and
    re-read it, so a third reading here would be a fresh chance to disagree with the one
    the decision was made from — the exact shape the settle exists to close. The two
    keypress callers pass nothing and this measures for them, as it always has: they act on
    a key rather than on a measurement, so there is no earlier reading for this one to
    contradict.
    """
    socket, harness_pane, v = where
    cols, rows = window if window is not None else _window_size(socket, harness_pane)
    panes = _relayout(socket, fid=fid, harness_pane=harness_pane,
                      panels=state.panes(fid),
                      want=_drawable_slots(cols, rows, want), v=v,
                      window_cols=cols, window_rows=rows)
    state.record_panes(fid, panels=panes)
    # Nothing to re-record. The palette is built from live state every time it opens
    # (`frame/builtin_actions.build`), so the mark on the density row moves with the frame
    # by construction — where the menu was a SNAPSHOT on disk that a switch had to rewrite
    # or it went on naming the level the frame had left.
    state.bump(fid)


def _hidden_now(fid: str, frame: dict) -> list[str]:
    """Which components *fid* is not drawing right now.

    Two sources, in the one order that makes "for the running frame only" true: the
    frame's OWN recorded set (`state.hidden`, written by a toggle key or a density level
    and by nothing else) first, and the config behind it. This is
    `frame/slots.py:verbosity`'s shape, said about visibility instead of about how much a
    panel says.

    ``None`` and not "empty" is what separates the two, which is why `state.record_hidden`
    goes to the trouble of telling those apart: a frame whose operator has toggled the
    last hidden panel back ON has an empty recorded set, and falling back to the config
    there would put the panel straight back and make the key look broken.

    **The config's answer is the arrangement MINUS the visible list, and reading only
    ``visible = false`` instead would be wrong on the commonest plane there is.**
    `instance.frame_arrangement` is deliberately longer than what a frame draws: it
    appends charter's own built-ins the plane never named, so that a density level can
    still reach them, as levels always have. A plane whose `[frame] slots` is
    ``["top", "bottom"]`` therefore has `repos` and `sidebar` in its universe and neither
    on screen — and taken as "not hidden", the very first keypress would have conjured
    both. `frame["slots"]` is the VISIBLE list in either spelling (`instance.frame_of`
    resolves `visible = false` into it), so subtracting it answers for both at once.
    """
    recorded = state.hidden(fid)
    if recorded is not None:
        return list(recorded)
    visible = set(frame.get("slots") or ())
    return [n for n in instance.frame_arrangement(frame) if n not in visible]


def _visible_now(fid: str, frame: dict) -> list[str]:
    """Which components *fid* IS drawing right now, in the arrangement's own order — the
    complement of :func:`_hidden_now` over `instance.frame_arrangement`, and the same
    expression `cmd_toggle` and `cmd_density` both end on.

    **Size does not come into it**, and that separation is the point: this answers what the
    OPERATOR has asked for (a density level, a toggle key, or the committed `[frame]
    slots` when neither has been used), and `_drawable_slots` is what then decides which of
    them a terminal of a given size can actually hold. `cmd_resize` needs exactly this
    split — it must recompute the second without ever quietly undoing the first, and
    recomputing from `config.FRAME["slots"]` instead would put a hidden panel back on
    screen on the operator's next terminal drag.
    """
    hidden = set(_hidden_now(fid, frame))
    return [n for n in instance.frame_arrangement(frame) if n not in hidden]


def cmd_toggle(args) -> int:
    """`charter frame-toggle <component>` — show or hide ONE component, live.

    Fired by that component's own `bind -n` (`conf_text`), and typeable by hand from
    inside a frame. The frame is resolved from `$CHARTER_SESSION_ID` exactly as
    `cmd_density`, `cmd_palette` and `cmd_respawn` resolve theirs, since one bind text is
    shared by every frame on `SOCKET`.

    **charter.toml is not touched**, and every word of `cmd_density`'s argument for that
    applies unchanged: `[[frame.component]]`'s `visible` says what a frame STARTS at, this
    changes what one running frame IS, and the override lives in the frame's own state
    directory (`state.record_hidden`) which `state.reap` deletes entire when the frame
    ends. Relaunch and the arrangement the operator committed is back.

    **The name is refused unless this frame's own arrangement contains it**, and that is
    the guard that matters here rather than a shape check on the string. A name reaching
    this far is about to become a `split-window`'s `charter panel <name>` argv and a
    respawn hook's tmux CONFIG TEXT (`_arm_panel_respawn`), which is the `[frame] hotkey`
    class of surface — and "is it in the arrangement" is a stronger answer than any
    alphabet, because it admits only names that were already resolved by
    `instance.component_tables` and already drawn. It is also the honest answer to the
    ordinary case: `charter frame-toggle repos` on a plane whose arrangement has no repo
    table is not a security question, it is a name this frame has nothing to toggle.

    **Always 0, and every refusal is a quiet no-op**, for `cmd_density`'s reason: this
    normally runs as a `run-shell` child of a keypress, where the only screen left to
    report on is the agent's own — the one rectangle ADR 0018 says charter never draws in.

    **There is deliberately NO "not inside a frame" refusal here, and the deletion sweep is
    why.** An `if not fid: return 0` stood at the top of this function and was found to be
    exactly equivalent — deleted, the FULL suite stayed green (6092 tests), and every
    observable was identical: same return code, no tmux command issued, no file written,
    nothing readable back out of the state directory. `$CHARTER_SESSION_ID` unset means
    ``fid == ""``; `contain.child` refuses ``""`` as a path segment, so `state.frame_dir`
    answers ``None`` and `state.harness_pane` with it, and :func:`_relayout_target`'s
    `_PANE_ID_RE` check — which IS pinned, and goes red when deleted — is what stops the
    empty string ever reaching a `-t`. Two guards in sequence, one of them free.

    It was deleted rather than left, and rather than papered over with a test that would
    have had to assert an implementation detail to see it: this command emits nothing by
    construction, so there is no *reason* for a refusal test to distinguish, only a
    consequence — and the consequence is already another line's. A guard nothing can pin
    is a comment with a runtime cost. `test_outside_a_frame_it_does_nothing` pins the
    property that survives it, and names the line that carries it.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    name = getattr(args, "component", None)
    frame = config.FRAME
    arrangement = instance.frame_arrangement(frame)
    if name not in arrangement:
        return 0
    where = _relayout_target(fid)
    if where is None:
        return 0
    hidden = set(_hidden_now(fid, frame))
    # `symmetric_difference_update` and not two branches: a toggle is one bit flipped, and
    # written as `if name in hidden: discard else: add` it is two statements that have to
    # agree about which name they are talking about.
    hidden.symmetric_difference_update({name})
    # Written back in the arrangement's own order rather than the set's, so the file a
    # human may end up reading reads like the frame does. Filtered by the arrangement too,
    # which is what keeps a name that has since left the operator's config from living in
    # this file for the rest of the frame's life.
    state.record_hidden(fid, [n for n in arrangement if n in hidden])
    _apply_arrangement(fid, where=where,
                       want=[n for n in arrangement if n not in hidden])
    return 0


#: How long the palette waits, at SHUTDOWN, for an action's ``run`` to have returned.
#:
#: **Not a wait for the work — a wait for the START.** §4g's fire-and-report says ``run``
#: starts something and returns, and `Invocation.join`'s own docstring names "a shutdown
#: that wants to know whether anything is still going" as one of its two legitimate
#: callers. This is that shutdown: the palette's pane is about to be killed, and
#: `kill-pane` hands SIGHUP to that pane's process group, which takes the action's worker
#: thread with it. Closing without this would race a `Popen` that has not happened yet.
#:
#: Two seconds because a conforming ``run`` returns in single-digit milliseconds and an
#: action still inside one after two seconds has broken the contract — at which point the
#: palette closes anyway rather than holding a pane open on a hung action, which is the
#: escape hatch's own argument applied one layer up.
_ACTION_START_GRACE = 2.0


def cmd_palette(args) -> int:
    """`F2`: open this frame's palette — or, with ``--pane``, BE it.

    Two modes and one subcommand, because they are two halves of one keypress and
    splitting them into two spellings would be two things to keep in step. Without
    ``--pane`` this runs as the hotkey bind's `run-shell` child and does nothing but carve
    the overlay's pane off the harness; with it, this IS the program inside that pane.

    **The bind carries the presser's client and nothing else** (see `conf_text`). The
    frame is resolved from `$CHARTER_SESSION_ID` at the moment the key fires, exactly as
    `cmd_density`, `cmd_switch` and `cmd_respawn` resolve theirs, because one bind is
    shared by every frame on `SOCKET`.

    **Always 0.** `run-shell` reports a non-zero command by printing it INTO THE HARNESS
    PANE and dropping that pane into copy-mode — charter drawing in the one rectangle ADR
    0018 says it never draws — so every refusal here is a quiet no-op, exactly like
    `cmd_density`'s.

    **Pressing the hotkey while the palette is already open opens a second one.** A
    `bind -n` is the ROOT key table, so tmux matches `F2` before any byte reaches the
    palette's pane — which is measured only for a key tmux's own table claims, and is the
    same property that makes the escape hatch work against a wedged overlay. The second
    palette is the modal one and Escape closes it; the first is left as an ordinary
    pane, closable by selecting it and pressing Escape, or by the hatch. Not guarded
    here on purpose: every cheap test for "is one
    already open" is a stale-state problem of its own (the hatch deliberately leaves the
    window option naming a pane that is gone), and a guard that refuses the palette after
    an escape-hatch press would be worse than the state it is preventing.
    """
    if getattr(args, "pane", False):
        return _draw_palette(args)
    return _open_palette(args)


def _open_palette(args) -> int:
    """Carve the overlay's pane off the harness and make it the surface.

    `frame/overlay.py` owns every one of these argvs — the split, the hatch, the focus and
    the zoom — and their ORDER is its property, not this function's: the hatch is armed
    before the pane can capture anything, so a surface that wedges on its first paint still
    has a way out.

    The pane is told this frame's own identity through `_relayout_pane_env`, for the reason
    that function's docstring measures: this process is a `run-shell` child of a tmux server
    shared between every frame on the machine, so its `$CHARTER_ROOT` and
    `$CHARTER_WORKSPACE` may be another frame's, and a palette listing another plane's
    workspaces would offer to switch to them.

    A tmux charter cannot get a version out of is a quiet no-op rather than a traceback in
    the harness pane: `_relayout_pane_env` needs the version to decide whether `-e` can be
    parsed at all, and there is nothing to open a palette against either way.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    harness = state.harness_pane(fid) or ""
    socket = state.frame_server(fid) or SOCKET
    v = tmuxctl.version()
    if v is None:
        return 0
    argv = overlay.open_argv(
        socket, harness=harness,
        command=util.self_relaunch_argv("frame-palette", getattr(args, "client", "") or "",
                                        "--pane"),
        env=_relayout_pane_env(fid, v))
    if argv is None:
        return 0
    opened = tmuxctl.run("opening the palette", argv)
    for cmd in overlay.modal_argvs(socket, harness=harness,
                                   overlay_pane=opened.stdout.strip()):
        tmuxctl.run("making the palette the surface", cmd)
    return 0


def _draw_palette(args) -> int:
    """Be the palette: draw the rows, take a choice, act on it, hand the pane back.

    **The close is a ``finally``**, for `overlay.Surface.run`'s reason one layer down: a
    palette that raised must still give the operator their harness back. Whatever went
    wrong is one traceback into a pane that is about to stop existing, which is the one
    place charter may print it.

    **The registry is built here, not carried.** `builtin_actions.build` resolves the
    density mark and every installed provider's actions against the moment the palette
    opened — the menu's staleness (`_rerecord_menu`, deleted with it) was a snapshot on
    disk that every other command had to remember to rewrite. `choose.open_rows` resolves
    the workspace and persona the frame is on at the same moment, for the same reason.

    **Two kinds of row, and one of them is a doorway.** A picker row replaces the surface
    in this same pane (`palette.own_the_tty`'s *then*) rather than starting anything, and
    :func:`_picker` is what turns one into the next surface; everything else is an action
    and goes through `invoke`. They are told apart by `choose.noun_of`, on ids a provider's
    action cannot spell — see `frame/choose.py`.

    **A third kind arrives only if the operator types**, and the catalogue is deliberately
    not where it lives. `query_only` hands the surface :func:`_name_rows`, which is called
    the first time the query is non-empty and never while it is empty — so `F2` on a plane
    with forty workspaces still costs what it cost when the doorways were the only way to
    a name, and `F2` then `beta` then Enter switches without the doorway's Enter in
    between. The rows that come back are the picker's own, so `_chosen_name` maps one back
    to a name without knowing which route drew it.

    **Every outcome is said on the operator's own screen, and there is exactly one call
    that says it.** Three things can need a sentence and none of them has anywhere else to
    go, because the pane they would have been drawn in is the one this is about to kill:
    a picker row refused before it opens (the launch pin), a switch refused after a name is
    chosen (an unknown name, a name that stopped existing while the picker was up), and a
    switch that took effect over a session lock and must name what it overrode (#517 —
    "a menu that silently fails against a lock is worse than no menu"). One call rather
    than a branch per case, because a branch per case is a case somebody adds without one.

    A STARTED action still says nothing: what it started surfaces through `inflight`,
    which is the frame's existing spinner and not a second clock.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    socket = state.frame_server(fid) or SOCKET
    harness = state.harness_pane(fid) or ""
    reg = builtin_actions.build(fid, current_density=_current_density(fid))
    snapshot = gather.cached(fid) or {}
    client = getattr(args, "client", "")
    opened: list[choose.Roster] = []
    try:
        surface = palette.Palette(
            catalogue=(choose.open_rows(fid)
                       + palette.rows(reg.offers(fid=fid, snapshot=snapshot))),
            query_only=lambda: _name_rows(fid, opened),
            mouse=True)
        chosen = palette.own_the_tty(
            surface, then=lambda row: _picker(row, fid, opened))
        if chosen is None:
            return 0
        picked = _chosen_name(chosen, opened)
        if picked is not None:
            noun, name = picked
            out = choose.switch_to(noun, fid, name)
            _say_on_screen(fid, out.message, client)
            return 0
        if choose.noun_of(chosen) is not None:
            # A picker row that never opened its picker: `_picker` refused it, which it
            # does for exactly one reason and always with that reason in the row's note.
            _say_on_screen(fid, chosen.note, client)
            return 0
        inv = reg.invoke(chosen.id, fid=fid, snapshot=snapshot)
        inv.join(timeout=_ACTION_START_GRACE)
        if not inv.started:
            _say_on_screen(fid, inv.reason, client)
    finally:
        _close_palette(socket, harness=harness,
                       overlay_pane=os.environ.get("TMUX_PANE", ""))
    return 0


def _picker(row, fid: str, opened: list) -> "palette.Palette | None":
    """The surface *row* opens, or ``None`` when it opens none.

    Handed to `palette.own_the_tty` as its *then*, so a picker is drawn in the palette's
    own pane with the tty never leaving raw mode — see that function for why a second pane
    would race this one's teardown.

    **A pinned frame does not get a picker**, and that is Task 4's rule kept rather than
    a new one: the row is listed, it carries the sentence that says why, and opening a list
    of names none of which can be switched to would be an offer charter already knows it
    cannot honour. ``None`` here sends the row back to :func:`_draw_palette`, which says
    the note on the operator's screen.

    The roster is put in *opened* by :func:`_roster` rather than returned beside the
    surface, because what comes back out of `own_the_tty` is a ROW and the caller has to
    map it to the name it stood for. Matching that by title would mean matching on a string
    `overlay.Surface.render` has already contained — see `choose.Roster`. It also comes
    from there rather than from `choose.roster` directly, so a noun the operator already
    typed against is not listed a second time — see that function.
    """
    noun = choose.noun_of(row)
    if noun is None or row.note:
        return None
    return palette.Palette(catalogue=_roster(noun, fid, opened).rows,
                           label=noun, mouse=True)


def _name_rows(fid: str, opened: list) -> "tuple[overlay.Row, ...]":
    """Every workspace and every persona as one row, each labelled with which it is.

    **Handed to the palette as `query_only`, so this runs on the first keystroke and never
    on a query of nothing.** That is the cost half of Task 8: a directory listing per noun
    is what a name row is made of, and an operator who opened `F2` to press `detach` asked
    no question that needs one. `frame/palette.Palette._reachable` is where the promise is
    kept and `tests/test_frame_palette_names.py` is where it is measured.

    Workspaces before personas — `choose.NOUNS`' own order, which the doorways are already
    drawn in — because `narrow` never reorders and this is where the two groups' order is
    decided rather than at the moment somebody types.

    **A pinned noun's names are listed WITH THE PIN, not dropped.** The doorway refuses to
    open a picker for one, because a pane of names none of which can be switched to is an
    offer charter knows it cannot honour — but a name the operator has already typed is a
    question they asked, and answering it with an empty pane is #512's "no repos" over a
    plane that had them. So the reason goes in the note (`choose.labelled`), the row says
    why before the keypress, and pressing it lands the same sentence on the screen —
    `choose.pin_reason` and `switch.to_workspace` build it from one read of
    `state.identity`, so the row and the refusal cannot describe one frame two ways.

    The rosters go into *opened* exactly as a doorway's would, which is what makes
    :func:`_chosen_name` indifferent to how a name row reached the screen.
    """
    out: list = []
    for noun in choose.NOUNS:
        out.extend(choose.labelled(_roster(noun, fid, opened),
                                   choose.pin_reason(noun, fid)))
    return tuple(out)


def _roster(noun: str, fid: str, opened: list) -> "choose.Roster":
    """This palette's roster for *noun*, read from the plane at most once.

    **One roster per noun per palette, and that is correctness rather than thrift.** Row
    ids are `<noun>:n<N>` — an index into the list that produced them — so two rosters for
    one noun are two lists that agree only for as long as the plane does not change under
    them, and `_chosen_name` would answer from whichever was appended first. A workspace
    created between the operator typing and the operator pressing Enter on a doorway is
    enough to make that the wrong name.

    Reading once is the same answer read twice as often as it needs to be: typing `bet`
    and then opening the workspace doorway is one glob of `workspaces/`, not two.
    """
    for roster in opened:
        if roster.noun == noun:
            return roster
    roster = choose.roster(noun, fid)
    opened.append(roster)
    return roster


def _chosen_name(row, opened: list) -> "tuple[str, str] | None":
    """``(noun, name)`` when *row* came off a picker this palette opened, else ``None``.

    Asked of every roster rather than only the last, so that the answer does not depend on
    how many surfaces were drawn — and asked by row ID, which is charter's own
    `<noun>:n<N>` and never the operator's name.
    """
    for roster in opened:
        name = roster.name_of(row)
        if name is not None:
            return roster.noun, name
    return None


def _close_palette(socket: str, *, harness: str, overlay_pane: str) -> None:
    """Hand the pane back — as ONE tmux invocation, because the second command kills the
    caller.

    `overlay.close_argvs` is `select-pane`, `kill-pane`, re-arm; this process is what
    `kill-pane` is aimed at. Measured against tmux 3.7c, sent one at a time from inside
    that pane: the first returned 0 and the process was gone before the second answered,
    so the re-arm never ran and the overlay pane was left standing, unzoomed and unfocused,
    drawing a dead program. `tmuxctl.chain` sends all three in one command line, which tmux
    parses whole and runs server-side — 3 times out of 3, with the re-arm applied.

    ``None`` from `chain` is an empty or mismatched list, which is `close_argvs` refusing:
    a harness or overlay id that is not tmux's own word for a pane. Nothing is issued at
    all rather than a command built around a value charter cannot predict the parse of —
    `overlay.close_argvs`' own docstring records that an empty kill target kills the pane
    the command is running against.
    """
    argv = tmuxctl.chain(overlay.close_argvs(socket, harness=harness,
                                             overlay_pane=overlay_pane))
    if argv is None:
        return
    tmuxctl.run("handing the palette's pane back to the harness", argv)


def cmd_switch(args) -> int:
    """`charter frame-switch --workspace <name>` / `--persona <name>` — move THIS frame.

    **Typed by hand, or run by an agent inside the harness.** The picker no longer starts
    it: a name chosen off `frame/choose.py` is switched in the palette's own process,
    which is what lets the outcome reach the pane the operator is looking at rather than
    a status line after that pane is gone. This is the same switch by another door — one
    that takes a name nobody drew, which is why it still has refusals a picker's row
    cannot produce.

    The frame is resolved from `$CHARTER_SESSION_ID` exactly as `cmd_density`,
    `cmd_palette` and `cmd_respawn` resolve theirs, and for the same reason: one bind is
    shared by every frame on `SOCKET`, so the frame a command acts on is resolved at the
    moment it runs, never baked into anything.

    The switch itself is `frame/switch.py`'s — this function is the tmux half and nothing
    else: which frame, and where the answer is shown.

    **Every outcome is put on the operator's screen, and that is the point of the command
    existing at all.** #517: "a menu that silently fails against a lock is worse than no
    menu — if a switch is refused, the frame must say so." A refusal here has no other
    surface: this runs detached, with its own streams on `/dev/null` — see
    `builtin_actions._spawn` — because the palette pane it was started from is killed the
    instant it has started. So the message goes through `display-message`, which draws on
    the client's own status area and disappears on its own.

    **Which screen.** `-t <session>`, which measured against tmux 3.7c with two real ptys
    attached to one session draws on the most recently attached client. The menu could do
    better — it recorded the presser's own client an instant before it drew — and the
    palette does better for the refusal it can see BEFORE the keypress (the row carries
    the reason, on the pane every client of that session is looking at). What is left here
    is the outcome of work that outlived the surface that started it, and there is nothing
    at that point that knows whose keypress it was. One client is the ordinary case and
    gets the right answer either way.

    **Nothing is re-recorded.** The menu was a snapshot on disk, so a switch that did not
    rewrite it went on naming the workspace the frame had left; the palette resolves every
    name and every mark when it opens.

    **Always 0**, like `cmd_density` and `cmd_respawn`: nothing reads this status, and a
    non-zero exit is what makes tmux print into the harness pane.
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    if not fid:
        return 0
    ws = getattr(args, "workspace", None)
    persona_name = getattr(args, "persona", None)
    if ws:
        out = switch.to_workspace(fid, ws)
    elif persona_name:
        out = switch.to_persona(fid, persona_name)
    else:
        return 0
    _say_on_screen(fid, out.message)
    return 0


def _say_on_screen(fid: str, message: str, client: str | None = None) -> None:
    """Put one line on the frame's own screen. Best effort, never raises.

    **The message is a tmux FORMAT.** `display-message`'s own docs say so, and
    `tmuxctl.inert_format`'s measurement — a `#(...)` runs during format evaluation
    whether or not the thing goes on to display — applies here word for word. Every
    caller's *message* already carries contained names (`contain.one_line`, in
    `switch.py` and in `frame/actions.py`'s own `_reason`), which closes the newline half;
    `inert_format` closes the `#` half. Both, because they are different properties: one
    line, and inert.

    A leading `-` would make tmux read the message as a flag of its own and refuse the
    whole command — the same measured failure `inert_format` guards against — so the same
    guard is what runs here rather than a second one that "does the same thing".

    *client* is `#{client_name}` where a caller has one: the palette carries the presser's
    own client from the hotkey bind, and `-c` draws on exactly that terminal where `-t`
    draws on whichever attached most recently.

    `-d 4000`: long enough to read a sentence, short enough that it is gone before the
    operator wants the screen back. tmux's own default comes from `display-time`, which is
    an operator's setting for THEIR messages and typically 750ms — too short for a refusal
    that names a workspace and says what to do about it.
    """
    socket = state.frame_server(fid) or SOCKET
    argv = tmuxctl.server_argv(socket, "display-message", "-d", "4000")
    if client:
        argv += ["-c", client]
    else:
        argv += ["-t", fid]
    # One prefix for every outcome. Every refusal `switch.py` produces already reads as
    # one ("cannot switch: …", "no workspace 'x' — have: …"), and a second word saying so
    # only ate columns off a status line tmux truncates without saying it did — measured
    # against a 100-column client: `charter: refused — cannot switch: …` ran off the end.
    tmuxctl.run("reporting on the frame's own screen",
                argv + [tmuxctl.inert_format("charter: " + message)])


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
      there is no frame to resolve or count against (`cmd_palette` treats the same gap
      the same way);
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
    # `frame_slots.drawable`, not a key of `frame_slots.SLOTS`: the panel this is
    # bringing back may be hosting a component an installed provider supplies, and a
    # membership test against charter's own four renderers refused every one of them.
    # It is still a whitelist — the name is interpolated into a `respawn-pane` argv
    # read back off a tmux hook — and it still admits only names charter can resolve
    # to a component, never a SHAPE that looks like one.
    if not fid or not frame_slots.drawable(args.slot):
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


