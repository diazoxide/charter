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

**Every frame on charter's own server shares one tmux server (`SOCKET`), and a WORKSPACE
is a session on it while a CHAT is one window in that session.** A frame used to BE the
session, named by the frame id; it is now the window, named by the chat id, with the
session named after the workspace the chats belong to. That single choice is what makes the
rest of this module non-obvious, and it is why the session-scoped/hook-installing config reaches tmux through
`source-file`/direct `set-hook`/`set-environment` commands rather than through the `-f`
flag `layout.session_argv` also carries. Measured against tmux 3.7c: `-f` is read only at
the moment a client's connection actually STARTS the server — a later `new-session -f`
against a server that is already running (the ordinary case, once a first frame is up) is
silently ignored. A frame launched second, third, or fifty-first would then never get its
own config applied at all if it relied on `-f` alone. `source-file` and direct commands
both re-apply against whatever server already answers on the socket, so they work
identically for the first frame and the fifty-first (verified by hand against tmux 3.7c: a
hook installed this way for a SECOND session on an already-running server fires correctly,
and its teardown tears down only its own window, leaving a sibling chat untouched).

**A second, narrower race survives even that fix.** `new-session` starts the harness
running immediately; the hooks that would record its exit code and end the session are
not installed until separate `set-hook` calls a few milliseconds later (measured
8.2-10.5ms). A harness that dies inside that window is never caught by them — hooks do
not fire retroactively for an event that already happened — and this is worse than
`state.exit_code` merely reading back `None`: with nothing left to run the teardown, an
`attach` against that session BLOCKS FOREVER (verified by hand against a real tmux 3.7c,
via a Python `pty` driving the real launcher end to end — `remain-on-exit`, armed for
exactly this reason, is legitimately keeping the dead pane's session alive; nothing else
was ever going to end it). `remain-on-exit on` in the placeholder `-f` config is still
necessary — it is what keeps the pane around long enough to be askable at all — but is
not sufficient on its own. What actually closes the race is asking tmux directly,
`display-message -p -t <harness_pane> '#{pane_dead}:#{pane_dead_status}'`, IMMEDIATELY
after the hooks are installed and BEFORE ever calling `attach`: if the pane is already
dead at that point, this launcher finishes the hooks' own job itself (records the code,
runs the teardown) and skips `attach` entirely, rather than block on a session that
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

**One session now holds several chats, so that variable names the frame ROOT and the hook
appends the chat.** It used to name one frame's own `exit` file, which was correct while a
session held exactly one frame and is a wrong exit code the moment it holds two: a
`set-environment` is session-scoped, so the second chat's launch would repoint it and the
first chat's death would write its status into the second chat's file. The chat comes from
the `@charter_chat` WINDOW option through a tmux format, `#{@charter_chat}`, expanded in
the dying pane's own context — measured on tmux 3.7c and on tmux 3.2, a real `pane-died`
hook wrote `42` to `$CHARTER_FRAME_EXIT/<chat>/exit` on both. The hook action stays a
constant string, which is the property that paragraph is about: the operator-controlled
half (the plane root) still travels out of band, and the half that is interpolated is a
chat id in `_FRAME_ID_RE`'s closed alphabet, which holds no quote, `$`, backtick or space
for the nested parse to trip over.

**Teardown is its own hook, `pane-died[1]`, entirely separate from the write hook
(`pane-died[0]`).** `kill-window` alone, a constant string with no interpolated data at
all. Verified by hand: even with the write hook's own action deliberately mangled, the
teardown hook — sharing no text with it — still fired and ended the frame correctly.
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
that death total: it catches the dead pane, runs `kill-window`, and because `code is
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
quoting the pane (`_pane_last_words`, read BEFORE `kill-window` destroys it) when the
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

from . import config, contain, harness, inflight, instance, tui, util, workspace
from .frame import (actions as frame_actions, builtin_actions, chats, choose, component,
                    gather, layout, leave, overlay, pane, palette, picker, state, switch,
                    tmuxctl)
# Aliased because `cmd_reopen` is a function in this module and `reopen` reads as one: the
# module answers what a quit RECORDED and the command is what puts it back, and a bare
# `reopen.read()` beside `cmd_reopen` invites a reader to think one is the other.
from .frame import reopen as reopen_state
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

#: The session-scoped environment variable the write hook's shell reads the frame ROOT
#: back from — see the module docstring's "constant string" section for why the path is
#: delivered this way instead of being embedded in the hook's own action text, and
#: `_exit_path_env_argv` for why it is the root rather than one chat's file.
_EXIT_PATH_ENV = "CHARTER_FRAME_EXIT"

#: The WINDOW option that says which chat a window is drawing — the identity, where the
#: window's NAME is only a label.
#:
#: **A window name is not an identity, and that is measured rather than argued.** On tmux
#: 3.7c and on tmux 3.2 alike: `new-window -n api.3` does pin the name (it turns that
#: window's `automatic-rename` off), but with `allow-rename on` the pane's own output —
#: `printf '\033kPWNED\033\\'` — renamed the window to `PWNED` on both versions while
#: this option was untouched. `automatic-rename` is also ON by default, so any window
#: charter did NOT name follows whatever runs in it. A liveness list read from
#: `#{window_name}` therefore loses a chat the moment something renames its window, and
#: `state.reap` — the only thing bounding `.charter/frame/` — would delete a running
#: chat's state. So `_live_chats` reads this, and every lookup asks the window what chat
#: it is rather than parsing a name.
#:
#: `@`-prefixed because that is tmux's own namespace for options it does not define, and
#: `@charter_hatch` (`frame/overlay.py`) is the same mechanism one surface over — which
#: is also why `F12` is per chat with no new code: one chat per window means one hatch
#: per window.
_CHAT_OPTION = "@charter_chat"

#: The SESSION option that says which plane a workspace session belongs to — §4b's plane
#: marker, and the answer to a question `#{session_name}` cannot be asked.
#:
#: **One tmux server serves every plane on this machine (`SOCKET`) and a session's name is
#: a bare workspace name.** Measured on the operator's own socket while this was written:
#: eleven sessions from three different projects, and `default` — `DEFAULT_WORKSPACE`, a
#: name every plane has whether anybody chose it or not — is one of them. So a
#: `switch-client -t default` decided on a name can put an operator in another project's
#: frame, across every isolation boundary charter has: a different `CHARTER_ROOT`, a
#: different persona set, different vaults, different memory. That is the correctness
#: problem §4b's switch is built around, and it is why `_plane_session` never resolves a
#: name.
#:
#: **The value is `config.STATE_DIR` and not `config.ROOT`, which is a narrower claim and
#: the true one.** What makes two charters one plane, for every read on this path, is
#: sharing the `.charter/frame/` directory the chat records live in — two roots pointed at
#: one `$CHARTER_HOME` are one plane by every other question charter asks, and marking
#: them as two would refuse a switch that is perfectly safe.
#:
#: **Session-scoped, and the scope is load-bearing.** A chat is a window and a workspace is
#: a session (§2.1), so this is the one thing on the server whose lifetime is the
#: workspace's. Measured on tmux 3.7c and at the 3.2 floor: `set-option -t <a pane id>`
#: with neither `-w` nor `-p` sets it on that pane's SESSION, another session on the same
#: server reads it as unset, and `show-options -g` does not have it — so no session can
#: answer for a plane that is not its own.
#:
#: **It is read through a FORMAT and never through `show-options`, and that is measured
#: rather than preferred.** `#{@charter_plane}` in a `list-panes -a` format resolves the
#: option hierarchically — pane, then window, then session — so one call answers "which
#: session, which pane, whose plane" on both versions. `show-options -t <session> -v
#: @charter_plane` for an option nobody set answers **rc 1 with `invalid option:` on 3.7c
#: and rc 0 with an empty line on 3.2**: a reader built on it would have to branch on the
#: tmux version to tell "unmarked" from "would not answer", and the format has no such
#: split.
_PLANE_OPTION = "@charter_plane"

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

#: The format that says WHERE a pane is — the session and the window it belongs to, as
#: tmux's own ids rather than as names (#684).
#:
#: Both halves, in one ask, because both are needed at the same instant and by the same
#: caller: `cmd_chat` has to know the target is in the session it is switching WITHIN
#: before it selects anything, and has to know which window to expect the client on
#: afterwards. Ids and never `#{session_name}`/`#{window_name}`: a name is not an
#: identity (`_CHAT_OPTION` records the measurement — `allow-rename on` let a pane's own
#: output rename its window), the ids are what tmux hands back for its own targets, and
#: `#{session_id}` is about to be one.
#:
#: `\t` is the separator, and it is not `:` for the deletion sweep's reason. Neither id can
#: contain either character — tmux spells them `$<digits>` and `@<digits>`, which is what
#: :data:`_SESSION_ID_RE` and :data:`_WINDOW_ID_RE` hold them to on the way back — but a
#: split on `:` has to be spelled `partition` or `rpartition`, and with exactly one
#: separator by construction those two are the same program: a question no test can ever
#: answer. `split("\t")` and a field COUNT says the same thing and is a guard a test can
#: redden, which is also how `_WINDOW_SEAT_FORMAT` asks it.
_PANE_PLACE_FORMAT = "#{session_id}\t#{window_id}"

#: The format that says which window a SESSION is on right now — what a switch has to
#: read back before it may believe the client moved. Measured on tmux 3.7c and 3.2:
#: `display-message -p -t $N` resolves to that session's CURRENT window, so this is the
#: reading `select-window`'s return code is not.
_WINDOW_ID_FORMAT = "#{window_id}"

#: tmux's own shapes for a session and a window, held to for :data:`_PANE_ID_RE`'s reason
#: and at its boundary: both come back through text tmux re-parsed. A format that expanded
#: to nothing — which is what tmux answers for a target it cannot resolve, with rc **0**
#: and no stderr (measured on 3.7c) — fails these rather than becoming an empty target,
#: and an empty `-t` resolves to the CURRENT window, which is the exact shape #668's own
#: last commit closed for `select-window`.
#:
#: **The two are not load-bearing in the same way, and saying which is which is the point
#: of writing them down separately.** The SESSION id goes straight back out as a `-t`
#: target (`_session_window`), so its whole alphabet is #475's rule and every part of it
#: is asserted — the sigil, the digits, and that `$0;kill-server` never reaches a command
#: line. The WINDOW id is only ever COMPARED, against a reading taken from the same tmux a
#: moment later, so beyond telling a window id from a session id its strictness decides
#: nothing that the comparison does not already decide. A sweep that reports `r"@[0-9]+"`
#: widened to `r".+"` as a survivor is right, and this is the answer to it.
_SESSION_ID_RE = re.compile(r"\$[0-9]+")
_WINDOW_ID_RE = re.compile(r"@[0-9]+")

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


def driving_keys() -> list[str]:
    """The three facts a first launch needs, one per entry. Read by `cli._wire`'s epilog —
    every `charter <harness> --help` and `charter frame --help` — and by
    :func:`frame_ready`, so the launcher and the probe cannot come to disagree.

    **Nothing charter printed outside the frame said any of this** (#747). `charter claude
    --help` listed four flags; `charter frame --help` was the same text; `frame-probe`
    named every standing limit and no key. Inside the frame it is one two-word field on
    the attention strip — `F2 palette` — which `density = minimal` drops, and
    `charter docs show frame` had the whole answer with nothing pointing at it.

    **The keys are resolved, not spelled.** `[frame] hotkey` moves the palette off `F2`,
    and a help line that says `F2` on a plane that moved it is worse than one that says
    nothing: it is a fact an operator will act on. `config.FRAME` has already resolved the
    committed value (or the shipped default, for a key charter refused) by the time any
    parser is built, and `frame/overlay.py`'s `HATCH_KEY` is charter's own constant for
    the hatch — the same object `conf_text` binds in both cases.

    Scrollback is the third and it is not a key at all. `docs/frame.md` calls it "the
    difference people notice first", and it is the one an operator has no reason to
    connect to charter: their terminal's own scrollbar stops moving, and the frame is
    simply what was on screen when it happened.

    **A list rather than a sentence, and the two readers join it differently on purpose.**
    A `--help` epilog is not reflowed (`RawDescriptionHelpFormatter`, which is what keeps
    a key name off the end of a line) and wants one fact per row; `frame_ready` prints a
    paragraph per limit and wants a sentence. What must not be duplicated is the FACTS,
    and they are here once — a joiner at each surface is presentation, not a second
    answer.
    """
    return [
        f"{config.FRAME['hotkey']} opens the palette — every action the frame has is in "
        f"there",
        f"{overlay.HATCH_KEY} takes the keyboard back from a pane that has stopped "
        f"answering",
        "scrollback is tmux's copy-mode now, not your terminal's own",
    ]



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

    **The four STANDING conditions are reported here and nowhere else.** The first three
    used
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

    **A refused `[[frame.component]]` arrangement is the fourth (#738), and it is the one
    that is about the plane rather than the machine.** It joins the list rather than
    getting a surface of its own because it is the same KIND of fact as the unimplemented
    slot beside it — a standing property of the committed file, true on every launch until
    somebody edits it — and because it fails in the same way: the frame comes up, at rc 0,
    looking exactly like the frame the operator would have had if they had never written
    the tables. It does not change the exit code either. `doctor.check_control_plane_config`
    is its second reader, where `[plane] worktrees` and `[harness] default` are already
    named for being silently ignored; this is the surface an operator asks BEFORE they
    launch, and the one `docs/frame.md` sends them to.

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
    # The fourth, and the only one that is about this PLANE rather than this machine —
    # which is `no_renderer_message`'s position too, and the reason it belongs on the same
    # list rather than in a report of its own. A `[[frame.component]]` arrangement charter
    # will not draw is refused whole and falls back to `slots`, and the frame that comes
    # up is indistinguishable from the one the operator would have got without the tables
    # (#738). Read off `config.FRAME`, which resolved it once at import, so this stays what
    # this function promises to be: `tmuxctl.version()` and `config.FRAME`, nothing
    # started, nothing written.
    refused = config.FRAME.get("components_refused")
    if refused:
        ceilings.append(instance.refused_arrangement_message(refused))
    # **Said on every probe, ceiling or none** (#747). This is not a limit and it is not
    # news; it is the answer to the question an operator is actually asking when they run
    # the closest thing charter has to "tell me about the frame", and it was the one thing
    # this command could not tell them. It rides BELOW the ceilings rather than joining
    # them, for `_statusline_suppressed_note`'s reason one surface over: a ceiling is a
    # capability this machine does not have, and a key that works is the opposite of one.
    drive = "\n".join([f"  \u00b7 {k}" for k in driving_keys()]
                      + ["  · charter docs show frame — all of it, and what `[frame]` "
                         "configures."])
    if not ceilings:
        return 0, "ok", "\n".join([head, drive])
    return 0, "warn", "\n".join([head, *(f"  ↳ {c}" for c in ceilings), drive])


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


#: The pane option that says "this rectangle is a panel charter split off", and the whole
#: of what `conf_text`'s `MouseDown1Pane` bind asks before it decides whether a click may
#: move the keyboard. Written by :func:`_panel_mark_argv` on every panel pane charter
#: creates; never on the harness pane, never on the palette's, never on a pane the
#: operator split themselves.
#:
#: **PANE-scoped, which is the narrowest thing tmux has, and that is why it is the marker
#: rather than a window option holding the harness's own `%N`.** The issue that asked for
#: this (#634) sketched the other shape — `set -w @charter_harness_pane <harness id>` plus
#: `#{==:#{pane_id},#{@charter_harness_pane}}` in the bind — and then asked whether that id
#: would be a second write free to drift from `overlay.HATCH_OPTION`'s. It would have been;
#: this shape has no id in it at all, so there is nothing to drift. A pane option also dies
#: with the pane, so nothing charter writes here outlives the frame on a server every other
#: frame shares — the "last launched wins" trap this module's own docstrings keep naming.
#:
#: The value is charter's own constant and so is the name: both halves reach a tmux option
#: that a `bind` line later FORMAT-EXPANDS, which is `_HOTKEY_RE`'s hazard one option over,
#: and no operator string is anywhere near either.
#:
#: **A pane option is not inherited by a pane split off the one carrying it** — measured on
#: 3.7c and at `tmuxctl.FLOOR`, `split-window -t <a marked panel>` producing a child whose
#: `#{@charter_panel}` reads `''` on both. That is the answer to the sharpest form of "what
#: about a pane the operator split themselves": even splitting one out of a PANEL leaves it
#: tmux's pane rather than charter's, and a click on it selects it the way every pane in
#: their own tmux does. Pinned by `test_frame_input_reaches_a_component.py`, because it is
#: a fact about tmux and this comment would otherwise be the only thing asserting it.
_PANEL_OPTION = "@charter_panel"

#: What :data:`_PANEL_OPTION` is set to, and it is a DIGIT rather than a word for a
#: measured reason. `if-shell -F` reads its condition the way tmux reads any format
#: truth-value, which is not the way an operator reads `on`/`off` — probed on tmux 3.7c
#: and at `tmuxctl.FLOOR`, identical on both::
#:
#:     '1' -> TRUE     '0'  -> FALSE     (unset/'') -> FALSE
#:     '2' -> TRUE     'on' -> TRUE      'off'      -> TRUE
#:
#: So `off` would have marked every panel as a panel, and only the empty string and `0`
#: are false. `1` is the shortest value that is true, it is charter's own literal rather
#: than anything read off a config file, and the UNSET row is what makes "not a charter
#: panel" the default answer for every pane charter never marked — the harness's, the
#: palette's, and the operator's own.
_PANEL_MARK = "1"

#: WHICH component a panel draws, said to tmux beside :data:`_PANEL_OPTION` — and the two
#: are separate options rather than one because they answer two different questions, only
#: one of which a `bind` line reads.
#:
#: `_PANEL_OPTION` is a truth-value: `conf_text`'s `MouseDown1Pane` bind format-expands it
#: and tmux decides where a click goes on whether it reads true. Folding the component's
#: id into that value would have worked by accident — every id `component.usable_id`
#: admits is a true format value — right up to the first id spelled `0`, which that
#: alphabet does not forbid and which would have made one component's panel the only one
#: in the frame that steals the keyboard. Two options, so naming a component cannot change
#: the routing answer.
#:
#: **This is what makes #714's reconciliation possible at all.** `state.record_panes`
#: cannot be the authority on which pane draws which component, because a wrong record is
#: that defect's failure mode: the file is rewritten whole on every re-layout, so the
#: moment a second pane is split for a component the first one's id is gone from it and no
#: reader that goes through `state.panes` can see that pane again — `_drop_panels`
#: included. tmux still can. A pane carrying this option answers "which component am I"
#: out of the SERVER, which is the one place a wrong file does not reach.
#:
#: The value is held to `component.usable_id` on the way out (:func:`_panel_slot_argv`)
#: and again on the way back (:func:`_window_panels`) — the same alphabet
#: `frame/component.py` already holds every id that reaches a `bind` line to. Measured on
#: tmux 3.7c and at `tmuxctl.FLOOR`: a user option's VALUE is not re-expanded when a
#: format reads it — ``set-option -p @charter_panel_slot '#{pane_id}'`` comes back out of
#: ``list-panes -F '#{@charter_panel_slot}'`` as those literal ten characters — so the
#: guard is about what charter compares against `want` and what it will kill a pane over,
#: not about tmux's parser.
_PANEL_SLOT_OPTION = "@charter_panel_slot"

#: The one `list-panes` format :func:`_window_panels` asks the window with, held beside the
#: two option names it is built from so the reader and the writers cannot drift apart.
#:
#: A single space between the three, and that is safe rather than lucky: `_PANE_ID_RE`'s
#: `%N` and `component.usable_id`'s alphabet both exclude it, so the only field that could
#: ever contain one is a slot value charter did not write — which fails `usable_id` on the
#: way back in and is discarded there. An unset option expands to the empty string on both
#: tmux 3.7c and `tmuxctl.FLOOR` (measured), so the line for an unmarked pane is `%0` and
#: two empty fields, and `str.split(" ")` — never bare `.split()`, which collapses them —
#: is what keeps that three fields rather than one.
_PANEL_LIST_FORMAT = f"#{{pane_id}} #{{{_PANEL_OPTION}}} #{{{_PANEL_SLOT_OPTION}}}"


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

    **`MouseDown1Pane` is the second such bind, and it is what keeps `click` point-to-act
    once `[frame] mouse = true` (#634).** tmux's own default for that key is `select-pane
    -t = \\; send-keys -M`: with its mouse on, tmux moves the keyboard to the pane under
    the pointer *before* forwarding the click. So the flag an operator turns on precisely
    because they want to click panels was the flag that took the property away, and the
    wheel — which tmux forwards without selecting — never did it. Charter rebinds the key
    to ask **whose rectangle the pointer is over** and answer separately:

    * a pane charter marked as a panel (:data:`_PANEL_OPTION`) — forward and **do not**
      select, which is exactly what `frame/events.py` delivers and what the frame is for;
    * anything else — the harness pane, the palette's pane, a pane the operator split
      themselves — **tmux's own two commands, unchanged**, so clicking BACK to a pane
      still works.

    That second row is the whole reason this is a conditional and not `bind -n
    MouseDown1Pane send -M`. Measured on tmux 3.7c and at `tmuxctl.FLOOR` (3.2)
    identically, real server, real client on a real pty, `mouse on`, three panes — a
    marked panel, the harness, and one more split by hand standing in for the operator's
    — with SGR reports injected as a reporting terminal sends them::

        bind              click a panel        click back to harness   click own split
        tmux's default    delivered, MOVED     works                   works
        blanket send -M   delivered, unchanged BROKEN (stays put)      BROKEN (stays put)
        this one          delivered, unchanged works                   works

    The blanket row is not a milder version of the same thing; it takes away clicking back
    to any pane at all, harness included, which is worse than the focus steal it fixes.

    **The marker is on the PANEL, not on the harness, and that decides the third case.**
    The issue sketched the mirror image — a window option holding the harness pane's own
    `%N`, compared with `#{==:#{pane_id},#{@charter_harness_pane}}` — and asked first
    whether that format parses at the floor. It does: measured on a 3.2 built from the
    release tarball, `#{==:}` with a nested `#{pane_id}` and a nested user option
    evaluates to `1` on the harness and `0` on a panel, and the whole `bind` line
    `source-file`s at rc 0 and reads back byte for byte through `list-keys`. The format
    was available and the SHAPE is what is refused: marking the harness makes every pane
    that is not the harness un-clickable-to, so a pane the operator split inside charter's
    window would stop taking the keyboard on a click — tmux's documented behaviour
    removed from a pane charter has nothing to do with. Marking the panels leaves that
    pane exactly as tmux left it, and leaves no pane id in the binding to drift from the
    one `overlay.HATCH_OPTION` already holds.

    **Bound whatever `mouse` says, like the wheel above and for a sharper reason than
    symmetry.** A root key table is server-wide and `source-file` can only ADD a binding —
    omitting this line on a `mouse = false` launch would not unbind what a `mouse = true`
    frame on the same socket already bound, so gating it would buy nothing and make the
    root table depend on launch order. It costs a `mouse = false` frame nothing either
    way: with tmux's own mouse off tmux runs no mouse binding at all, and the reports go
    to whichever pane the terminal was already reporting for.

    `MouseDown1Pane` and `WheelUpPane` are both in `instance.component_tables`'s set of
    keys a component may not claim, for `HATCH_KEY`'s reason exactly — both are written
    here BEFORE the toggles, so tmux's last-wins would leave charter's mouse handling
    silently deleted and `list-keys` reading back one line where two were meant (#566).

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

    **`"#{@charter_chat}"` is what makes that resolution per CHAT, and it is not
    decoration over the variable — it is the only thing that works.** A session holds
    several chats now, and `set-environment` has no window scope, so
    `$CHARTER_SESSION_ID` can only ever name one of them. Measured on tmux 3.7c and on
    tmux 3.2, one session carrying `set-environment CHARTER_SESSION_ID session-wide` with
    a window created `-e CHARTER_SESSION_ID=chat-A` in it: the window's own PANE reported
    `chat-A` and a `run-shell` fired against that session reported `session-wide`. This
    bind's action IS a `run-shell`, so the variable would hand every chat's keypress the
    same frame. The window option does not: it is expanded in the context of whoever's
    keypress is firing the bind — the same property `#{client_name}` was carried here for
    until #729 removed the need for it — so `F2` in chat two opens chat two's palette. `cmd_palette` falls back to `$CHARTER_SESSION_ID` when the
    option is empty, which is what a frame launched by an older charter — its window
    carrying no such option, its bind text replaced by this one the moment a newer frame
    launches on the shared server — still resolves through.

    **This bind used to carry `"#{client_name}"` as a third value, and does not any
    more** (#729). It named which of several clients attached to one frame should be TOLD
    when an action refuses, and it was threaded from here through `args.client` and the
    palette pane's own relaunch argv for exactly one consumer: `display-message -c`. That
    consumer is gone. An outcome is now written to the frame's own state and drawn by its
    attention panel (`_say_on_screen`), which is a PANE — so every client attached to the
    frame sees it, and "which client pressed the key" stopped being a question that has
    to be answered rather than being answered better. The value is not merely unused, it
    is unnecessary: the surface it was selecting between no longer has variants.

    `charter frame-palette` still ACCEPTS the positional (`cli.py`, `nargs="?"`), and that
    is not vestigial — a bind installed by a charter that predates this change is still
    sitting in a running server's key table across the upgrade and fires the command with
    a client name in it, exactly as `--chat`'s own optionality is there for. Accepting and
    ignoring it is what keeps `F2` working on those frames; emitting it is what stopped.

    **The palette was never per-client anyway, and that is the difference from the menu it
    replaced.** A `display-menu` was drawn per client; the palette is a PANE (§4k), and a
    pane belongs to the window, so with two clients attached to one frame both of them
    see it. That is the price of the surface being something charter draws rather than
    something tmux draws, it is the same price every other panel already pays, and the
    outcome line has now been moved onto the same footing as the surface that produces it.

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
    no client name: a toggle changes the FRAME, not what one client is looking at, so
    there is nothing here to draw on a particular presser's screen — which since #729 is
    true of the palette's bind as well, and for the same reason.
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

        bind-key -T root F2  run-shell "\\"\\$CHARTER_PY\\" -m charter frame-palette"
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
        f"'\"${_CHARTER_PY_ENV}\" -m charter frame-palette "
        f"--chat \"#{{{_CHAT_OPTION}}}\"'",
        f"bind -n {tmuxctl.WHEEL_KEY} if-shell -F -t = '#{{mouse_any_flag}}'"
        " 'send-keys -M' 'copy-mode -e; send-keys -M'",
        f"bind -n {tmuxctl.CLICK_KEY} if-shell -F -t = '#{{{_PANEL_OPTION}}}'"
        " 'send-keys -M' 'select-pane -t =; send-keys -M'",
    ]
    for name, key in (toggles or {}).items():
        if not component.usable_id(name):
            continue
        if instance.toggle_key(key) is None:
            continue
        lines.append(f"bind -n {key} run-shell "
                     f"'\"${_CHARTER_PY_ENV}\" -m charter frame-toggle {name} "
                     f"--chat \"#{{{_CHAT_OPTION}}}\"'")
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
    return tmuxctl.server_argv(socket, "set-environment", "-t", session,
                               _CHARTER_PY_ENV,
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


def _exit_path_env_argv(*, socket: str, session: str, frame_root: str) -> list[str]:
    """`set-environment`: carries the frame ROOT to the write hook's shell out of band.

    One argv value, no shell parsing at all on this side — the whole point (see the
    module docstring). `run-shell`'s own spawned shell later reads it back from its
    inherited environment via `$CHARTER_FRAME_EXIT`, verified by hand to work for a
    SESSION-scoped `set-environment` (no `-g`) reaching a hook fired for a pane in that
    session.

    **The root, not one chat's file, and a session holding two chats is why.** This is a
    SESSION-scoped variable and a session is now a workspace, so a value naming one
    chat's `exit` file is a value the next chat's launch repoints — after which the first
    chat's death writes its status into the second chat's file and that chat's launcher
    reports a number that was never its own. The hook appends the chat itself, from the
    window option `_chat_option_argv` writes; see `_pane_died_write_hook_argv`.
    """
    return tmuxctl.server_argv(socket, "set-environment", "-t", session,
                               _EXIT_PATH_ENV,
                               frame_root)


def _session_id_env_argv(*, socket: str, session: str, chat: str) -> list[str]:
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

    **The value is the CHAT's id, and the session's name is the workspace.** Those used to
    be one string; they are two now, and this variable means the frame rather than the
    session it is drawn in, so it takes *chat* explicitly. `state.new_chat_id` mints it in
    the same restricted alphabet `state.frame_id` used, so there is still nothing here for
    this call's own text to sanitise.

    **It is a FALLBACK under tabs, not the mechanism, and that is measured.** A session
    can only hold one value of it, so with two chats it names whichever launch set it
    last. What makes a keypress reach the RIGHT chat is `conf_text`'s bind carrying
    `#{@charter_chat}`, expanded in the presser's own window. This is still set because a
    `run-shell` child reads the SESSION's environment and nothing else: measured on tmux
    3.7c and on 3.2, a window created with `-e CHARTER_SESSION_ID=chat-A` gave `chat-A` to
    its own pane and `session-wide` to a `run-shell` fired against that session. So a
    chat's harness gets its id from `-e` (`layout.chat_window_argv`), a keypress gets it
    from the bind, and this is what answers for everything else that asks the session.
    """
    return tmuxctl.server_argv(socket, "set-environment", "-t", session,
                               "CHARTER_SESSION_ID", chat)


def _chat_option_argv(*, socket: str, harness_pane: str, chat: str) -> list[str] | None:
    """`set-option -w`: write which chat this window is drawing. ``None`` to refuse.

    Task-critical and one line, because it is what every later lookup asks instead of
    parsing a name (see :data:`_CHAT_OPTION` for the measurement that makes a name
    unusable for it). `state.reap` reads it back through `_live_chats`, the `pane-died`
    write hook expands it as `#{@charter_chat}` to find the chat's own `exit` file, and
    `conf_text`'s binds carry it so a keypress reaches the presser's own chat.

    **Window-scoped, targeting the harness PANE** — the idiom
    `_panel_remain_on_exit_argv` and `overlay.arm_hatch_argv` already use, and for the
    same reason: `-w -t <a pane id>` resolves to that pane's window, which is charter's
    own on either server (measured again here on tmux 3.7c and 3.2 — the option set that
    way reads back through `list-windows -a -F '#{@charter_chat}'`). Never `-g`, which
    would hand every window on charter's shared private server one chat's id.

    ``None`` rather than a raise for `overlay.arm_hatch_argv`'s reason — this is called
    from inside a launch — and the check is `_FRAME_ID_RE`, the same alphabet a frame id
    already travels to tmux under. It is asked HERE rather than trusted from
    `state.new_chat_id` because the value goes on to be expanded by tmux inside a hook
    action and inside a key bind, and a value that reaches two re-parsing grammars is
    checked where it enters them, not where it was minted.
    """
    if not _FRAME_ID_RE.fullmatch(chat or ""):
        return None
    return tmuxctl.server_argv(socket, "set-option", "-w", "-t", harness_pane,
                               _CHAT_OPTION, chat)


def _this_plane() -> str:
    """Which plane this process is acting for, as :data:`_PLANE_OPTION` spells it.

    Asked rather than cached, because `config.use` re-points every charter path at
    runtime — that seam is how `tests/_isolation.py` gives a test its own plane and how
    the two-plane tests put two of them in one process. A module-level constant would be
    the first plane that happened to import this.
    """
    return str(config.STATE_DIR)


def _plane_option_argv(*, socket: str, harness_pane: str) -> list[str] | None:
    """`set-option`: write which PLANE this session belongs to. ``None`` to refuse.

    :func:`_chat_option_argv` one scope out, and the differences between them are all
    that is worth saying:

    * **No `-w`.** That is the whole of what makes this session-scoped rather than
      window-scoped; the target is still the harness pane, for `_chat_option_argv`'s
      reason — a pane id resolves to its window and to its session on both versions, and a
      workspace NAME is unusable as a target twice over: tmux parses `api.1` as
      ``window.pane``, and a bare name is matched against WINDOW NAMES before it settles
      for a session's current window (measured for §4b; `layout.chat_window_argv` carries
      the reading). Charter's chat windows are named `<workspace>.<n>`, so `-t
      <workspace>` on charter's own server names one of that workspace's chats.
    * **Written once, by the launch that CREATES the session, and never afterwards.** A
      launch that finds `session in live_sessions` is joining a session it did not
      necessarily make: that test is on the NAME, which is exactly the collision this
      option exists to record, so re-writing the marker there could relabel another
      plane's session as this one's — turning the guard into the defect. A session an
      older charter created carries no marker and keeps none; :func:`_plane_session` reads
      that as "unmarked", which is where the pane records it already relies on still
      decide.
    * **The refusal is about the ROUND TRIP, not about an alphabet.** A chat id has one
      (`_FRAME_ID_RE`); a state directory is a filesystem path and charter does not get to
      narrow it. What this reader needs is a value that survives
      `list-panes -F 'a\\tb\\t#{@charter_plane}'`, so the check is exactly that: a tab
      would add a field and a newline would add a row, and either would be read as a
      server answering some other format. A plane whose path holds one is left unmarked
      rather than marked wrongly, which costs it §4b's veto and nothing else.
    """
    value = _this_plane()
    if not value or any(c in value for c in "\t\r\n"):
        return None
    return tmuxctl.server_argv(socket, "set-option", "-t", harness_pane,
                               _PLANE_OPTION, value)


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

    **The path is the frame root plus this window's own chat**, `$CHARTER_FRAME_EXIT` from
    the session and `#{@charter_chat}` from the window, and both halves are load-bearing.
    A session holds several chats, so the session-scoped variable cannot name one chat's
    file without the next launch repointing it (see `_exit_path_env_argv`); and the chat
    cannot travel in the variable for the same reason. `#{@charter_chat}` is expanded by
    tmux in the context of the pane the hook fired for — measured against tmux 3.7c AND
    tmux 3.2, a real `pane-died` on a window carrying `@charter_chat hooky.7`, with the
    exit path set to a directory: both wrote `42` to `<dir>/hooky.7/exit`.

    **This action is still a CONSTANT string** — the property the module docstring calls
    the whole fix. The two things it names are resolved by tmux and by the shell at fire
    time, not interpolated here, so a plane root with an apostrophe in it still cannot
    corrupt the stored text. The chat id that IS interpolated by tmux is `_FRAME_ID_RE`'s
    closed alphabet (`_chat_option_argv` refuses anything else on the way in), so it holds
    no quote, `$`, backtick, space or `#` for either parser to trip over.
    """
    action = ('run-shell "v=#{pane_dead_status}; echo '
             f'\\"\\${{v:-{_UNKNOWN_DEATH_CODE}}}\\" > '
             f'\\"\\${_EXIT_PATH_ENV}/#{{{_CHAT_OPTION}}}/exit\\""')
    return tmuxctl.server_argv(socket, "set-hook", "-p", "-t", harness_pane, "pane-died",
                               action)


def _pane_died_teardown_hook_argv(*, socket: str, harness_pane: str) -> list[str]:
    """`pane-died[1]`: ends the CHAT's window. Nothing else — see the module docstring's
    "Teardown is its own hook" section for why this is never combined with the write
    hook above.

    **`kill-window`, never `kill-session`, and this is the single most dangerous line in
    the move to chats.** A session used to be one frame, so ending it ended exactly the
    frame whose harness had died. A session is a WORKSPACE now and a chat is one window
    in it, so the old spelling would take every other chat in that workspace down with the
    one that died — including chats mid-turn, in another agent's terminal, for a death
    that had nothing to do with them.

    Measured on tmux 3.7c and on tmux 3.2, one session, a pane-scoped `pane-died[1]
    kill-window` on a harness that exits 9: the dying chat's window is gone, every sibling
    window is still listed, and the session is still listed. And the single-chat case is
    unchanged where it matters — killing a session's last window destroys the session
    (measured on both), so `attach` returns exactly as it did.

    The hook is PANE-scoped (`-p -t <harness pane>`), and `kill-window` with no `-t` acts
    on the target the hook fired for, which is that pane's own window. The same
    pane-resolves-to-its-window rule is what `_remain_on_exit_argv` and
    `overlay.arm_hatch_argv` already rely on for `-w -t <pane>`.

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
                               "pane-died[1]", "kill-window")


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


def _live_chats(socket: str) -> set[str] | None:
    """Every chat id *socket*'s server reports through the `@charter_chat` window option,
    or ``None`` when it did not answer at all.

    The liveness list for a CHAT, and the one `state.reap` has to be given for one: a
    chat's id carries no launcher pid (`state.new_chat_id`), so this list is the only
    thing keeping its directory, and a chat missing from it is a chat whose state is
    deleted.

    **The OPTION, not `#{window_name}`, and the difference is a running chat's state.**
    `_live_windows` reads the name because that is what the guest path has always named a
    frame's window; a name is not an identity. Measured on tmux 3.7c and on tmux 3.2:
    `new-window -n api.3` pins the name and turns `automatic-rename` off for that window,
    but with `allow-rename on` the pane's own `printf '\\033kPWNED\\033\\\\'` renamed it
    to `PWNED` on both versions while `@charter_chat` stayed `api.3`. A liveness list read
    from the name loses that chat, and `reap` — the only bound on `.charter/frame/` —
    removes the state of a chat that is running.

    **``None`` rather than an empty set for a non-zero return, and here it costs more
    than it does anywhere else in this module.** `_live_windows` keeps the two apart
    because `$TMUX` can outlive its server; this keeps them apart because a chat's
    directory has NOTHING ELSE keeping it. An old frame that vanished from a failed
    `list-sessions` is still held by the pid in its name (#383); a chat has no pid, so a
    server that answers "no windows" because it was wedged rather than because it is
    empty would take every live chat's state with it — the version file its panels poll
    and the exit code its launcher has not read yet. Both callers refuse to reap on that
    answer. A LIVE server with no chats on it is a different fact and answers with an
    empty set, which reaps exactly as it should.

    A window with no such option prints an empty line, which the comprehension drops.

    **Stripped once, tested once.** `_live_sessions` and `_live_windows` both spell
    ``{line.strip() for line in … if line.strip()}``, and the second call is an
    EQUIVALENT MUTANT: ``s.strip()`` and ``s.lstrip()`` are truthy for exactly the same
    strings, so no test can tell the guard from its own mutation. Stripping into a name
    and asking whether the NAME is empty says the same thing once, and both halves are
    then pinnable — `tests/test_frame_launcher.TheChatListIsParsedLineByLine` pins them.
    """
    out = tmuxctl.run("listing the chats already running",
                      tmuxctl.server_argv(socket, "list-windows", "-a", "-F",
                                          f"#{{{_CHAT_OPTION}}}"),
                      timeout=5, report=False)
    if out.returncode != 0:
        return None
    names = (line.strip() for line in out.stdout.splitlines())
    return {name for name in names if name}


#: What :func:`_chat_being_left` asks of every window on a server, in one call.
#:
#: Three fields and all of them tmux's own: which SESSION a window is in, whether it is
#: that session's current one, and which chat it draws. Ids and an option, never names —
#: a window name is not an identity (`_CHAT_OPTION`), and a session NAME is worse than
#: useless as a `-t` target here because tmux parses `api.2` as ``window.pane``, so a
#: workspace with a dot in it would resolve to somebody else's window or to none.
#:
#: `\t` because none of the three can contain one: `#{session_id}` is `$<digits>`,
#: `#{window_active}` is `0` or `1`, and a chat id is held to `_FRAME_ID_RE`'s alphabet
#: on the way back out.
_WINDOW_SEAT_FORMAT = f"#{{session_id}}\t#{{window_active}}\t#{{{_CHAT_OPTION}}}"


def _chat_being_left(socket: str, *, beside: str) -> str:
    """The chat currently on screen in the same tmux session as chat *beside* — or ``""``.

    **What a launch has to know before it takes the client somewhere else** (#688).
    `cmd_chat` tears the panels of the chat it leaves down, and #668 calls that a
    correctness rule rather than a saving: a background window keeps STALE geometry
    (§7.4, measured identically on tmux 3.7c and 3.2), so panels left running in one are
    not idle, they are rendering at a width that is no longer their window's. The launch
    path is what CREATES that situation — `layout.chat_window_argv` opens the second chat
    with `new-window -d` and the launcher then selects it — and it kept no such rule.

    One `list-windows -a`, and the whole of the reason it is not two: the new chat's own
    window is in that answer too, so its `@charter_chat` gives the SESSION without a
    second round trip, and nothing here ever hands tmux a name to resolve. That matters
    beyond tidiness — see :data:`_WINDOW_SEAT_FORMAT` for what `-t <workspace>` does to a
    workspace whose name contains a dot.

    ``""`` for every way this can fail to answer, because the caller does one thing with
    all of them: leave the other chat alone. A server that would not list its windows, a
    *beside* whose window is not in the list, a session whose current window carries no
    chat at all (the operator's own shell, on their own server), and *beside* itself —
    the launch that created the session, whose only window is already current.

    Held to `_FRAME_ID_RE` on the way out, at #475's boundary: this value came off a tmux
    option, and it is about to be a state directory's name and the key `state.harness_pane`
    is read under.
    """
    seats = _window_seats(socket, "finding the chat this launch is leaving")
    # `None` when *beside*'s own window is not in the listing, and it needs no branch of
    # its own: no session id is ever `None`, so the second lookup below matches nothing
    # and answers "". A sentinel that cannot collide is one guard rather than two.
    session = next((s[0] for s in seats if s[2] == beside), None)
    # *beside* itself is this function's own case and not :func:`_chat_showing`'s: the
    # launch that CREATED the session is already on its only window, and there is nothing
    # for it to leave.
    chat = _chat_showing(seats, session)
    return chat if chat != beside else ""


def _window_seats(socket: str, why: str) -> list[list[str]]:
    """Every window on *socket* as a :data:`_WINDOW_SEAT_FORMAT` row, rows charter cannot
    read dropped.

    One `list-windows -a` for both readers of it — :func:`_chat_being_left`, before a
    launch takes the client somewhere else, and :func:`_chat_showing`, after a workspace
    switch has taken it. They ask the same question of the same listing keyed two
    different ways (by a chat, by a session), and a second call would be a second reading
    of a server that moves between them.

    **No branch on the return code**, which is the shape both callers had: a listing that
    failed has an empty stdout and falls out as "no seats and therefore no answer" — the
    same sentence, said once. **Exactly three fields**, because `#{@charter_chat}` is an
    option and an option's value is whatever somebody set: one holding a tab of its own
    would otherwise have its first half read as a chat id.
    """
    out = tmuxctl.run(why, tmuxctl.server_argv(socket, "list-windows", "-a", "-F",
                                               _WINDOW_SEAT_FORMAT),
                      timeout=5, report=False)
    return [s for s in (line.split("\t") for line in out.stdout.splitlines())
            if len(s) == 3]


def _chat_showing(seats: list[list[str]], session: str | None) -> str:
    """Which chat *session*'s current window is drawing, out of *seats* — or ``""``.

    ``""`` for every way this can fail to answer, because both callers do one thing with
    all of them: leave that window alone. A *session* nothing in the listing is in (a
    `None` sentinel from `_chat_being_left`, or a session id that went away between two
    calls), and a session whose current window carries no chat at all — the operator's own
    shell, on their own server, or a workspace session an older charter left unnamed.

    Held to `_FRAME_ID_RE` on the way out, at #475's boundary: this value came off a tmux
    option, and it is about to be a state directory's name and the key
    `state.harness_pane` is read under.

    **Not stripped, and that is the guard rather than a missing one.** `splitlines` has
    already taken the line terminator and the fields are cut on tabs, so there is no
    whitespace here tmux put in — only whitespace somebody put in the OPTION, and
    `_FRAME_ID_RE`'s alphabet holds none. Stripping would turn ` demo.1 ` into a chat
    charter then re-dresses, which is normalising a value the one rule here is there to
    refuse. It is also the `strip`/`lstrip` shape `_live_chats`' own docstring names:
    truthy for exactly the same strings, so no test can tell the two apart — a question
    with one answer, not asked.
    """
    chat = next((s[2] for s in seats if s[0] == session and s[1] == "1"), "")
    return chat if _FRAME_ID_RE.fullmatch(chat) else ""


#: What :func:`_plane_session` asks of every pane on a server, in one call.
#:
#: The first two fields are tmux's OWN ids and neither of them is a name —
#: :data:`_WINDOW_SEAT_FORMAT`'s rule one noun over, and here it is the whole correctness
#: argument rather than a target-grammar convenience. One tmux server serves every plane
#: on this machine (:data:`SOCKET`), session names are bare workspace names, and `default`
#: is a name EVERY plane has, so `#{session_name}` cannot say whose session it is.
#: `#{pane_id}` can: a pane id is minted by the server, and the only plane holding one is
#: the plane whose launcher wrote it down (`state.record_harness_pane`).
#:
#: **The third field is the marker that closes what a pane id alone cannot** — see
#: :data:`_PLANE_OPTION`, and :func:`_plane_session` for which of the two decides what.
#: It is read here rather than through a second `show-options` call because a session
#: option resolves in a PANE's format on both versions (measured on 3.7c and at the 3.2
#: floor), so this stays one round trip, and because `show-options -v` for an unset user
#: option answers differently on the two versions where a format does not.
#:
#: `\t` between them because none of the three can contain one: `#{session_id}` is
#: `$<digits>` and `#{pane_id}` is `%<digits>`, both tmux's own and neither settable by
#: anyone, and `_plane_option_argv` refuses to write a marker holding one.
_PANE_SEAT_FORMAT = f"#{{session_id}}\t#{{pane_id}}\t#{{{_PLANE_OPTION}}}"


def _plane_session(socket: str, *, ws: str) -> tuple[str, str] | None:
    """``(tmux session id, chat id)`` for THIS PLANE's live *ws* — or ``None``.

    **The one question two surfaces ask**: §4k's open-or-focus
    (:func:`_workspace_to_focus`) and §4b's workspace switch (:func:`_switch_client`).
    Both need "which session on this shared server is MY plane's `ws`", both are wrong in
    the same way if they answer it from a name, and a second implementation would be a
    second chance to answer it differently.

    **Matched on this plane's own chat directories, never on a live session name, and that
    inversion is §3.3's.** `cmd_launch` asks `if session in live_sessions:` and that
    question is unanswerable across planes: `_live_sessions` returns every session on the
    machine, `default` is `DEFAULT_WORKSPACE_FALLBACK` and therefore a name every plane
    has, and `config.STATE_DIR` is the only thing that is per-plane. Reading the answer off
    a name would make an existing collision into ADVERTISED behaviour — `charter -w
    default` in one plane attaching to another plane's frame, and a workspace tab in one
    plane switching an operator into another project's harnesses. So the question starts on
    disk: `chats.of_workspace` is `.charter/frame/` under THIS plane's state directory, and
    `state.harness_pane` is a `%N` this plane's own launcher wrote down.

    **The marker is a veto and the pane record is the finder, and each covers what the
    other cannot.** A pane id is unique on a running server but restarts at `%0` when that
    server does, so a `%3` recorded for a chat that is over can later name a different live
    pane — on the launch path `cmd_launch`'s reap narrows that to almost nothing, and on
    the switch path there is no reap in front of it at all. :data:`_PLANE_OPTION` closes it
    from the other side: a candidate session whose marker names a plane that is not this
    one is refused however well its pane id matched. What the marker cannot do is find
    anything, because a session an older charter created carries none — every session on
    the operator's own socket the day this was written — so an ABSENT marker is read as
    "unmarked" and decides nothing, and the pane record still answers. The residual is
    therefore one recycled pane id belonging to a session created by a charter that
    predates this option, and it ages out with those sessions.

    ``None`` for every way this can fail to answer, because both callers do the same thing
    with each: a workspace this plane has never opened has no chat directory, reaches no
    tmux call at all, and can never be confused with anybody's.

    Measured on tmux 3.7c and at the 3.2 floor, identically on both: `list-panes -a` lists
    every pane on the server with its own session id, and a session-scoped user option
    resolves in that pane format.
    """
    mine: dict[str, str] = {}
    for chat in chats.of_workspace(ws):
        pane = state.harness_pane(chat)
        if pane is not None:
            mine[pane] = chat
    if not mine:
        return None
    out = tmuxctl.run("finding this plane's live chats in this workspace",
                      tmuxctl.server_argv(socket, "list-panes", "-a", "-F",
                                          _PANE_SEAT_FORMAT),
                      timeout=5, report=False)
    # No branch on the return code, for `_window_seats`' reason: a listing that failed has
    # an empty stdout and falls out below as "no seats and therefore no answer". Exactly
    # three fields per row, so a server answering some other format cannot have half a row
    # read as a pane id — and `_plane_option_argv` is what guarantees charter's own marker
    # never adds a fourth.
    seats = [line.split("\t") for line in out.stdout.splitlines()]
    ours = _this_plane()
    seat = next((s for s in seats
                 if len(s) == 3 and s[1] in mine and s[2] in ("", ours)), None)
    if seat is None:
        return None
    return seat[0], mine[seat[1]]


def _workspace_to_focus(socket: str, *, ws: str) -> tuple[str, str] | None:
    """``(tmux session id, chat id)`` for THIS PLANE's live *ws* when somebody is looking
    at it — otherwise ``None``, meaning "open a chat as you always did".

    **§4k, and §3.3 is why it is spelled this way.** `charter -w foo` used to mean "add a
    chat to `foo`" unconditionally, and §2.3 measured what that does to a workspace
    somebody already has open: both clients of a tmux session share one current window, so
    the `select-window` that puts THIS launch on its new chat drags the other client off
    the chat they were reading, and `_drop_panels` then tears that chat's panels down.
    §2.10 measured the alternative and there is not one — tmux has no per-client current
    window inside a session on 3.7c or at the 3.2 floor, and the only mechanism that does
    not drag is a session GROUP, which would stop a workspace being a tmux session.
    So the drag is not fixed; the reason to drag is removed. A workspace somebody is
    already in is FOCUSED — one more client on the session they are on, looking at the
    window they are on — and a workspace nobody is in opens a chat exactly as before.

    **Which session is this plane's is :func:`_plane_session`'s question and not this
    one's** — a workspace tab asks it too now (§4b), and one answer is what stops a focus
    and a switch disagreeing about whose `default` is whose. What is left here is the
    second half, and it is the half §4k is actually about: **is anybody looking at it.**

    **Three answers, and the two tmux calls are asked in the order that makes most
    launches pay for neither.** No chat directory for *ws* on this plane — the ordinary
    first launch — returns before `list-panes`. No live pane among the ones this plane
    recorded — every chat of *ws* is cold — returns before `list-clients`. And a live
    session nobody is attached to returns ``None`` as well, deliberately: with no client
    on it there is nobody to drag, so a launch there SHOULD add its chat and select it,
    which is the behaviour that shipped and the one an operator reopening a detached
    workspace wants. **§4b's switch takes the opposite view of that same reading and is
    right to**: it is moving a client rather than avoiding a drag, so a detached workspace
    is a perfectly good place to move one to and it never asks this question at all.

    Measured on tmux 3.7c and at the 3.2 floor, identically on both: `list-clients -t $N`
    takes a session ID as its target, prints one line per attached client, and exits 0 with
    nothing on stdout when none is attached (rc 1 only for a session id the server does not
    have).
    """
    seat = _plane_session(socket, ws=ws)
    if seat is None:
        return None
    clients = tmuxctl.run("asking whether anybody is looking at that workspace",
                          tmuxctl.server_argv(socket, "list-clients", "-t", seat[0],
                                              "-F", "#{client_name}"),
                          timeout=5, report=False)
    # Both halves, and they are two different facts: a non-zero return is a server that
    # would not answer (or a session it no longer has, between the two calls), and an
    # empty answer is a session with no client on it. Either way nobody is being dragged,
    # so either way this launch opens its own chat.
    #
    # **`.split()` and NOT `.strip()`, and the deletion sweep is what settled it.** The
    # only question here is "is there a client at all", and `strip()` asked it in a form
    # no test can pin: `s.strip()` and `s.lstrip()` are truthy for exactly the same
    # strings, so the mutation is invisible — the equivalent mutant `_live_chats`' own
    # docstring names. A bare `.split()` asks the real question once, on tmux's own
    # newline-separated output: no non-whitespace token, no client. It has no half to
    # mutate, and deleting it IS pinnable, because a server answering a bare newline then
    # reads as one client with a blank name.
    if clients.returncode != 0 or not clients.stdout.split():
        return None
    return seat


def _frame_is_live(socket: str, fid: str) -> bool:
    """Is the frame *fid* still running on *socket*? One question, one place — #408.

    A frame is a WINDOW on either server now — one of a workspace session's chats on
    charter's own, one of the operator's own windows in theirs — and `@charter_chat` is
    what it answers to on both, so `_live_chats` is asked either way.
    `tmuxctl.is_operator_socket` is the single question "whose server is this" — since
    #812 a comparison of SERVERS rather than of spellings, so charter's own socket read
    out of `$TMUX` as a path is not mistaken for somebody else's — and it decides what
    the SECOND question is:
    `cmd_respawn` asked `_live_sessions(SOCKET)` unconditionally, which on the operator's
    server is a question about a server that is not theirs: it answers "no such session"
    for a frame that is on screen, so a panel that died there could never have been
    brought back even once the hook reached charter.

    The second question is the old shape's, and both are asked because both can answer:
    a frame launched by a charter that predates chats is still a session named by its id
    on charter's server and a window named by its id in the operator's, and neither of
    those carries `@charter_chat`.

    ``False`` for a server that did not answer at all (`_live_windows`'s ``None``), and
    that direction is deliberate: the only caller is about to RESPAWN something, and
    respawning into a server charter could not reach is the outcome with a cost. Not
    respawning costs a panel that was already dead.
    """
    if tmuxctl.is_operator_socket(socket):
        live = _live_windows(socket)
        if live is None:
            return False
        return fid in live or fid in (_live_chats(socket) or set())
    return fid in _live_sessions(socket) or fid in (_live_chats(socket) or set())


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


def _slot_sizes(fid: str, slots: list[str], *,
                window_rows: int, pane_cols: int) -> dict[str, int]:
    """`layout.slot_sizes` for a frame that has a plane behind it — the only place in
    charter where a committed arrangement becomes a pane height.

    **This function is the boundary, and it is one function because the boundary is one
    line.** `layout` is arithmetic: hand it a row count and a slot list and it answers the
    same thing on every machine, which is what makes `layout.repos_rows` testable with no
    tmux, no cache and no plane. The two facts it cannot derive are what the table's
    content wants (`frame_slots.repos_rows_wanted`, which reads this frame's gather) and
    whether the operator pinned the strip to a height (`layout.pinned_repo_rows`, which
    reads this plane's `charter.toml`). Both are read HERE, once, and passed down.

    #660 put the second read inside `layout.repos_rows` instead, borrowing
    `layout._placed_here`'s "rather than threaded through five signatures". #661 is the
    bill: `repos_rows(content_rows=4, window_rows=50, slots=["top","bottom","repos"])`
    answered `15` on a plane whose `charter.toml` carried `size = 15`, from a file the
    caller never named, in the module's one provably pure function. On this repository
    `charter.toml` is tracked, so committing the line the feature's own news entry tells
    an operator to write turned six tests red for everybody.

    And the five was not this path's number. `_placed_here`'s five is real for
    `_placed_here` — an edge and a cell count for a name `slot_sizes`, `panel_argvs`,
    `repos_cols`, `repos_rows` and `harness_rows` each have to ask about, none of which
    knows it in advance. The pin is one number for one slot with one consumer, so its
    path is two signatures (`slot_sizes`, `repos_rows`) and the three call sites below —
    which were already building `content_rows` the same way three times over, and now do
    not.

    The three are `_launch_sizes` (both launch paths), `_relayout`'s splits for a density
    the frame did not have, and `_reassert_sizes` on every `window-resized`. They differ
    only in how the table pane's width is arrived at, which is why that arrives here as
    *pane_cols* already measured rather than as a window to measure.
    """
    return layout.slot_sizes(
        slots, window_rows=window_rows,
        content_rows=frame_slots.repos_rows_wanted(fid, pane_cols=pane_cols),
        pinned_rows=layout.pinned_repo_rows())


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

    Through :func:`_slot_sizes`, which is where the plane's own pinned height is read and
    why this is not a call to `layout.slot_sizes` directly.
    """
    return _slot_sizes(fid, slots, window_rows=window_rows,
                       pane_cols=layout.repos_cols(slots, window_cols=window_cols))


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
#: The foreground half of it, alone — what a rule is drawn IN before the plane has said
#: anything. Split out because the ``dim`` beside it is a decision now (`[frame] dim`) and
#: the surface behind it always was: `instance.rule_style` composes all three, and handing
#: it the assembled string instead would make it take an attribute back out of one.
_CHROME_FG = "fg=default"

#: The style every rule in the frame is drawn in when the plane has said nothing — and the
#: MARKER in :data:`_CHROME` for "this entry is a style", which is how `_chrome_argvs` knows
#: which of the five options carry a colour without a second list saying so.
#:
#: `instance.rule_style(None, _CHROME_FG, instance.look_of({}))` is this string, and
#: `TheShippedRuleIsStillTheOneCharterAlwaysDrew` asserts it rather than leaving a reader to
#: check by eye — so the day the shipped `Look` changes, the table stops agreeing with the
#: assembler loudly instead of quietly.
_CHROME_STYLE = f"{_CHROME_FG},dim"

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
#: The other two on this list are the same defect wearing different options, and they bite
#: on the OPERATOR'S server where their own `.tmux.conf` is what charter would otherwise
#: inherit (measured, with a hostile config on a real 3.7c): `pane-border-lines
#: double`/`heavy`/`number` redraws every rule in a different weight (`number` writes pane
#: NUMBERS into them); and `pane-border-status top` is the worst of them — it turns every
#: border into a title bar carrying `#{pane_title}` (the machine's hostname, by default)
#: AND adds a border row above the topmost pane, a row `layout._BORDER_ROWS` never
#: budgeted for.
#:
#: **`pane-border-indicators` is pinned to `arrows`, and it used to be pinned to `off`.**
#: That is #750, and the correction is a measurement rather than a change of taste. With
#: the shipped `chrome = "off"` there is no surface, so `window-active-style` has no shade
#: to be one step from and the two rule styles are pinned to one value on purpose — which
#: left the frame with **nothing at all** saying which pane the keyboard is in. That
#: matters more than it sounds, because `F12` exists for "you are in a pane that is not
#: answering" and the frame gave no way to notice you were in one.
#:
#: The reason it was `off` was that `arrows` "puts a glyph on the active pane's borders
#: and no others, so one rule carries a glyph its neighbour does not". That is true, and
#: read off an attached client's wire through a nested tmux on 3.7c it is not #514's
#: defect — it is the cue. #514 was a rule whose COLOUR changed mid-line; the arrow is a
#: glyph substituted into a rule that keeps one style along its whole length. Measured,
#: charter's own shape (a harness beside a sidebar, a footer under both), the bottom rule
#: in full::
#:
#:     harness active   ESC[2m ─↑─────────────────────────────────┴──────────────
#:     sidebar active   ESC[2m ───────────────────────────────────┴─↑────────────
#:     footer active    ESC[2m ─↓─────────────────────────────────┴─↓────────────
#:
#: One `ESC[2m` at the start of the row and none anywhere else in it: the rule is the same
#: dim default-foreground rule charter has drawn since #514, and the only thing that moved
#: is which cell holds an arrow. The vertical divider carries `←`/`→` the same way.
#:
#: **And it costs nothing on the frame that already has an answer.** Over a surface with
#: the shipped `rules = "hidden"` the rule is `fg=<surface>,bg=<surface>` — the glyph IS
#: the background — so the arrow disappears exactly where `window-active-style`'s one-shade
#: step is already saying which pane is live. The cue appears where there is no other, and
#: nowhere else, without a word being read to decide it.
#:
#: What is lost below `tmuxctl.BORDER_INDICATORS_FLOOR` is the cue and not the frame: tmux
#: 3.2 has no such option, charter does not issue it there, and that plane gets the frame
#: it has today. Which is the third field's whole job.
#:
#: `pane-border-format` is deliberately NOT here: it is inert while `pane-border-status`
#: is `off`, and pinning a format nothing renders would be pinning a spelling rather than
#: the property.
#:
#: **The two style entries are where the frame's SURFACE joins its rules**, and the value
#: written here is the one they carry when there is no surface. `_chrome_argvs` appends
#: `instance.border_bg`'s background clause to whichever entries are pinned to
#: `_CHROME_STYLE` — derived from this table the way `instance.chrome_option_names`
#: derives its own, so a sixth border-style option added here is not the one rule left
#: with a seam through it.
#:
#: **The third field is the oldest tmux charter will issue that option to, and it is a
#: FIELD rather than a check for #716's reason.** `pane-border-indicators` does not exist
#: at `tmuxctl.FLOOR` — it arrived in 3.3 — and pinning a name tmux does not have is not
#: degraded but refused, so this table shipped for a release making **every launch on the
#: supported floor** print `charter frame: styling the frame's own rules failed — … invalid
#: option: pane-border-indicators`. That was the third time charter got this wrong in one
#: file: `pane-border-style` at pane scope has `tmuxctl.PANE_BORDER_FLOOR` and
#: `window-resized` has `tmuxctl.RESIZE_HOOK_FLOOR`, each a bespoke check of its own, and
#: a third bespoke check would leave the FOURTH just as easy to add unguarded. A row here
#: cannot be written without a floor, `_chrome_argvs` cannot read one without applying it,
#: and `tests/test_frame_tmux_integration.py`'s
#: `EveryBorderOptionThisTmuxHasIsPinned` measures every floor in it against the binary
#: it is running on — in both directions, so a floor set too high goes red on the tmux
#: that has the option and a floor set too low goes red on the tmux that does not.
#:
#: `tmuxctl.FLOOR` means "everywhere charter supports", and it is measured rather than
#: assumed: all four of these answer `show -w` with rc 0 on a real tmux 3.2.
_CHROME: tuple[tuple[str, str, tuple[int, int]], ...] = (
    ("pane-border-style", _CHROME_STYLE, tmuxctl.FLOOR),
    ("pane-active-border-style", _CHROME_STYLE, tmuxctl.FLOOR),
    ("pane-border-indicators", "arrows", tmuxctl.BORDER_INDICATORS_FLOOR),
    ("pane-border-lines", "single", tmuxctl.FLOOR),
    ("pane-border-status", "off", tmuxctl.FLOOR),
)


def _chrome_values() -> dict[str, str]:
    """`{option: charter's answer}` for every row of :data:`_CHROME`, floors dropped.

    The one reading of that table that is about WHAT charter pins rather than about which
    tmuxes get told — `dict(_CHROME)` said once, now that a row is three fields wide, so
    the callers that only want the two do not each re-spell the unpacking.
    """
    return {name: value for name, value, _floor in _CHROME}


def _chrome_argvs(*, socket: str, harness_pane: str, v: tuple[int, int] | None,
                  surface: str | None = None,
                  look: instance.Look = instance.SHIPPED_LOOK) -> list[list[str]]:
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

    ***surface* is `instance.border_bg`'s background clause, and it closes the SEAM.**
    `window-style` paints a pane's interior and the rule between two panes is not in any
    pane, so a frame with every panel painted grey came out as grey rectangles separated
    by a one-cell strip of the terminal's own black — reported off a screenshot, and read
    back off an attached client's wire as an `\\x1b[49m` in the middle of two `\\x1b[100m`
    runs. Appending the same background to the rule removes that `\\x1b[49m` and the
    surface runs straight through (measured on 3.7c and on `tmuxctl.FLOOR`; both accept
    the combined value and read it back verbatim). `instance.border_bg` decides WHICH
    colour and argues why; this decides only that the rule is drawn in it.

    **Both styles, again, and for #514's reason rather than by symmetry.** The whole
    defect that put these two options here is a rule that changed colour where it passed
    the active pane's corner, and a BACKGROUND that changed there would be the same defect
    an order of magnitude more visible. So the surface goes on both or on neither, and the
    active-pane indication tmux's own default carries — `pane-active-border-style`
    defaults to the format expression
    ``#{?pane_in_mode,fg=yellow,#{?synchronize-panes,fg=red,fg=green}}``, measured — stays
    discarded, as it has been since #514. Focus is drawn where it cannot run past a
    corner: on the pane's own rectangle, by `window-active-style`
    (`instance.FRAME_PANE_BG` pairs every colour with its focused shade).

    **Which options carry it is derived, not listed**: the ones already pinned to
    `_CHROME_STYLE` are exactly the styles, which is `instance.chrome_option_names`'
    discipline said about this table. `pane-border-lines` and the rest are not styles and
    take no colour.

    ``None`` is the frame that had no surface, and then every value here is `_CHROME`'s
    own — byte-identical to what charter emitted before this parameter existed, which is
    what makes `chrome = "off"` a REMOVAL on the live path too: `cmd_chrome` re-issues
    these `set-option`s and the coloured value is replaced by the uncoloured one. These
    two options are charter's at every level (#514 pins them with or without a surface),
    so there is nothing here to `-u`.

    **`NO_COLOR` refuses the surface and keeps the rules**, which is the split that
    matters: `_CHROME_STYLE` is an attribute over the terminal's own foreground and never
    a colour charter chose, so it is not what `NO_COLOR` is about, while a background is
    exactly what it is about. Asked through `chrome.no_colour` so there is one reading of
    the variable (#547), the same way `_surface_argvs` asks.

    **It refuses the plane's own `text` colour by the same line**, and that falls out of
    the surface being refused rather than needing a second answer: with no surface,
    `instance.rule_style` has nothing to hide a rule in, and the foreground it composes is
    the only thing `NO_COLOR` would still have to argue about. It does not — a `text` word
    IS a colour charter was asked to paint, so it goes with the background. What survives
    is `_CHROME_FG` and the `dim`, which are the operator's own foreground and an
    attribute over it, exactly as before.

    ***look* is the plane's own `[frame] rules`, `text` and `dim`** — the frame-wide
    appearance, resolved once by `instance.look_of` and passed whole rather than as three
    keywords, so this and the three functions below cannot be handed different halves of
    one frame's answer. `instance.rule_style` is where all three are spent; nothing here
    reads a word out of it.

    **It is a different question from *v* and the two do not meet.** A floor says which
    tmuxes are told about an option at all; a `look` says what the two style rows carry
    when they are. `pane-border-indicators` is dropped below its floor whatever the plane
    asked for, and a `hidden` rule is composed identically on every tmux that gets one —
    which is why the `>= floor` filter below is applied to the row and never to the value.

    ***v* is this tmux's version, and every row of `_CHROME` is filtered through its own
    floor by it (#716).** REQUIRED rather than defaulted, because a default is the shape
    the defect had: `pane-border-indicators` does not exist below
    `tmuxctl.BORDER_INDICATORS_FLOOR`, and a caller that could forget to say which tmux it
    is talking to is a caller that goes on issuing it there. Both production call sites
    already hold the answer for `_pane_borders_wanted`, so this asks for nothing new — it
    only stops the question being skipped.

    ``None`` — charter could not read a version — is taken as `tmuxctl.FLOOR`, and that is
    the opposite of `_pane_borders_wanted`'s ``None`` for a stated reason rather than by
    inconsistency. There the unknown answers *False* because the failure below its floor is
    SILENT and destructive (a `set -p` that writes the window). Here every failure is loud
    and harmless — `run` reports it and the launch continues — so the useful answer is the
    supported baseline: pin everything every tmux charter supports has, and leave the one
    option that arrived after it to a tmux that says so.
    """
    rule = instance.rule_style(
        None if chrome_mod.no_colour() else surface, _CHROME_FG, look)
    known = v if v is not None else tmuxctl.FLOOR
    return [tmuxctl.server_argv(socket, "set-option", "-w", "-t", harness_pane, name,
                                rule if value == _CHROME_STYLE else value)
            for name, value, floor in _CHROME if known >= floor]


def _pane_borders_wanted(v: tuple[int, int] | None) -> bool:
    """Can this tmux give a pane its OWN edges — `tmuxctl.PANE_BORDER_FLOOR`, asked once.

    ``None`` is "charter could not find out", and it answers **False** for the reason the
    floor constant gives at length: below the line `set -p pane-border-style` is not
    refused, it is rc 0 and writes the WINDOW. A wrong guess in that direction is not a
    cosmetic loss but the last panel written deciding every rule in the frame, and an `off`
    removing charter's own #514 pin window-wide. The frame-wide answer is correct on every
    tmux; the per-pane one is correct only above the floor, so the unknown takes the one
    that cannot be wrong.

    A version and not a probe, because the failure below the line is SILENT: there is no
    refusal to catch, and a probe would have to perform the damaging write to discover it
    (`tmuxctl.PANE_BORDER_FLOOR`).
    """
    return v is not None and v >= tmuxctl.PANE_BORDER_FLOOR


def _pane_border_pairs(*, chrome, bg, pane_borders: bool,
                       look: instance.Look = instance.SHIPPED_LOOK
                       ) -> tuple[tuple[str, str], ...]:
    """This PANEL pane's own edges, in this panel's own surface — or nothing at all.

    **A panel's edges are its own colour, which is the half of #631 that stands.** A
    window-wide border surface (`instance.border_bg`, which is what #628 shipped) is one
    value for every rule in the frame, so an arrangement whose components name different
    backgrounds got rules in a colour some of its panels were not. Above
    `tmuxctl.PANE_BORDER_FLOOR` a panel carries its own instead, so every rule cell a panel
    owns matches the panel it belongs to.

    **The harness's edges are a different question and are answered separately**
    (`_harness_rule_argvs`). #631 answered it by leaving them unset, which is where the
    frame's surface stopped: tmux resolves every border cell around the harness against the
    harness's own options, so the horizontal rule under the identity bar came out dark for
    the cells over the harness and surfaced for the cells over the sidebar. Rendered,
    charter's real four-panel shape at 100x24 on tmux 3.7c, every panel
    `bg = "brightblack"`, read through a nested client::

        window-wide (#628)  harness top ESC[100m         right ESC[100m  panel|panel ESC[100m
        per pane    (#631)  harness top ESC[49m          right ESC[49m   panel|panel ESC[100m
        with the rules      harness top ESC[100m         right ESC[100m  panel|panel ESC[100m
                            ^ and its INTERIOR still ESC[49m: `window-style` is not one of
                              these two names, so nothing here or there can reach it.

    *pane_borders* is `tmuxctl.PANE_BORDER_FLOOR` already answered by the caller, which is
    where the tmux version is known. Below it these two names are WINDOW options wearing a
    pane's clothes: `set -p` is rc 0 and writes the window, so the last panel written would
    decide every rule in the frame, and `set -p -u` would remove charter's #514 pin
    window-wide. Nothing is emitted there and `_chrome_argvs` carries the frame-wide answer
    instead — one of the two, never both, so the two designs cannot half-apply.

    `instance.pane_border_options` builds the values from charter's own tables and
    `_CHROME_STYLE`, so no operator string reaches a style here any more than it does one
    function up.
    """
    return (instance.pane_border_options(bg, chrome, _CHROME_FG, look)
            if pane_borders else ())


def _harness_rule_argvs(*, socket: str, harness_pane: str, surface: str | None,
                        pane_borders: bool,
                        look: instance.Look = instance.SHIPPED_LOOK) -> list[list[str]]:
    """`set-option -p`: the rules AROUND the harness pane, in the frame's own surface —
    and never, at any value, anything that reaches INSIDE it.

    **The seam #631 left, and the boundary that survives closing it.** A border cell is not
    in either pane it separates, and above `tmuxctl.PANE_BORDER_FLOOR` tmux resolves each of
    them against exactly one: `screen_redraw_check_cell` walks the window's panes in order
    and takes the first whose border box contains the cell, and the harness is the first
    pane charter's window has. So all three of the harness's edges — its top, its right and
    its bottom — are drawn from the harness's own two options, and #631 leaving them unset
    put the terminal's own background on a rule that runs on past the harness's corner over
    a panel that IS surfaced. Measured on 3.7c through a nested client, four panels all
    `bg = "brightblack"`, at 100x24::

        #631        row 1: cols 0-77 ESC[49m  cols 78-99 ESC[100m   <- one rule, two colours
        with this   row 1: cols 0-99 ESC[100m                       <- one rule, one colour

    That is #514's own defect — a rule that changes colour where it passes a corner — and
    the operator has reported it off a screenshot three times.

    **Both options, and #514's own four-focus-state measurement re-run for this pane.** The
    harness is a pane that can be ACTIVE, and tmux draws a border cell from
    `pane-active-border-style` when it touches the active pane — so a harness whose two
    differed would put the defect back on the very rules this closes. They cannot differ:
    `instance.rule_options` builds both from one value. Measured on 3.7c through a nested
    client with the rules armed, cycling harness → each of the three panels → harness, every
    rule cell came back `ESC[100m` in all five readings and the frame did not move when
    focus did.

    **This names the harness, and what that costs is one option name.** #631's construction
    was that nothing needed to: `_surface_argvs` is only ever handed a panel, so the harness
    was protected by never being an argument. That construction protected two things at
    once, and only one of them was ever the report. The one that matters is the harness's
    INTERIOR — `window-style` and `window-active-style`, the rectangle ADR 0018 says charter
    "never decides what a cursor or a colour means inside" — and it is still protected by
    construction here: this function's option names come from
    `instance.PANE_BORDER_OPTIONS`, which does not contain either of them, and
    `_surface_argvs`, which does set them, is still only ever handed a panel. The other
    thing #631 protected was the harness's border cells, which were never inside anything:
    charter has drawn their foreground, their dim attribute, their line weight, their
    indicators and their border-status since #514 (`_CHROME`, set window-wide on this very
    pane). Withholding only their BACKGROUND is half a cell, and the half it left behind is
    the seam.

    ***surface* is `instance.agreed_border_bg`, and `None` is a real answer** rather than a
    missing one: an arrangement whose panels wear more than one colour has no colour all
    three of the harness's neighbours share, so any value would be a cell matching neither
    pane beside it. There the rules stay bare, which is still one of the two panes they
    touch.

    **The unset is why this is one function and not a launch/live pair, and the cost of
    that is measured rather than waved at.** `_surface_argvs` and `_resurface_argvs` are
    split because on a LAUNCH `off` is free: the panes they write are ones `split-window`
    just created, so there is nothing on them to remove. The harness pane is not one of
    those on either path that reaches here — `_split_panels` runs again on every density
    change, and `cmd_chrome` runs on a frame that has been surfaced for hours — so "no
    surface" has to mean *remove what is there* on both, or `chrome: off` is a keypress that
    reports success and leaves the operator looking at the surface they turned off.

    `set-option -p -u` on an option that was never set is rc 0 (measured on 3.7c and at
    `tmuxctl.FLOOR`, `_resurface_argvs`' own measurement), so the launch path can pay the
    removal instead of a second function. What it pays is two commands at a measured 6.0ms
    each on this machine — 12ms, on the launches where the frame has no surface at all,
    against the roughly fifteen tmux calls a launch already makes and the 22.2ms
    `_chrome_argvs` alone costs. A pair would save that and buy back the failure this whole
    area has already paid for twice (#547, #610): two functions answering one question, with
    the launch and the keypress free to answer it differently. It would also be WRONG on the
    density path, where a `charter.toml` re-read between launch and keypress can take the
    surface away and only the removal would notice.

    *pane_borders* is `tmuxctl.PANE_BORDER_FLOOR`, and the gate is on the unset exactly as
    it is on the set. Below the line these two names are window options wearing a pane's
    clothes: `set -p` is rc 0 and writes the WINDOW, so a `-u` here would remove charter's
    own #514 pin for every rule in the frame — silently, with nothing for `tmuxctl.run` to
    report. Nothing at all is emitted there and `_chrome_argvs` carries the frame-wide
    answer instead, which already reaches these same cells.

    **`NO_COLOR` refuses the surface and keeps the rules**, `_chrome_argvs`' split word for
    word: `_CHROME_STYLE` is an attribute over the terminal's own foreground and is not what
    that variable is about, a background is exactly what it is about, and here refusing it
    means the unsets rather than silence — a frame surfaced before `NO_COLOR` was exported
    has a value on this pane to remove. Asked through `chrome.no_colour` so there is one
    reading of the variable (#547).
    """
    if not pane_borders:
        return []
    pairs = instance.rule_options(None if chrome_mod.no_colour() else surface,
                                  _CHROME_FG, look)
    if not pairs:
        return [tmuxctl.server_argv(socket, "set-option", "-p", "-u", "-t", harness_pane,
                                    name) for name in instance.PANE_BORDER_OPTIONS]
    return [tmuxctl.server_argv(socket, "set-option", "-p", "-t", harness_pane, name, value)
            for name, value in pairs]


def _surface_argvs(*, socket: str, pane_id: str, chrome, bg=None,
                   pane_borders: bool = False,
                   look: instance.Look = instance.SHIPPED_LOOK) -> list[list[str]]:
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

    ***bg* is this component's own word, and it WINS WHOLE where it is given.** It is the
    key `[frame] chrome` could not be: the operator set `chrome = "dark"` on a terminal
    that was already black and reported panes indistinguishable from the terminal and from
    each other — which is what a single frame-wide word can always produce, since whatever
    it says it says about every pane at once. A frame reads as an application because its
    regions are told apart.

    Whole rather than merged, and that is the half worth stating: a *bg* that set only
    `window-style` would leave `window-active-style` carrying the frame-wide chrome's
    colour, so one pane would be two unrelated colours depending on focus — a cell's worth
    of the two-colour defect #514 fixed on the borders. `instance.pane_bg_options` answers
    both options or neither, so a pane is one word's answer or the other's and never a
    blend of them.

    It is the same containment one key over: the operator's word is a KEY into
    `instance.FRAME_PANE_BG` and the value that reaches tmux comes out of charter's own
    table. A word charter does not know yields nothing here — and the arrangement carrying
    it was already refused whole at the config boundary, so this is the second of two
    answers rather than the only one.

    **It does not need `chrome` to be on.** `chrome`'s default is `off` because a default
    that repaints a stranger's terminal makes a working frame worse on upgrade; a *bg* is
    not a default, it is a line somebody wrote by hand about one pane. So a plane can leave
    `chrome = "off"` and still colour its sidebar, which is the smallest way to answer the
    report that started this.

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
    # `or`, not a merge: `pane_bg_options` answers `()` both for a component that named no
    # background and for one whose word charter does not know, and both mean "this pane
    # takes the frame's own answer". One expression, so there is no state in which a pane
    # gets one option from here and the other from there.
    return [tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane_id, name, value)
            for name, value in (*instance.surface_options(bg, chrome, look),
                                *_pane_border_pairs(chrome=chrome, bg=bg, look=look,
                                                    pane_borders=pane_borders))]


def _panel_mark_argv(*, socket: str, pane_id: str) -> list[str]:
    """`set-option -p`: say that THIS pane is a panel charter split off, and no other.

    The write half of :data:`_PANEL_OPTION`; `conf_text`'s `MouseDown1Pane` bind is the
    read half, and between them they are the whole of why a click on a panel does not take
    the keyboard off the harness once `[frame] mouse = true` (#634).

    **Beside `_surface_argvs` and issued from the same funnel, for the same measured
    reason** — `_split_panels` is the one place every panel pane charter creates comes out
    of, so both launch paths and every density change mark a pane as it appears rather
    than by a second pass that could forget one. A pane a later `_relayout` adds is marked
    when it is created; a pane `cmd_respawn` brings back keeps the mark, because a respawn
    reuses the same `%N` and this is a property of the rectangle rather than of the process
    in it — the argument `_surface_argvs` already makes one option over.

    **Unlike `_surface_argvs` it is NOT gated on `NO_COLOR`, and that difference is the
    point.** A surface is something an operator can ask charter not to paint. This is
    routing: it decides where a click goes and where the keyboard stays, and an operator
    who set `NO_COLOR` asked for no colour, not for their clicks to start moving the
    keyboard. Nothing here reaches a screen.

    **Written on BOTH servers, and inside the operator's own tmux it marks a pane nothing
    reads.** `conf_text` is sourced only against charter's private `SOCKET` — charter does
    not rebind keys in a server it does not own, and `_launch_in_operator_tmux` is
    explicit that it must not — so inside an operator's tmux this is a user option on a
    pane charter created, carrying charter's namespace, dying with the pane, and changing
    nothing. That is the same reach `_surface_argvs` and `_panel_remain_on_exit_argv`
    already have there. It is written on both anyway rather than gated on which server,
    because a mark that existed on one is a second thing for `_split_panels` to be right
    about, and the value of getting it wrong is a focus steal that only reproduces on one
    of two paths.

    A plain `list[str]`, never ``None``: both arguments are charter's own — *pane_id* is a
    `%N` this module just read back off `split-window` and held to `_PANE_ID_RE` before it
    got here, and the name and value are the two constants above — so there is no value
    this can be handed that it would have to decline.
    """
    return tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane_id,
                               _PANEL_OPTION, _PANEL_MARK)


def _panel_slot_argv(*, socket: str, pane_id: str, slot: str) -> list[str] | None:
    """`set-option -p`: say WHICH component this panel draws — or ``None`` for a name
    charter will not put on a pane.

    The write half of :data:`_PANEL_SLOT_OPTION`, issued from `_split_panels`' funnel one
    line after :func:`_panel_mark_argv` and for the same funnel reason: a panel that
    reached the screen without it is a panel `_reconcile_panels` cannot name, and a panel
    charter cannot name is one it will never kill (see there).

    **Unlike :func:`_panel_mark_argv` this one CAN decline, because its value is not
    charter's own constant.** A slot name arrives from `[[frame.component]]` — a committed
    file — by way of a density level's list or `_drawable_slots`, and every guard between
    here and there is a guard on a different question (`frame_slots.drawable` asks whether
    anything can DRAW it). `component.usable_id` is the alphabet `frame/component.py`
    holds an id to before it may reach a `bind` line, asked here rather than re-spelled,
    so a name charter would not let near tmux's parser does not get onto a pane either.

    Declining is not silent about its consequence: the pane is still split, still marked,
    still drawn and still recorded — it is only unnameable to a later reconciliation,
    which is exactly the position every pane split by a charter older than #714 is in, and
    :func:`_reconcile_panels` treats both the same way. That is the safe direction: a pane
    charter cannot identify is left running, never guessed at and never killed.
    """
    if not component.usable_id(slot):
        return None
    return tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane_id,
                               _PANEL_SLOT_OPTION, slot)


def _resurface_argvs(*, socket: str, pane_id: str, chrome, bg=None,
                     pane_borders: bool = False,
                     look: instance.Look = instance.SHIPPED_LOOK) -> list[list[str]]:
    """:func:`_surface_argvs` for a pane that is ALREADY DRAWN — with the unsets.

    **The difference is `off`, and it is not a detail.** On a launch, `off` is free:
    nothing is set, so `_surface_argvs` correctly issues no command. On a running frame
    the same word has to mean *remove what is there*, or the palette row that says
    ``chrome: off`` leaves the operator looking at the surface they just turned off — a
    keypress that reports success and changes nothing, which is the shape this spec's own
    §4 refuses an ``auto`` value for.

    So: every option the new word does NOT set is unset, and every option it does set is
    set. Which options exist comes from `instance.chrome_option_names`, derived from the
    table rather than spelled here, so a third option added to `FRAME_CHROME` is unset by
    this function on the day it is added instead of surviving an `off` nobody re-read.

    `set-option -p -u` is the removal, and it is measured rather than assumed on both
    tmux 3.7c and tmux 3.2 (`tmuxctl.FLOOR`): rc 0, `show -p` reads back `''` afterwards,
    and unsetting an option that was never set is rc 0 as well — so this needs no "was it
    set" round trip, which would be a subprocess per pane per keypress to answer a
    question the unset already answers.

    ***bg* is this pane's own colour and it wins here exactly as it wins at launch**, and
    that agreement is the whole reason it is a parameter rather than something this
    function looks up. `_surface_argvs` resolves `pane_bg_options(bg) or
    chrome_options(chrome)`; if this path resolved only the frame-wide word, the two would
    be two answers to one question — a pane painted one way when the frame launched and
    another way the moment somebody pressed a palette row (#547's shape, on a surface the
    operator can see).

    What that costs if it is missed is specific: a component with `bg = "brightblack"`
    would be repainted in the frame-wide colour by `chrome: light`, and **erased** by
    `chrome: off` — the unsets below remove exactly the two options a per-pane colour is
    made of — with nothing to bring it back until the frame is relaunched. A committed
    value silently undone by a keystroke.

    So a pane whose component named a colour has that colour SET here and nothing unset;
    a pane whose component named none takes the frame's word, including `off`'s removal.
    `NO_COLOR` still beats both, and on this path it means the unsets rather than silence.

    **Never the harness pane**, exactly as `_surface_argvs` is never handed one: the
    caller (`cmd_chrome`) iterates `state.panes`, which is the PANEL map, and the harness
    pane lives in a different record entirely (`state.harness_pane`). ADR 0018's boundary
    holds here by the same construction.

    **`NO_COLOR` wants NOTHING SET, which on this path means the unsets and not silence.**
    `_surface_argvs` answers `[]` there because a launch under `NO_COLOR` has nothing to
    remove. A frame surfaced before `NO_COLOR` was exported has, and answering `[]` here
    would honour the letter of the promise while leaving charter's colour on the screen —
    which is the half of that promise this spec was written about. Asked through
    `chrome.no_colour` so there is one reading of the variable (#547).
    """
    want = dict(() if chrome_mod.no_colour()
                else (*instance.surface_options(bg, chrome, look),
                      *_pane_border_pairs(chrome=chrome, bg=bg, look=look,
                                          pane_borders=pane_borders)))
    # The removal reaches the pane's own EDGES too, and only where charter ever wrote
    # them: below `tmuxctl.PANE_BORDER_FLOOR` a `set -p -u` on these two names removes the
    # WINDOW's value — charter's own #514 pin, for every rule in the frame — so the gate
    # is on the unset exactly as it is on the set. `instance.PANE_BORDER_OPTIONS` is the
    # one list both halves read.
    removable = (*instance.chrome_option_names(),
                 *(instance.PANE_BORDER_OPTIONS if pane_borders else ()))
    argvs = [tmuxctl.server_argv(socket, "set-option", "-p", "-u", "-t", pane_id, name)
             for name in removable if name not in want]
    argvs += [tmuxctl.server_argv(socket, "set-option", "-p", "-t", pane_id, name, value)
              for name, value in want.items()]
    return argvs


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


def _launch_in_operator_tmux(socket: str, session: str, *, ws: str,
                             argv: list[str], h, v: tuple[int, int],
                             picked: bool) -> int | None:
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
    # Both lists, and the same refusal `_reap_this_server` makes: a chat's directory is
    # kept by `@charter_chat` alone (no launcher pid abstains in its favour), so a server
    # that answered for the window NAMES and would not answer for the chats is one
    # charter cannot reap on.
    chats_before = _live_chats(socket)
    if chats_before is not None:
        state.reap(live_before | chats_before, server=socket)

    # AFTER the reap, never before it: allocation claims a directory by creating it
    # (`state.new_chat_id`), and a `reap` that ran afterwards would see a directory with
    # no window of its own yet and delete it out from under this very launch. The same
    # ordering, for the same reason, as the private-server path's.
    fid = state.new_chat_id(ws)
    if fid is None:
        util.err(f"charter frame: could not open a chat in workspace {ws!r} — its state "
                 f"directory could not be created")
        return 1
    _pin_workspace(ws, fid, picked)
    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1
    # The recycled-pid adoption #383 fixed, kept as belt and braces on both paths now
    # that it cannot happen. A frame id WAS `<workspace>-<launcher pid>`, `reap` keeps
    # such a directory while that pid is live, and on a launch it was live because it was
    # ours — so an earlier launcher for this workspace that landed on the same pid left
    # its whole directory to be adopted. `state.new_chat_id` claims its ordinal with a
    # `mkdir` that FAILS when the name is taken, so a launch cannot land on an occupied
    # directory at all. These four calls therefore have nothing to clear today; they are
    # kept because Stage 5c reopens a COLD chat into its own existing directory, which is
    # the case they are actually for (`state.clear_shape`, #413).
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
    # And WHERE, which until now was read on this line, handed to tmux and dropped
    # (`state.record_cwd`). It is the fourth of §4e's four restore items and the only one
    # with nowhere to live: `workspace` is a file, `identity` carries the harness and the
    # persona pin, and a cwd could not join `identity` because every value in that record
    # goes onto a world-readable tmux `-e` argv. Recorded on BOTH launch paths, from the
    # same `os.getcwd()` each one already reads, because a chat reopened out of the wrong
    # directory is a harness given a different plane to look at.
    state.record_cwd(fid, cwd)
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
    # What every later lookup asks instead of parsing this window's name — the liveness
    # list `_reap_this_server` reads, and the same option `@charter_hatch` already uses.
    # A window name is not an identity; see `_CHAT_OPTION`.
    named = _chat_option_argv(socket=socket, harness_pane=harness_pane, chat=fid)
    if named is not None:
        tmuxctl.run("naming the chat on its window", named)

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
    # Read before the select takes the operator off it, for the private path's reason
    # (#688). Ordinarily nothing: the window they are on is one of THEIR windows and
    # carries no `@charter_chat`, so this answers `""` and nothing of theirs is touched —
    # the same promise every other line on this path makes. It is a chat only when the
    # operator already had a charter frame open in this session, which is exactly the
    # case the rule is about.
    leaving = _chat_being_left(socket, beside=fid)
    # `select-window`, never `attach`: the operator has a client already, on this very
    # server. A second attach IS the nesting this path exists to remove.
    selected = tmuxctl.run("switching to the frame",
                           tmuxctl.server_argv(socket, "select-window", "-t", window_id))
    if selected.returncode == 0:
        _drop_panels(socket, leaving)

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
    # Given up before this path's own closing reap, for the reason `cmd_launch` gives at
    # its own (#685): the marker is held for the LAUNCH, and this launch is over.
    state.clear_claim(fid)
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

    The chat ids are unioned in for `_live_chats`' own reason, and it matters MORE here
    than on charter's own server rather than less: this is somebody else's tmux, where
    `allow-rename on` is an ordinary thing to have in a `.tmux.conf`, and a frame whose
    window the harness has renamed is a frame this function would otherwise reap while
    the operator was looking at it.
    """
    live = _live_windows(socket)
    chats = _live_chats(socket)
    if live is not None and chats is not None:
        state.reap(live | chats, server=socket)


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


def _window_panels(socket: str, harness_pane: str) -> dict[str, str | None] | None:
    """What panes *harness_pane*'s WINDOW actually has, and which component each of
    charter's own draws — **asked of tmux, never of `state.panes`** (#714).

    ``{pane id: mark}`` in tmux's own order, with three distinguishable answers per pane
    and a fourth for the whole window:

    * ``None`` — this pane does not carry :data:`_PANEL_OPTION`. It is the harness's, the
      palette overlay's (`frame/overlay.py` splits one and never marks it), or one the
      operator split themselves. **Not charter's to kill**, and that is the single
      discriminator constraint 1 of #714 rests on.
    * ``""`` — a panel charter split, by a charter that did not yet write
      :data:`_PANEL_SLOT_OPTION` (anything before #714), or one whose slot name
      :func:`_panel_slot_argv` declined. Charter's pane, and charter cannot say which
      component it is.
    * ``"<id>"`` — a panel charter split for that component, said by the pane itself.
    * ``None`` from the whole function — charter could not ask. `list-panes` exited
      non-zero (a harness pane that has gone), or answered something that is not this
      window's pane list. Callers fall back to the record, which is exactly the behaviour
      that shipped before this function existed.

    **The window, and only the window.** `list-panes -t %N` resolves a pane target to its
    containing window on tmux 3.7c and at `tmuxctl.FLOOR` alike — measured by adding a
    second window to the session and re-reading, which returned the same three lines — so
    a frame with several chats reconciles one chat's window per call and cannot reach
    another's. That scoping is the same one `_install_resize_hook` relies on, and it is
    what makes this safe to run inside an operator's own tmux at all.

    **The harness pane is the proof that the answer is real.** tmux lists the window
    containing the target, so the target is always in its own answer; a reply that does
    not contain it is not this window's pane list, whatever its exit code said. That is
    not belt and braces over the return code — it is what tells a truthful empty window
    (impossible) from a stub, a truncated read, or a `tmuxctl.run` that answered something
    else, and it is what makes the whole reconciliation degrade to "charter cannot tell"
    rather than to "charter's window has no panes", which would be a licence to split
    duplicates and a licence to kill.

    **The mark is compared against the constant, not read as a tmux truth-value, and the
    asymmetry with `conf_text`'s bind is deliberate.** That bind is generous — `2`, `on`
    and even `off` all read TRUE (see :data:`_PANEL_MARK`) — because being wrong there
    costs a click going to the wrong pane. Being wrong here costs a pane. `_PANEL_MARK` is
    charter's own literal, written by exactly one function, so a pane carrying anything
    else is a pane charter did not write and the safe answer is that it is not charter's.

    `report=False`: a frame whose harness pane has gone is a frame this cannot be asked
    about, and both callers are on paths (`cmd_resize`'s hook child, a `run-shell`
    keypress) whose only remaining screen is the agent's own.
    """
    p = tmuxctl.run("asking which panes this frame's window already has",
                    tmuxctl.server_argv(socket, "list-panes", "-t", harness_pane,
                                        "-F", _PANEL_LIST_FORMAT),
                    report=False)
    if p.returncode != 0:
        return None
    found: dict[str, str | None] = {}
    for line in p.stdout.splitlines():
        # Exactly three, split on the single space the format joins them with — see
        # :data:`_PANEL_LIST_FORMAT` for why that is a property of the two alphabets
        # rather than an assumption about the values.
        fields = line.split(" ")
        if len(fields) != 3:
            continue
        pane_id, mark, slot = fields
        # #475's rule on the way IN from tmux's stdout, the same way `_split_panels`
        # applies it to `split-window`'s: these ids become `kill-pane -t` arguments below.
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        if mark != _PANEL_MARK:
            found[pane_id] = None
            continue
        found[pane_id] = slot if component.usable_id(slot) else ""
    if harness_pane not in found:
        return None
    return found


def _reconcile_panels(socket: str, *, harness_pane: str, want: list[str],
                      panels: dict[str, str]) -> dict[str, str]:
    """Make the panes this window HAS agree with the components *want* names, and answer
    with the ``{slot: pane id}`` that survived. **The whole of #714.**

    Two directions, one walk, and they were two different bugs with one cause:

    * a component `want` names that already has a pane nobody recorded is **adopted**,
      instead of getting a second pane split for it. Six panel panes where there should
      have been two, each holding a ~24 MB `charter panel` process and each drawing
      correct content, is what the other answer looked like on a real frame;
    * a component `want` no longer names loses its pane, **whether or not the record
      still points at it**. That direction is how the first orphans were made: a
      `charter.toml` edited twice in a session left `state.panes` naming only the newest
      pane per component, so the previous one was already unreachable to the kill loop
      that ran here, to `_drop_panels`, and to every other reader of `state.panes`.

    **Why the record cannot be the authority, stated once.** `state.record_panes` writes
    the map whole on every re-layout. Splitting a second pane for a component therefore
    *deletes* the first one's id — not corrupts it, deletes it — and after that no reader
    that goes through `state.panes` can see the pane at all. A reconciliation built on the
    record would be built on the thing that is wrong. So the window is read
    (:func:`_window_panels`) and the record is demoted to what it is good for: naming
    panes split by a charter that predates :data:`_PANEL_SLOT_OPTION`, and choosing WHICH
    of several panes for one component is the one to keep.

    **What may be killed, as a rule rather than a list.** A pane is killed only if charter
    itself said so — either the pane carries :data:`_PANEL_SLOT_OPTION` naming a component
    (tmux's word), or `state.panes` names it for a slot (charter's own record of a
    `split-window` it ran). Everything else is left running: the harness pane, the palette
    overlay, a pane the operator split, and — the case worth naming — a pane carrying
    :data:`_PANEL_OPTION` but no component id, which is every panel of a frame launched by
    a charter older than this one and not otherwise recorded. Charter can see that such a
    pane is its own and cannot see which component it is; killing it would be a positional
    guess on a window whose panes have moved, which is the guess that kills the wrong
    pane. It is left alone, and one re-layout later every pane charter splits carries an
    id, so the frame heals forward rather than being repaired by guesswork.

    **The record still chooses, and that is why it is walked first.** Where a component
    has both a recorded pane and unrecorded ones, the recorded one is the survivor: it is
    the one the resize hook, the respawn hooks and `layout.repos_cols`' ordering already
    agree about, so keeping it makes the reconciliation a no-op for a healthy frame —
    `keep` comes out in exactly the order and with exactly the contents the loop this
    replaced produced. A recorded id tmux does not list is dropped instead of kept, so a
    pane that has gone gets its component re-split rather than leaving a hole nothing
    fills; that is only ever done when the window could actually be read.

    Adopted panes are not re-armed for respawn. A pane split for component `x` was armed
    for `x` at its creation (`_arm_panel_respawn`) and a re-layout does not change which
    component it draws, so the hook it carries is already the right one; re-arming would
    be a second writer of a fact that has not changed.
    """
    live = _window_panels(socket, harness_pane)
    keep: dict[str, str] = {}
    doomed: list[tuple[str, str]] = []
    # Which pane ids the record spoke for at all — the set the window walk below defers
    # to, so one pane cannot be decided twice.
    recorded: set[str] = set()
    for slot, pane_id in panels.items():
        # #475, on the value that is about to become a `kill-pane -t` argument. `panels`
        # is `state.panes(fid)`, JSON on disk, so a truncated write or a hand edit reaches
        # here: a `%1;kill-server` in that file armed `kill-server` on every window resize
        # for the life of a window once already.
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        # The harness pane is the one pane `state.record_panes` deliberately never holds
        # (see its docstring), so a record naming it is a record charter did not write —
        # and this loop's other branch would `kill-pane` it, taking the agent's own
        # rectangle down along with the panel the operator was dropping.
        if pane_id == harness_pane:
            continue
        recorded.add(pane_id)
        if live is not None and pane_id not in live:
            # The record points at a pane this window does not have. Neither kept (there
            # is nothing to keep) nor killed (there is nothing to kill) — dropping it is
            # what puts the component back in `_relayout`'s `missing` list.
            continue
        # No `and slot not in keep` here, unlike the window walk below: `panels` is a
        # dict, so a slot cannot arrive twice and a second check would be one no input
        # could ever reach.
        if slot in want:
            keep[slot] = pane_id
            continue
        doomed.append((slot, pane_id))
    for pane_id, mark in (live or {}).items():
        if pane_id in recorded:
            # Already decided by the record, above, which is where a component with both
            # a recorded pane and unrecorded ones picks its survivor.
            continue
        if not mark:
            # `None` — not charter's pane at all — and `""` — charter's, but from a
            # charter that could not say which component. One outcome for two facts, and
            # it is the same outcome for the same reason: charter will not kill a pane it
            # cannot name.
            continue
        if mark in want and mark not in keep:
            # The adoption. Without it this is the split that made the duplicates.
            keep[mark] = pane_id
        else:
            doomed.append((mark, pane_id))
    for slot, pane_id in doomed:
        # Disarm before killing, always: `kill-pane` on an armed panel fires its own
        # `pane-died` hook, and `cmd_respawn` brings back the panel the operator just
        # dropped, one respawn life poorer. See :func:`_disarm_panel_respawn`.
        _disarm_panel_respawn(socket, pane_id=pane_id)
        tmuxctl.run(f"closing the {slot} panel",
                    tmuxctl.server_argv(socket, "kill-pane", "-t", pane_id))
    return keep


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

    **The reconciliation runs here as well as in `_relayout`, and #697's `_dress_window`
    is the precedent rather than a caution** (:func:`_reconcile_panels`). On both launch
    paths this window was created by the launch two dozen lines above — `new-session` on
    charter's own server, `new-window` inside an operator's — so it holds the harness pane
    and nothing else, and the reconciliation correctly finds nothing to adopt and nothing
    to kill. That is precisely the shape #686 cost a release: the window options were
    right at launch and issued from a branch a re-layout could skip, and "this path
    happens not to need it" is what made it possible to be wrong on the other one. "One
    pane per placed component in this window" is a property of `_draw_panels`, not of one
    of its two callers, so both callers assert it. The cost is one `list-panes` per launch
    — one round trip, ~5ms against a real tmux 3.7c, beside the several dozen calls a
    launch already makes.

    **And it is handed NO record, which is the whole difference between the two callers.**
    A re-layout's `state.panes` describes the frame that is running; a launch's describes
    whatever held this frame id last. Frame ids are reused — a chat is `<session>.<n>` and
    a workspace relaunched after its server died gets the same one — so the file in the
    frame's directory can name panes of a server that no longer exists, and believing it
    here would mean a launch that split no panel at all and recorded a map of dead ids.
    Only the window's own answer is admissible at launch, and it is authoritative:
    `_reconcile_panels` drops a recorded id the window does not have, so even a `panels`
    passed here would be discarded the moment tmux could be asked — passing ``{}`` is that
    same conclusion made structural instead of depending on `list-panes` succeeding.
    """
    # The window's own options first, and before any `split-window`: `remain-on-exit` has
    # to be armed before a panel can be born into a window that would throw its corpse
    # away (#408), which is the ordering this call site inherits from where these three
    # used to live (`_dress_window`, #686).
    _dress_window(socket, fid=fid, harness_pane=harness_pane, env=env, v=v)
    panes = _reconcile_panels(socket, harness_pane=harness_pane, want=slots, panels={})
    panes.update(_split_panels(socket, slots=[s for s in slots if s not in panes],
                               fid=fid, harness_pane=harness_pane,
                               env=env, pane_env=pane_env, sizes=sizes, v=v))
    _install_resize_hook(socket, harness_pane=harness_pane, panes=panes, v=v, env=env,
                         fid=fid)
    # Written down, because a frame's shape can now be CHANGED while it runs (the density
    # palette — `cmd_density`), and nothing else afterwards can say which tmux pane charter
    # meant as which slot. Slots only: `state.record_harness_pane` already owns the
    # harness pane, and one fact recorded twice is one fact free to disagree with itself.
    state.record_panes(fid, panels=panes)
    # And SAID so — #748. The panels above are processes, and the ids in that record are
    # what their own `split-window` calls returned, so the record cannot be written until
    # after the last of them exists and every one of them may already have painted. A
    # panel repaints on a version bump and on nothing else (`panel._watch`; `top` is not
    # in `slots.ANIMATED`), and this launch's own bump happened before the first split, so
    # a shape written with nothing behind it is a shape the panels that lost the race
    # never read: `slots._sidebar_live` answered `False` off an empty `state.panes`, `top`
    # drew the roster the sidebar was already drawing, and it stood for the life of the
    # frame. Measured on 90 real launches at 200x50 before this line: 16 of them.
    #
    # AFTER the write and never before it, which is `notify.plane_changed`'s order and its
    # reason — a poller that saw the new version must never then read the old record —
    # and the order `_apply_arrangement` already keeps around the OTHER call to
    # `record_panes`, for this same question one keypress later. Charging the launch one
    # `os.replace` and at most one
    # repaint per panel, both spent before any client is attached or any window selected.
    state.bump(fid)
    return panes


def _dress_window(socket: str, *, fid: str, harness_pane: str, env: dict | None,
                  v: tuple[int, int] | None) -> None:
    """Assert everything about a frame that belongs to its WINDOW rather than to a pane.

    `remain-on-exit -w`, the five `_CHROME` window options, and #657's two `-p` rules
    around the harness. Three calls' worth of scope semantics, measured on tmux 3.7c and
    identical on 3.2:

    * ``set-option -w -t %0 pane-border-lines double`` writes window `@0` and shows as
      ``[]`` on a pane of any other window;
    * ``set-option -p -t %0 pane-border-style …`` writes pane `%0` and shows as ``[]``
      both on `%1` and on `show -w -t %0`.

    Nothing is server- or session-scoped, so **every chat's window has to be told
    separately** — and until #686 that only ever happened at that chat's own launch.

    **Called on every re-layout, not only when a pane is created, and that is the whole
    of #686.** These three used to live inside `_split_panels`, which `_relayout` calls
    behind `if missing:`. A switch into a chat that still holds every drawable panel has
    nothing missing, so it wrote geometry and hooks and **not one option** — measured with
    the repo's own `_FakeServer` driving the real `cmd_chat` at 200x50: `select-window`,
    two `display-message`, four hook calls, five `resize-pane`, `select-pane`, and no
    `set-option` at all. It is reachable in one step: `cmd_launch` creates the second
    chat's window with `new-window -d` and selects it, leaving chat 1's panels alive and
    still recorded, so the operator's first `F2` back into chat 1 finds nothing missing.

    **What is NOT hoisted is as deliberate as what is.** The per-pane calls —
    `_panel_mark_argv`, `_surface_argvs` — stay in the split loop, because that is the one
    place charter has a panel's own `%N` in hand and `window-style` is a pane option. And
    the harness's INTERIOR is never painted at any version or level: `_harness_rule_argvs`
    cannot emit `window-style`, which is ADR 0018's boundary made structural rather than
    remembered. #657's own design point is that the harness's rules are charter's and its
    interior is the agent's; making the rules apply everywhere must not blur that.

    Reported but not fatal, like the splits: a frame whose borders kept tmux's own colours
    looks wrong, it does not fail. `remain-on-exit` is armed FIRST and this function is
    called before any `split-window`, so no panel is ever born into a window that would
    throw its corpse away and its respawn hook with it (#408).
    """
    tmuxctl.run("keeping the frame's own dead panes long enough to bring them back",
                _panel_remain_on_exit_argv(socket=socket, harness_pane=harness_pane),
                env=env)
    # The frame's own word for its chrome, resolved through `_current_chrome` so a live
    # `charter frame-chrome` is what a re-layout re-asserts rather than the configured
    # value the frame launched with. `_split_panels` resolves it the same way for the pane
    # scope; one resolver, so the two cannot disagree.
    chrome = _current_chrome(fid)
    # Above `tmuxctl.PANE_BORDER_FLOOR` the rules are each pane's own
    # (`_pane_border_pairs`, set beside that pane's surface in `_split_panels`) and the
    # WINDOW's stay bare — which is #631: a frame-wide border surface is one value for
    # every rule, so a panel whose colour is not the frame's got rules in a colour it does
    # not wear. Below the floor those two options have no pane scope at all, so the
    # frame-wide answer is the only one that can be given and it is given here. One or the
    # other, never both.
    #
    # The rules carry the frame's surface from the ARRANGEMENT rather than from any slot
    # list: `instance.border_bg` reads `config.FRAME`, which is the same thing `cmd_chrome`
    # reads on the live path — the agreement #610 is about, made by construction rather
    # than by two call sites matching.
    pane_borders = _pane_borders_wanted(v)
    # The plane's own answers about how its chrome READS — its rules, its foreground and
    # whether charter may dim. Resolved ONCE for the whole dressing and handed to both
    # calls below, for the reason `instance.Look` is a record at all: the window's rules
    # and the harness's three edges are two writes of one decision, and a frame whose two
    # disagreed is #514 with a new cause.
    look = instance.look_of(config.FRAME)
    # The same *v*, handed on rather than re-read: `_chrome_argvs` drops the rows of
    # `_CHROME` this tmux has no such option for (#716), and a second `tmux -V` here would
    # be a second subprocess for one unchanging fact — and one more place for the two
    # readings to disagree. It answers a different question from *look*: one says which
    # rows are issued, the other says what the two style rows carry.
    for argv in _chrome_argvs(
            socket=socket, harness_pane=harness_pane, v=v, look=look,
            surface=None if pane_borders else instance.border_bg(config.FRAME, chrome)):
        tmuxctl.run("styling the frame's own rules", argv, env=env)
    # And the three rules AROUND the harness, which above the floor are the harness's own
    # cells and are therefore the one part of the frame the loop above no longer reaches
    # (`_harness_rule_argvs`, #657). Left unset by #631 they were the terminal's own
    # background while the panel on the far side of the very same rule was surfaced, so one
    # horizontal rule came out in two colours — reported off a screenshot three times, and
    # reproduced through a nested client at 100x24: 78 rule cells carrying an explicit
    # `ESC[49m` beside 22 cells of `ESC[100m`.
    for argv in _harness_rule_argvs(
            socket=socket, harness_pane=harness_pane, pane_borders=pane_borders,
            look=look, surface=instance.agreed_border_bg(config.FRAME, chrome)):
        tmuxctl.run("styling the rules around the pane charter does not paint", argv,
                    env=env)


def _split_panels(socket: str, *, slots: list[str], fid: str, harness_pane: str,
                  env: dict | None, pane_env: dict[str, str] | None,
                  sizes: dict[str, int] | None = None,
                  v: tuple[int, int] | None = None) -> dict[str, str]:
    """One `split-window` per slot, and the `{slot: pane id}` map that came back.

    The splitting half of :func:`_draw_panels`, separated from the hook-and-record half so
    a live re-layout (`_relayout`) can add panes to a frame that already has some without
    re-installing hooks per batch or overwriting the map it is in the middle of building.

    **What belongs to the WINDOW is not here, and #686 is why it moved out**
    (:func:`_dress_window` — `remain-on-exit -w`, the five `_CHROME` options, the two
    harness rules). Those are properties of the frame, not of a pane being created, and
    this function is only ever called when there is a pane to create: `_relayout` calls it
    behind `if missing:`, so a switch into a chat that still holds every drawable panel
    wrote **zero** window and pane options — geometry and hooks re-asserted, not one
    border option, measured with the repo's own `_FakeServer` driving the real `cmd_chat`
    at 200x50. What was right about arming them here is the FUNNEL, and that survives: it
    is the same funnel one level up, in `_draw_panels` and `_relayout`, where it runs
    whether or not a pane is being split.

    **The pane SURFACE is still here, one pane at a time** (`_surface_argvs`). It
    is the same funnel and the same argument, one scope down: `window-style` is a PANE
    option, so unlike the border it has to be set on each panel as it appears — which is
    also exactly why the harness pane never gets it. Both launch paths and every density
    change reach panels through this function, so a frame cannot come out surfaced on one
    server and bare on the other, and a pane added by a later `_relayout` is surfaced when
    it is created rather than by a second pass that could forget it.

    **And the PANEL MARK, for that third time and the same reason** (`_panel_mark_argv`,
    #634). It is the pane option `conf_text`'s `MouseDown1Pane` bind asks before it lets a
    click move the keyboard, so a panel that reached the operator's screen without it is a
    panel that steals focus the first time it is clicked. A funnel is the only place that
    can promise every panel has one.

    Reported but not fatal, like the splits themselves: a frame whose panels cannot be
    respawned is still a frame, and the harness pane's own `remain-on-exit` was armed
    separately and earlier (`_remain_on_exit_argv`), so the exit code does not ride on
    this.
    """
    # The word, resolved once for the whole batch, through `_current_chrome` rather than
    # off `config.FRAME` — which is the difference a live `charter frame-chrome` makes.
    # The configured value says what a frame STARTS at; a frame the operator has since
    # surfaced from the palette carries its own record, and a pane split into it by a
    # later density change has to be born into the surface the frame IS rather than the
    # one it launched with. One resolver, so the palette's mark and this pane's option
    # cannot disagree. `instance.chrome_options` is still what turns the word into
    # styles, so a value charter does not know produces no commands at all, which is
    # `off`. `_dress_window` resolves the same word for the WINDOW's options; both ask
    # `_current_chrome`, so the pane scope and the window scope cannot answer differently.
    chrome = _current_chrome(fid)
    # The frame-wide appearance, resolved once for the whole batch rather than per pane —
    # `chrome` above is resolved once here for the same reason, and the two travel together
    # into `_surface_argvs`. A per-pane read would let two panes split in one loop be
    # painted from two readings of one file.
    look = instance.look_of(config.FRAME)
    pane_borders = _pane_borders_wanted(v)
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
            # This pane is a PANEL, said to tmux itself where a root-table binding can ask
            # (#634, `_panel_mark_argv`). It goes before the surface rather than after
            # because it is the only one of the two that decides where a click goes: a
            # panel that came up unmarked takes the keyboard off the harness the first
            # time it is clicked, which is the whole defect. Reported but not fatal, like
            # the splits and the surface — a panel charter could not mark still draws and
            # is still clicked, it just costs the operator an `F12` afterwards.
            tmuxctl.run("marking a panel so a click on it stays where it points",
                        _panel_mark_argv(socket=socket, pane_id=pane_id), env=env)
            # And WHICH panel it is, on the same pane and out of the same funnel (#714,
            # `_panel_slot_argv`). The mark above says a pane is charter's; this says what
            # charter meant by it, which is the half `state.panes` cannot be trusted for —
            # that file is rewritten whole on every re-layout, so a component whose pane
            # got split twice has one id recorded and the other invisible to every reader
            # that goes through it. Written where the pane is CREATED rather than by a
            # later pass, because a later pass is a thing to forget: `_reconcile_panels`
            # will not kill a pane it cannot name, so a panel that missed this is a panel
            # that can be orphaned exactly once more. `None` for a name charter will not
            # put on a pane — see there for why that is not silent about its cost.
            slot_argv = _panel_slot_argv(socket=socket, pane_id=pane_id, slot=slot)
            if slot_argv is not None:
                tmuxctl.run("saying which component a panel draws", slot_argv, env=env)
            # The pane surface, on the pane that was just created and on no other — the
            # one place charter has a panel's `%N` in hand. The harness pane is never an
            # argument (`_surface_argvs`), which is how ADR 0018's boundary holds by
            # construction. Not fatal, like the splits: a frame whose panes kept the
            # terminal's own background is a frame that looks plainer, not one that fails.
            # And this pane's OWN background where the arrangement gave it one
            # (`instance.component_style`, the single walk over the placements that
            # `frame/slots.py` asks for the pad on the other side of the split). Resolved
            # per slot rather than once for the batch, unlike `chrome` above, because that
            # is the whole difference between the two keys: one word about the frame, one
            # word about this pane.
            bg = instance.component_style(config.FRAME, slot)["bg"]
            for surface in _surface_argvs(socket=socket, pane_id=pane_id, chrome=chrome,
                                          bg=bg, pane_borders=pane_borders, look=look):
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


def _pin_workspace(ws: str, fid: str, picked: bool) -> None:
    """Write the workspace pointer for a chat the operator PICKED — and say it locked.

    Lifted out of `cmd_launch` rather than copied, because both launch paths now allocate
    their own chat id after their own reap (`state.new_chat_id` claims a directory, and a
    reap that ran afterwards would delete it), so the pointer can only be written once the
    id exists — which is inside each path rather than above both of them.

    **Only when the operator actually picked.** A launch that resolved silently writes no
    pointer today and must keep writing none — starting to would move every framed
    session's workspace on a path nobody asked anything on.

    *fid* is the chat, so the per-session pointer lands under the CHAT's id: inside a frame
    the frame IS the charter session (ADR 0019), which is what makes the choice reach the
    panels and the agent's own shell alike (`state.workspace_for` rung 1) — and what makes
    two chats in one workspace able to hold two different pointers. `set_active` also
    writes the pointer for the LAUNCHER's terminal, and that is the half that answers
    #518's "must not answer a prompt every launch": the next launch from this terminal
    finds `workspace.chosen` already answered and never asks.

    `force=True` is kept, and its reason has narrowed rather than gone. It was for the
    recycled pid: an earlier launcher for this workspace landing on this process's pid
    minted the same id and could have left a lock under it. An allocated id cannot be a
    previous frame's, so that case is closed by `new_chat_id` — but a lock can still be
    standing under this id from a chat that ran, was reaped, and had its ordinal allocated
    again, and being refused by a dead frame's lock on a name the operator just typed is
    not a refusal worth having.

    **Picking IS the confirmation that locks, and #518 asks that this be decided and
    SAID.** `set_active`'s contract is that confirming a workspace locks the session to it,
    and the picker is a confirmation — the alternative would be a launch that writes the
    pointer and leaves the lock off, which is a third behaviour for `charter workspace use`
    to disagree with. What makes it liveable is that the lock has its own way out, named in
    the sentence that announces it: `charter workspace unlock`, typed in the frame's own
    shell, so the operator is not stuck with a choice they just made at a prompt. Printed
    here rather than in the picker, because it describes what the LAUNCH did with the
    answer, not what the answer was.

    **That sentence used to lead with `F2 → workspace` and no longer can** (§4j/§4b). The
    escape it named was `switch.to_workspace` overriding the lock — and that switch no
    longer touches the lock at all, because what it moves is the tmux client rather than
    the chat: after it the operator is looking at another workspace and this session is
    still locked to the one its commands act on. The argument above is unchanged because
    the OTHER escape was always the one that does the work: `unlock` releases the lock
    without moving the chat, which is exactly what a lock the operator wants gone needs
    and all it needs.

    **This write is a lock, not a membership move, which is why §4j leaves it standing.**
    The pointer lands under a chat whose workspace is *ws* already — this launch is what
    minted the link (`state.record_workspace`, called by both launch paths) or, on the
    focus path, is joining a chat `chats.of_workspace(ws)` selected on exactly that
    predicate. So the value written is the value already there, and what it adds is the
    lock and the launching terminal's own pointer.
    """
    if not picked:
        return
    workspace.set_active(ws, session_id=fid, force=True)
    util.info(f"Workspace '{contain.one_line(ws)}' — 🔒 locked for this frame's "
              f"session. `charter workspace unlock` releases it.")


def _focus_workspace(session_id: str, chat: str, *, ws: str, picked: bool) -> int:
    """Attach to a workspace this plane already has open, and start nothing — §4k.

    The other half of :func:`_workspace_to_focus`, and everything it does NOT do is the
    point: no `new_chat_id`, no directory, no `new-window`, no `select-window` and no
    `_drop_panels`. Somebody is reading a chat in this workspace; this launch joins them
    on it. §2.4's geometry race between two clients of one session is untouched and §5
    says so out loud — this removes the reason to have two clients on different windows,
    not the cost of having two.

    **`-t <session id>`, never the workspace's name.** Measured on tmux 3.7c and at the
    3.2 floor: `attach -t $N` attaches to that session, and a second client attaching to a
    session does not move its current window — so a focus is guaranteed not to be the drag
    it exists to avoid. The id came from the pane THIS plane recorded
    (:func:`_workspace_to_focus`); a name would have come from a namespace every plane on
    the machine shares.

    **The pointer is still written, and #518 is why.** `_pin_workspace` is what tells the
    launching terminal which workspace it answered for, so an operator who picked one at
    the prompt and was focused into it is not asked again next launch. It is written under
    the chat being focused rather than under a new id, because that chat is the frame
    session the pointer is about — and it names the same workspace that chat is already
    in, so the write is the same value it already held.

    **No `env=` on the attach, and that is the difference from `cmd_launch`'s own.** That
    one carries `_frame_env(fid, h)` because it is the launch that MADE the frame and the
    client it attaches is that frame's. This launch made nothing: the session already
    carries its own `CHARTER_SESSION_ID` (`_session_id_env_argv`) and each window its own
    `-e` overlay, and handing this client a frame identity charter invented for a chat it
    did not open would put a second answer next to the right one.

    Returns 0 for a clean detach — the workspace is still running and that is not a
    failure — and `attach`'s own code for a refusal, reported rather than folded away, the
    same way `cmd_launch`'s own attach is.
    """
    _pin_workspace(ws, chat, picked)
    attach_cmd = tmuxctl.server_argv(SOCKET, "attach", "-t", session_id)
    attached = tmuxctl.interact(attach_cmd)
    if attached.returncode != 0:
        tmuxctl.report_failure("attaching to the frame", attach_cmd, attached)
        return attached.returncode
    # The same sentence a launch that detached prints, and true for the same reason: this
    # client left, the session did not. Named by the WORKSPACE, because that is what an
    # operator types — the `$id` above is charter's own way of being sure which one.
    util.info(f"charter frame: detached — the harness is still running.\n"
              f"  reattach with: tmux -L {SOCKET} attach -t "
              f"{state.workspace_prefix(ws)}")
    return 0


class Reopening:
    """One recorded chat on its way back, and the id it came back as.

    **The whole of the seam between `charter reopen` and the launcher**, carried on the
    `args` namespace `cmd_reopen` builds and read with `getattr` — which is `--probe`'s own
    shape (`cmd_launch`'s first line) and the reason there is no new CLI surface here. A
    reopen is not a thing an operator types at a launcher; it is `charter reopen` driving
    one, so it has no business in `charter claude --help`, and every existing caller of
    `cmd_launch` (production and test) constructs an `args` without this field.

    **Mutable, and that is what it is for.** `cmd_launch` allocates the new chat id, and
    the driver needs it back: to put the operator on the right tab at the end, and to
    report which recorded chat became which live one. The alternative — reading the frame
    root before and after and taking the difference — would be inferring an id from a
    directory listing that a sibling launcher on the same plane is free to change, for a
    fact the launcher itself has in hand.

    :attr:`fid` stays ``""`` for a launch that never got as far as claiming one, which is
    exactly what makes "this chat did not come back" reportable rather than silent.
    """

    def __init__(self, chat) -> None:
        #: The `frame/reopen.Chat` this is restoring.
        self.chat = chat
        #: The chat id the launcher allocated, or ``""`` if it never got one.
        self.fid = ""


def _reopening(args) -> "Reopening | None":
    """The reopen this launch is part of, or ``None`` for an ordinary launch.

    Four things in :func:`cmd_launch` turn on it, and each would be a defect on the reopen
    path rather than a saving: open-or-focus (§4k) would swallow every chat after the first
    of a workspace, `select-window` would move a client that does not exist yet,
    `_drop_panels` would strip the panels off the sibling this same reopen had just drawn,
    and `attach` would block on the first chat and never build the second.
    """
    got = getattr(args, "reopening", None)
    return got if isinstance(got, Reopening) else None


def _wants_attach(args) -> bool:
    """Whether this launch should become the operator's terminal.

    Asked as its own function rather than inline as ``_reopening(args) is None`` because
    the two are genuinely different questions that happen to have had one answer — "am I
    restoring" and "am I the terminal". **That day arrived**: :func:`_open_workspace`
    opens a workspace a tab named, from a `frame-switch` running detached with its streams
    on `/dev/null`, and it is neither restoring nor a terminal. So the expression is
    unpicked here, once, rather than at the three call sites that read it.

    ``attach`` is deliberately a `getattr` default rather than a required field, for
    `_reopening`'s own reason: every other caller of `cmd_launch` in this codebase — the
    CLI's own namespace, `cmd_reopen`'s, and every test's — constructs an `args` without
    it, and each of them IS the terminal. Absent means yes.
    """
    if not getattr(args, "attach", True):
        return False
    return _reopening(args) is None


def _launch_size(args) -> tuple[int, int] | None:
    """The window size this launch was HANDED, or ``None`` to measure its own terminal.

    `cmd_launch` sizes the frame from `os.get_terminal_size()`, which is right for every
    launch that has a terminal and raises `OSError` for one that does not — and the
    `_FALLBACK_SIZE` underneath it is 80x24, which `_drawable_slots` reads as room for
    almost nothing. A workspace opened by :func:`_open_workspace` has no terminal of its
    own and yet is about to be shown on one charter can already measure (the client is
    looking at the switching chat's window this instant), so the number is passed in
    rather than guessed.

    Validated rather than trusted: it reaches `layout.session_argv` as `-x`/`-y`, and a
    non-positive or non-integer pair there is a tmux parse error that would take the whole
    launch down with nothing on screen to say why.
    """
    got = getattr(args, "size", None)
    if not isinstance(got, tuple) or len(got) != 2:
        return None
    cols, rows = got
    if not isinstance(cols, int) or not isinstance(rows, int):
        return None
    return (cols, rows) if cols > 0 and rows > 0 else None


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

    # **`_wants_attach` is in this guard, and it is a correctness fix rather than a
    # widening.** A non-tty stdout means exactly one thing — "this process cannot be the
    # operator's terminal" — and the conclusion drawn from it, run the harness bare, is
    # right only for a launch that was going to BE that terminal. `_open_workspace` starts
    # one from a `frame-switch` running detached with all three streams on `/dev/null`
    # (`builtin_actions._spawn`), and it never attaches: arriving is `switch-client`.
    #
    # Left un-gated this is not a wrong frame, it is an `os.execvp` — `bypass` execs — so
    # the switching process would have been REPLACED by a bare harness writing to
    # `/dev/null`: no frame, no session, no switch, no exit code, and no surface left to
    # report any of it on. That, and not the `attach` the old refusal named, is what
    # actually stopped a detached process opening a workspace.
    if args.no_frame or (not sys.stdout.isatty() and _wants_attach(args)):
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
    # Inside a tmux the operator already has, the frame is a WINDOW in THEIR server —
    # same layout, no second tmux, no second prefix key (ADR 0018 and the design spec
    # both settle this; `docs/frame.md` describes what it costs). Read from `$TMUX`,
    # which tmux exports into every process it starts in a pane, and CONFIRMED against
    # the server before anything is built on it: that variable outlives the server it
    # names often enough to matter (`env` captures, a `tmux kill-server` under a running
    # script), and charter is then not inside a tmux whatever it says. `None` back means
    # exactly that, and the private-server path below is the right one after all.
    #
    # **`is_operator_socket`, not `inside is not None`, and that is #812's other half.**
    # `$TMUX` says which tmux this process is INSIDE; it does not say whose it is. A
    # launch reached from a workspace tab runs in a panel process of charter's own frame
    # (`_open_workspace` calls this function in-process), so its `$TMUX` is charter's own
    # socket spelled as a path — and this branch, asked only whether the variable parsed,
    # built the new workspace's chat as a `new-window` in whatever session the CALLER was
    # in. Measured on tmux 3.7c against a real server, before this line changed, with
    # `$TMUX` naming charter's own socket by path and a launch asking for workspace
    # `beta`: `list-sessions` reported only the caller's own session and never a `beta`,
    # `list-windows -a` reported `beta.1` as a window INSIDE it, `state.record_server`
    # wrote the absolute spelling, and the launcher was still inside `_wait_for_harness`
    # when the harness was still alive. So `_open_workspace`'s closing
    # `_plane_session(socket, ws=ws)` could never find the session it had just asked for,
    # and the switch never got its answer back at all. With this line as it now stands,
    # the same launch recorded the short NAME and left a real `beta` session behind. On
    # charter's own server a workspace IS a session (§2.1), so the private path below is
    # the right one however `$TMUX` spells the socket.
    #
    # **What this changes for a launch that WILL be a terminal, stated rather than
    # discovered.** `charter <harness>` typed at a shell inside a charter frame now builds
    # its own session and reaches the `attach` below, from a process already inside that
    # very server — a NESTED client, which `_frame_env` makes possible by popping `$TMUX`
    # and which was measured to succeed on tmux 3.7c and at the 3.2 floor (a client on the
    # inner pane's tty, `list-clients` reporting it against the new session). That is the
    # pre-ADR-0018 shape and it stacks two prefix layers on one terminal, so it is not
    # good — but what it replaces is not "working": that launch used to land a chat for
    # one workspace as a window inside ANOTHER workspace's session, whose every workspace
    # tab then refused with #812's own sentence. The one-way door was reachable by typing
    # as well as by clicking, and both doors are the same defect.
    inside = tmuxctl.operator_server()
    if inside is not None and tmuxctl.is_operator_socket(inside[0], own=SOCKET):
        rc = _launch_in_operator_tmux(inside[0], inside[1], ws=ws, argv=argv,
                                      h=h, v=v, picked=picked)
        if rc is not None:
            return rc

    # Reap BEFORE this frame's own directory exists, not after: `frame_dir(create=True)`
    # below makes that directory, and a `reap()` run afterward — but still before
    # `session_argv` starts this frame's OWN tmux session — would see a directory with
    # no live session yet and delete it out from under this very launch. Reaping first
    # also narrows (though does not close) the same race for a sibling frame's `exit`
    # file: less time between "session gone" and "directory removed" for a sibling's own
    # launcher to lose the read.
    live_sessions = _live_sessions(SOCKET)
    # Both, and neither is redundant: a CHAT's directory is kept only by the chat id its
    # window carries (`_live_chats` — a chat's id holds no launcher pid for `reap`'s
    # second rule to abstain in its favour), and a frame launched by a charter that
    # predates chats is still a SESSION named by its id and is kept only by that list.
    live_chats = _live_chats(SOCKET)
    # A server that listed SESSIONS and then would not list its windows is a server
    # charter cannot tell the live chats of, and reaping on that answer deletes the state
    # of chats that are running — the version file their panels poll and the exit code
    # their own launcher has not read yet. Not reaping costs a directory until the next
    # launch. An EMPTY session list is the opposite fact and reaps as it always did: the
    # server is not running, so nothing recorded against it is live.
    can_reap = live_chats is not None or not live_sessions
    live_before = live_sessions | (live_chats or set())
    if live_sessions:
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
    if can_reap:
        state.reap(live_before, server=SOCKET)

    # **Open or focus** (§4k), and it is asked HERE — after the reap, before the
    # allocation — for both of its neighbours' reasons. After the reap, so the chat
    # directories `_workspace_to_focus` reads are the ones that survived it rather than
    # ones about to be deleted; before `new_chat_id`, so a launch that focuses claims no
    # ordinal, makes no directory and writes no state at all. A focus is a READ and an
    # `attach`, and nothing else.
    #
    # **Only for a launch that asked for nothing but the workspace, and that gate is not
    # a nicety.** Attaching answers "put me in `foo`". It cannot answer "run THIS in
    # `foo`", and a focus taken over one would silently discard an argv the operator
    # typed: `charter frame -- <cmd>` is the escape hatch for a command charter has never
    # met, and `charter claude --resume <id>` carries the operator's own flags through
    # `rest` into `launch_argv`. Both must still open a chat and run what they named — a
    # launcher that swallowed a command and attached instead would be wrong in the one
    # direction this module refuses everywhere else, silently.
    #
    # **`rest` alone, and an `h is not None` beside it would be a guard nothing can
    # reach.** An unregistered harness means `argv = rest`, and `if not argv:` above has
    # already ended the launch with "nothing to run" — so by here a launch with no `h` is
    # a launch with a non-empty `rest`, and the second half of that condition could never
    # decide anything. The deletion sweep found it as a survivor for exactly that reason.
    #
    # **And never for a reopen**, which is the same gate one case wider. Attaching answers
    # "put me in `foo`"; a reopen is answering "put back the four chats `foo` had", and the
    # second of those four would be swallowed by a focus onto the session the first one had
    # just created. It cannot be spelled as "reopen always carries a `rest`" either — only
    # a Claude chat with a recorded id carries `--resume`, and a chat with nothing to resume
    # is exactly the one that must still come back as a tab.
    if not rest and _reopening(args) is None:
        focus = _workspace_to_focus(SOCKET, ws=ws)
        if focus is not None:
            return _focus_workspace(*focus, ws=ws, picked=picked)

    # AFTER the reap, for the reason the paragraph above gives about the directory: the
    # allocation IS a `mkdir`, so an id claimed before the reap would be a directory the
    # reap then deleted, with this launch already holding the name.
    #
    # **Allocated, not computed.** `state.frame_id`'s `{workspace}-{pid}` could collide
    # with a previous launcher's on a recycled pid, which is what `clear_exit` /
    # `clear_shape` / `clear_respawn` below exist to survive; a claimed ordinal cannot,
    # because `mkdir` is the exclusion. Those calls stay because Stage 5c reopens a COLD
    # chat into its own existing directory, which is the case they are actually for.
    fid = state.new_chat_id(ws)
    if fid is None:
        util.err(f"charter frame: could not open a chat in workspace {ws!r} — its state "
                 f"directory could not be created")
        return 1
    _pin_workspace(ws, fid, picked)
    # The tmux SESSION is the workspace, and the chat is one window in it. `new_chat_id`
    # spells the id out of the same `workspace_prefix`, so the session's name is exactly
    # the part of the chat id before the dot and the two can always be matched up by eye.
    session = state.workspace_prefix(ws)

    fdir = state.frame_dir(fid, create=True)
    if fdir is None:
        # `frame_dir` refuses rather than raises (see charter/frame/state.py) — an id
        # `contain.child` cannot shape into a directory (or a name so long `mkdir` hits
        # ENAMETOOLONG) must not be treated as a Path here just because it usually is one.
        util.err(f"charter frame: could not create state for frame {fid!r}")
        return 1
    # Belt and braces, and no longer the guard it was. The pid this launch was handed
    # could belong to an earlier launcher for the same workspace, which minted the SAME
    # `<workspace>-<pid>` id — and since #383 `reap` keeps that earlier directory while
    # the pid in its name is live, which on a launch it is, because it is ours. An
    # ALLOCATED id cannot be a previous frame's: `state.new_chat_id` claims by `mkdir`
    # and a taken ordinal is skipped, so nothing is under this id to clear. Kept because
    # Stage 5c's reopen relaunches into a cold chat's OWN directory, which is where these
    # four become live again — `state.clear_shape` most of all (#413).
    #
    # Three things, because three readers inherited. `state.exit_code(fid)` below would
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
    # And WHERE — see the identical call on the operator's-tmux path, and
    # `state.record_cwd` for why this fact could not ride in `identity`. Read once here
    # and used twice: recorded, and handed to `chat_window_argv` below.
    launch_cwd = os.getcwd()
    state.record_cwd(fid, launch_cwd)
    # **And the two restore items that are not this launch's own to write** — the persona
    # the recorded chat had chosen, and the transcript captured off its pane before it was
    # killed. Both are keyed on a chat ID, and the id has just changed: `new_chat_id`
    # allocates a fresh ordinal, so the pointer and the file that named the old chat name
    # nothing a moment from now. Done HERE, before the harness is started, because the
    # persona is resolved inside the pane at run time and a pointer written afterwards
    # would be read one turn late. See :func:`_restore_recorded_chat`.
    restoring = _reopening(args)
    if restoring is not None:
        restoring.fid = fid
        _restore_recorded_chat(restoring.chat, fid)
    state.clear_respawn(fid)
    state.bump(fid)
    # Kicked before tmux is asked for anything, so the gather runs alongside the session
    # start rather than in front of it. See `_spawn_gather`.
    _spawn_gather(fid, ws)

    given = _launch_size(args)
    if given is not None:
        # Handed a size by a caller that has no terminal of its own but knows which one
        # this frame is about to be shown on — see :func:`_launch_size`. Measured in a
        # process with its streams on `/dev/null`, on tmux 3.7c and at the 3.2 floor
        # alike: `os.get_terminal_size()` raises there, so without this the frame would be
        # laid out for `_FALLBACK_SIZE` and `_drawable_slots` would drop every panel the
        # operator's real terminal has room for.
        cols, rows = given
    else:
        try:
            cols, rows = os.get_terminal_size()
        except OSError:
            # `os.get_terminal_size()` raises even when `isatty()` said yes — a tty with
            # nothing behind it to answer `TIOCGWINSZ`. Falling back rather than
            # propagating is the deliberate choice; see `_FALLBACK_SIZE`'s own docstring
            # for why 80x24.
            cols, rows = _FALLBACK_SIZE

    slots = _drawable_slots(cols, rows)
    env = _frame_env(fid, h)
    # Same as the operator's-tmux path above, and needed harder here: charter's private
    # server is SHARED, so a `run-shell` child on it reads whichever launcher's
    # environment started the server. See `state.record_identity`.
    state.record_identity(fid, _frame_identity_env(env))

    conf_path = fdir / "tmux.conf"
    # The frame ROOT, not this chat's own `exit` file: the variable is session-scoped and
    # a session holds several chats now, so the hook appends `#{@charter_chat}` itself.
    # See `_exit_path_env_argv` and `_pane_died_write_hook_argv`.
    frame_root = fdir.parent
    # `config.write_for`, not `Path.write_text`: this file is under `.charter/frame/<fid>/`
    # and `write_text` creates at `0o777 & ~umask` — 0644 by default, 0666 under `umask
    # 000`. The frame's tmux config carries the session id, the hotkey and the toggles, and
    # a world-WRITABLE one is tmux configuration another account gets to choose for this
    # session (#582; the dispatch is #505's).
    config.write_for(conf_path, _PLACEHOLDER_CONF)

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
    #
    # **The workspace's session, or one more window in it.** A chat is a window, so the
    # second chat of a workspace does not start a second session: it joins the one the
    # first chat's launch created, which is what makes switching between them
    # `select-window` rather than a reattach, and what makes a workspace something several
    # harnesses can be open in at once. `new-window` carries the identical `-e` overlay
    # for the identical reason, and needs no version gate for it — window `-e` is
    # available at `tmuxctl.PANE_ENV_FLOOR` (3.0), below the floor, where `new-session -e`
    # is a parse error below `SESSION_ENV_FLOOR` (3.2).
    #
    # **The one residual, stated rather than guarded.** `live_sessions` also holds the
    # sessions of frames an OLDER charter launched, which are named `<workspace>-<pid>` —
    # so a workspace named, say, `api-4242` would join a still-running old frame for
    # workspace `api` whose launcher pid happened to be 4242, and that frame's own
    # teardown hook (`kill-session`, installed before this change) would take the new chat
    # down with it. It needs a workspace deliberately named like a frame id AND that exact
    # frame still live across the upgrade, and it stops being reachable at all once the
    # last pre-chat frame on the machine has ended. Telling the two apart would mean
    # asking tmux a second question on every launch to buy a case that expires on its own.
    joining = session in live_sessions
    if joining:
        start_cmd = layout.chat_window_argv(
            socket=SOCKET, session=session, chat=fid, cwd=launch_cwd,
            harness_argv=argv, env=_frame_identity_env(env))
        started_what = "adding a chat to the workspace"
    else:
        start_cmd = layout.session_argv(
            session=session, conf=str(conf_path), socket=SOCKET, cols=cols, rows=rows,
            harness_argv=argv, chat=fid,
            env=_frame_identity_env(env) if v >= tmuxctl.SESSION_ENV_FLOOR else None)
        started_what = "starting the frame"
    proc = tmuxctl.run(started_what, start_cmd, env=env)
    if proc.returncode != 0:
        return 1
    harness_pane = proc.stdout.strip()
    if not harness_pane:
        util.err("charter frame: tmux started the frame but did not report a pane id "
                 "— cannot scope the exit-code hook to it")
        return 1
    # See the identical call on the operator's-tmux path: this is what lets a status line
    # tell "I am this frame's harness" from "I inherited this frame's id", which below
    # `SESSION_ENV_FLOOR` a second frame on this shared server genuinely does.
    state.record_harness_pane(fid, harness_pane)
    # And what every later lookup asks instead of parsing the window's name: `reap`'s
    # liveness list (`_live_chats`), the write hook's `#{@charter_chat}`, and the binds
    # `conf_text` writes. Armed before either hook, so the hook that names it cannot fire
    # against a window that has not been told yet.
    named = _chat_option_argv(socket=SOCKET, harness_pane=harness_pane, chat=fid)
    if named is None:
        util.warn(f"charter frame: {fid!r} is not a shape tmux can carry as a window "
                  "option — this chat's state may be reaped while it is still running")
    else:
        chat_set = tmuxctl.run("naming the chat on its window", named, env=env)
        if chat_set.returncode != 0:
            util.warn("charter frame: continuing without it — this chat's state may be "
                      "reaped while it is still running, and its exit code may not be "
                      "recorded")
    # And which PLANE the session belongs to — §4b's marker, written by the launch that
    # CREATED the session and by no other. `joining` is the same NAME test three dozen
    # lines up, which is exactly the cross-plane collision this marker records: a launch
    # that joined a session it did not make may be standing in another plane's session,
    # and re-marking it there would relabel that plane's frame as this one's. A warning
    # rather than a failure, because what is lost is a veto and not the switch: an
    # unmarked session is still found by the pane records `_plane_session` starts from.
    if not joining:
        marked = _plane_option_argv(socket=SOCKET, harness_pane=harness_pane)
        if marked is None:
            util.warn("charter frame: this plane's state directory is not a shape tmux "
                      "can carry as an option — a workspace switch cannot tell this "
                      "session from another plane's of the same name")
        elif tmuxctl.run("marking the workspace session with its plane", marked,
                         env=env).returncode != 0:
            util.warn("charter frame: continuing without it — a workspace switch cannot "
                      "tell this session from another plane's of the same name")

    frame = config.FRAME
    config.write_for(conf_path,
                     conf_text(hotkey=frame["hotkey"], mouse=frame["mouse"],
                               history_limit=frame["history_limit"], session=session,
                               toggles=instance.frame_toggles(frame)))
    src = tmuxctl.run("loading the frame's config",
                      tmuxctl.server_argv(SOCKET, "source-file", str(conf_path)),
                      env=env)
    if src.returncode != 0:
        util.warn("charter frame: continuing without it — mouse/history-limit/hotkey "
                  "settings may not be in effect for this frame")

    env_set = tmuxctl.run(
        "carrying the exit-status path",
        _exit_path_env_argv(socket=SOCKET, session=session,
                            frame_root=str(frame_root)),
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
                          _session_id_env_argv(socket=SOCKET, session=session, chat=fid),
                          env=env)
    if sid_set.returncode != 0:
        util.warn("charter frame: continuing without it — the palette may not find "
                  "this frame's own actions")

    # The second value the same mechanism carries: which interpreter runs charter when
    # the hotkey fires. Without it both fall back to a bare `charter`
    # on the tmux server's own `$PATH`, and `run-shell` reports the resulting 127 by
    # printing it INTO THE HARNESS PANE — see `conf_text`'s own docstring.
    py_set = tmuxctl.run("carrying charter's own interpreter to the palette",
                         _charter_py_env_argv(socket=SOCKET, session=session), env=env)
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
                               _charter_pythonsafepath_env_argv(socket=SOCKET,
                                                               session=session),
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
        "installing the chat-teardown hook",
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
            # learning anything at all. Read the pane BEFORE `kill-window` destroys it;
            # afterwards there is nothing left to read.
            #
            # Only for a FAILURE. A command that finished cleanly before the frame came
            # up (`charter frame -- true`) reaches this same branch with 0, and whatever
            # it wrote was its stdout — charter repeating that onto stderr would be
            # inventing output on the wrong stream. Its exit code is the whole message.
            util.err(early_death_message(argv, code,
                                         _pane_last_words(SOCKET, harness_pane)))
        # `kill-window` targeting the harness PANE, never `kill-session` on the workspace:
        # this chat's window is what is over, and the workspace may hold other chats whose
        # harnesses are mid-turn. Measured on tmux 3.7c and on tmux 3.2 that `kill-window
        # -t <pane id>` resolves to that pane's own window, and that killing a session's
        # LAST window destroys the session — so the ordinary one-chat case ends exactly as
        # it did.
        tmuxctl.run("ending the chat after an early death",
                    tmuxctl.server_argv(SOCKET, "kill-window", "-t", harness_pane),
                    env=env)

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
            util.err("charter frame: refusing to attach — the chat-teardown hook "
                     "failed to install, so a crash later would block `attach` forever "
                     "with nothing to end the session. The harness is still running, "
                     f"detached; reattach manually if you must: "
                     f"tmux -L {SOCKET} attach -t {session}")
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

            # The chat this launch just built is the one the operator asked for, so it
            # is the one their client lands on. `new-window` created it detached (`-d`),
            # for `layout.chat_window_argv`'s reason — a client already in this workspace
            # must not be dragged onto a half-built window — and this is where that is
            # paid back. A no-op for the launch that created the session, whose only
            # window is already current.
            # Which chat the client is on right now, read BEFORE the select takes them
            # off it (#688). `new-window -d` did not move them, so the session's current
            # window is still the chat being left. A no-op for the launch that created
            # the session, whose only window is the one it just built.
            #
            # **A reopen does neither**, and the reason is this pair's own argument turned
            # around (`_reopening`). `charter reopen` builds several chats back to back
            # with no client attached anywhere: the "chat being left" is then the sibling
            # this same reopen created a moment ago, and dropping ITS panels would strip
            # the frame charter has just finished drawing. Nothing is being taken off a
            # window it was looking at, because nobody is looking yet — the one
            # `select-window` a reopen wants is the one `cmd_reopen` issues at the end, at
            # the chat the manifest says was active.
            if _reopening(args) is None:
                leaving = _chat_being_left(SOCKET, beside=fid)
                selected = tmuxctl.run(
                    "selecting the chat",
                    tmuxctl.server_argv(SOCKET, "select-window", "-t", harness_pane),
                    env=env)
                # And the chat the client has just left loses its panels — `cmd_chat` step
                # 2's rule, applied where the situation is made rather than only where it
                # is managed. Gated on the select having WORKED, which here is what the
                # return code honestly says: both windows are in one session by
                # construction (this launch created its own in `session`), so unlike
                # #684's cross-session case there is no way for `select-window` to succeed
                # against a window the client cannot be moved to. A teardown ahead of a
                # failed select would kill the panels of the chat the operator is still
                # looking at.
                if selected.returncode == 0:
                    _drop_panels(SOCKET, leaving)

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

            # **A reopen does not attach either, and exactly once rather than N times.**
            # `attach` is what makes this function the operator's terminal for the life of
            # the harness, so a reopen calling it per chat would block on the first one and
            # never build the second. `cmd_reopen` attaches itself, after every chat is up,
            # to the workspace the manifest recorded as the one in front of the operator
            # when they quit. Everything below this point already handles "we did not
            # attach": `attach` stays ``None``, `code` stays ``None``, and the tail reports
            # a detached chat, which is precisely what this is.
            if _wants_attach(args):
                # `tmuxctl.interact`, not `tmuxctl.run`: no capture and no timeout — this
                # IS the operator's own terminal for as long as the harness runs, not an
                # admin command whose output (or lifetime) charter should own.
                attach_cmd = tmuxctl.server_argv(SOCKET, "attach", "-t", session)
                attach = tmuxctl.interact(attach_cmd, env=env)

            code = state.exit_code(fid)
            if code is None:
                # A second ask, for whatever gap the eager check above could not have
                # seen yet (the harness was still alive at that point and died later —
                # the ordinary case, caught here by the hooks, or a rarer one where they
                # never actually reached the server despite reporting success).
                code = _query_pane_dead_status(SOCKET, harness_pane)

    # The claim is given up before the reap that exists to collect it (#685). The marker
    # `state.new_chat_id` wrote says "a launcher is still working on this directory", and
    # this one no longer is: `state.exit_code` has already been read above, so the #383
    # window the marker covers is behind it. Left in place it would refuse the one thing
    # only the CLOSING reap can do — remove this frame's own directory — by naming a
    # process that is, necessarily, still alive.
    state.clear_claim(fid)
    after_sessions = _live_sessions(SOCKET)
    after_chats = _live_chats(SOCKET)
    live_after = after_sessions | (after_chats or set())
    if after_chats is not None or not after_sessions:
        state.reap(live_after, server=SOCKET)
    # **The one place the operator can be told a quit happened, and it is not the quit.**
    # `charter: quit` runs detached with its three streams on `/dev/null`
    # (`builtin_actions._spawn`), so whatever it prints is read by nothing; and by the time
    # it has finished there is no frame, no attention row and no client left to draw on.
    # THIS process is where the operator gets their shell back from, so it is the one that
    # can say what happened and name the command that undoes it. Said before every return
    # below, because a quit lands on more than one of them: a `kill-window` writes no `exit`
    # (§2.17), so the ordinary quit reaches the bare `return 0` at the bottom, while a
    # harness that had already recorded a code reaches the first.
    #
    # **Only for a launch that WAS the operator's terminal**, and this gate is a defect fix
    # rather than a tidiness. `new_chat_id` walks upward from 1 and `reap` frees the ordinals
    # a quit's chats held, so a reopen very often gets the SAME ids back — and every one of
    # its own launches would then find itself named in the manifest it is in the middle of
    # acting on, and print "this plane was quit; put it back with `charter reopen`" while
    # `charter reopen` was putting it back.
    if _wants_attach(args):
        _say_it_was_quit(fid)
    if code is not None:
        return code
    if refused_to_attach:
        # Nonzero, deliberately distinct from the `still live` detach path below: the
        # harness never ran interactively and charter has no way to learn its real exit
        # code, so a script or `&&` chain must see this as a failure rather than the
        # quiet success an operator's own deliberate detach is allowed to be.
        return 1
    if fid in live_after:
        if not _wants_attach(args):
            # The reopen path, and it says nothing HERE on purpose. This launch was never
            # the operator's terminal, so "detached" would be describing something that
            # did not happen; `cmd_reopen` is the one process that knows how many chats it
            # asked for and what each one could bring back, so it owns the sentence.
            return 0
        # Detach, not completion — the chat tmux still lists is this launch's own.
        # Silence here is exactly what the spec calls out: "an agent surviving a closed
        # lid is a feature, and returning silently to a shell with it still running is
        # not." The reattach line names the WORKSPACE, because that is the session; the
        # chat is a window inside it, and tmux puts the client back on whichever window
        # was last current.
        util.info(f"charter frame: detached — the harness is still running.\n"
                 f"  reattach with: tmux -L {SOCKET} attach -t {session}")
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


def _current_chrome(fid: str) -> str:
    """The pane surface *fid* is on right now: its own recorded override, else the
    configured default, else ``off``.

    :func:`_current_density`'s twin and read the same way — one function, so the palette's
    mark, a newly split pane's surface and a live `frame-chrome` keypress cannot come to
    three different conclusions about which word this frame is on. `_split_panels` used to
    read `config.FRAME.get("chrome")` directly, which was right while nothing could change
    it and became a pane born bare into a frame the operator had surfaced.

    `instance.chrome_level` gates BOTH sources rather than only the file: a word charter
    does not recognise is `off` wherever it came from, which is the same rule
    `instance.frame_of` already applies to the committed file and the reason a hostile
    `charter.toml` cannot reach a tmux style through here either. `.get` on the config,
    never ``[...]``: a frame relaunched by a charter that predates this key has a resolved
    config without it.
    """
    return (instance.chrome_level(state.chrome(fid))
            or instance.chrome_level(config.FRAME.get("chrome"))
            or "off")


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
    `CHARTER_SESSION_ID` is session-scoped (`_session_id_env_argv`) — and under chats it
    names ONE of the workspace's chats rather than the presser's own, which is why a
    keypress carries `#{@charter_chat}` instead (`conf_text`, `_pressers_chat`) and why
    *fid* arrives here as an argument rather than being read back; `CHARTER_ROOT`,
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

    Dress the window, reconcile the panes it already has against *want*, split what is
    still missing, re-arm the hooks, re-assert every size. In that order, and each step is
    here for a measured reason:

    * **Dress the window first, and unconditionally** (#686, :func:`_dress_window`). What
      belongs to the WINDOW — `remain-on-exit`, the frame's chrome, #657's rules round the
      harness — is a property of the frame rather than of a pane being created, and it
      used to be issued from inside the split below. Behind `if missing:` that meant a
      re-layout of a frame already holding everything it wants wrote no option at all,
      which is one `charter claude` away: the second launch selects its own new window and
      leaves the first's panels alive and recorded, so the first `F2` back has nothing to
      split. First rather than last because `remain-on-exit` has to be armed ahead of any
      pane charter creates, and this function creates some further down.
    * **Ask the WINDOW which panes it has, not the record** (#714,
      :func:`_reconcile_panels`). *panels* used to be the whole answer: a slot in it and
      in *want* was kept, a slot in it and not in *want* was killed, and a slot in *want*
      and not in it was split. All three go wrong together the moment the record is wrong,
      and the record is rewritten whole on every re-layout — so a `charter.toml` edited
      twice in one session left `state.panes` naming only the newest pane per component
      and the older ones unreachable to every reader of it, this loop included. Six panel
      panes where there should have been two, each holding a ~24 MB process and each
      drawing correct content, is what that looked like on a real frame. The record is
      still read, and still chooses which of several panes for one component survives; it
      is no longer the only thing that can see a pane.
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
    # Unconditionally, and above the kill loop — see this function's own first bullet for
    # both halves of why (#686). Being above it costs the kills nothing: measured on 3.7c,
    # `kill-pane` destroys a pane on a window with `remain-on-exit on` outright and leaves
    # no corpse, because that option decides what happens when a pane's COMMAND exits and
    # not what happens when tmux is told to close it.
    _dress_window(socket, fid=fid, harness_pane=harness_pane, env=None, v=v)
    # The kill loop that used to be written out here lives in `_reconcile_panels` now,
    # with the window's own answer alongside the record — see this function's second
    # bullet and that function's docstring for what the record could not see (#714). It
    # is shared with `_draw_panels` for #697's reason, so a pane per placed component is a
    # property of both paths rather than of this one.
    keep = _reconcile_panels(socket, harness_pane=harness_pane, want=want, panels=panels)

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
            pane_env=pane_env, v=v,
            sizes=_slot_sizes(
                fid, want, window_rows=window_rows,
                pane_cols=layout.repos_cols(list(keep) + missing,
                                            window_cols=window_cols))))
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
    `layout.resize_flag` answers exactly *flag* about.

    One loop, called twice by :func:`_reassert_sizes` — once for the columns and once for
    the rows — rather than one loop over both, because the two are separated by a
    MEASUREMENT that only the first one makes truthful (:func:`_variable_pane_cols`).

    **The axis is asked of `layout`, not read out of a map here, and that map was the last
    hand-written table of per-slot facts charter had.** `layout` derives every other one
    (`SLOT_EDGE`, `SLOT_SIZE`, `_COLUMN_SLOTS`, `_FIXED_ROW_SLOTS`, `VARIABLE_ROW_SLOTS`)
    from what each component declares, exactly so a component charter did not write is
    answered rather than missing — and this module's `{"top": "-y", "bottom": "-y",
    "right": "-x"}` was the one that was not. Once Phase 5 made a `[[frame.component]]`
    table reachable, a placed component travelling under its own id was SIZED by
    `layout.slot_sizes`, CHARGED to the harness by `layout.harness_rows`, and then never
    re-asserted here at all: the harness's own `-y` took those rows out of a neighbour and
    the variable row slot absorbed the error. See `layout.resize_flag` for the measurement.

    `layout.VARIABLE_ROW_SLOTS` gets no flag from that function at all, which is what
    leaves the table pane as the stack's dependent one — see its docstring for the tmux
    measurement, and note that a second check for it here would be a guard no mutation
    could turn red, because that one already catches it.

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
        if layout.resize_flag(slot) != flag or slot not in sizes:
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

    So this asserts every slot `layout.resize_flag` gives an axis — which is every slot
    whose size is a CONSTANT, **including one this plane placed under its own id**, and
    deliberately not `layout.VARIABLE_ROW_SLOTS` — plus `layout.harness_rows` on the
    harness itself. The table's size is already a function of all the others, so it lands
    on exactly `repos_rows`' answer without being asserted. In a stack of N panes only N-1
    heights are free; asserting all N is what made the result depend on the order, and
    asserting N-2 is what left a placed bar holding the table's rows.

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
    sizes = _slot_sizes(
        fid, order, window_rows=window_rows,
        pane_cols=_variable_pane_cols(socket, panes=panes, window_cols=window_cols))
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
        # **This command stopped being hook-only the day the 3.2 limit named it** (#744).
        # Below `tmuxctl.RESIZE_HOOK_FLOOR` there is no `window-resized` hook to fire it,
        # and typing it by hand is the operator's ONLY way to get the panels back — so
        # `frame-probe` and `docs/frame.md` now tell them to, and a hand-typed recovery
        # that answers rc 0 and nothing else is the worst possible reply to "did that
        # work?". The hook path cannot reach here: `_resize_hook_argv` writes
        # `--frame <fid>` into the action or arms no hook at all.
        return outside_a_frame("charter frame-resize")
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
    if not fid:
        return outside_a_frame("charter frame-density")
    # Split off the empty *fid* above rather than sharing its `or`, because the two are
    # different silences with different surfaces. That one has no frame and goes to the
    # asker's stderr; this one HAS a frame, and #729 gave the frame a row to answer on.
    # A level outside the closed set can only be hand-typed — a palette row carries one of
    # the three constants — so nothing reaches here but a person who typed a fourth word
    # and, until now, got a success status and no output.
    level = instance.density_level(getattr(args, "level", None))
    if level is None:
        _say_on_screen(fid, f"no density level "
                            f"{contain.readable(getattr(args, 'level', None))} — have: "
                            f"{', '.join(instance.FRAME_DENSITY)}")
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


def outside_a_frame(command: str) -> int:
    """Say that *command* acts on a frame and this shell is not in one, and refuse. Never 0.

    **The one sentence every `frame-*` command an operator can type says when it has no
    frame to act on (#734), and having exactly one is the point.** `charter frame-chat`,
    `frame-density`, `frame-toggle`, `frame-chrome`, `frame-switch` and `frame-resize` each
    used to open with `if not fid: return 0` — four bytes of silence and a success status
    for a command that did nothing. There was no way to tell "it worked" from "you are not
    in a frame", and `docs/frame.md` makes two of them the ONLY route to their action:
    inside a tmux you already have, charter binds no key at all, so typing the command in
    the frame's own window is the documented escape hatch — and typing it in the window you
    started from, one keystroke away, was indistinguishable from success.

    **stderr, and never `_say_on_screen`.** There is no frame here by construction, so
    there is no frame's screen to draw on — and `display-message` would not find one
    anyway. Measured by hand on tmux 3.7c and at the 3.2 floor, one server, sessions `sa`
    and `sb`, the only client attached to `sb`::

        $ tmux -L t display-message -t %0 MSG        # %0 is a pane of session sa
        # the message appears on sb's terminal, and nowhere on sa's

    **`-t` is the target for FORMAT EVALUATION, not the choice of screen.** The screen is
    `-c`, and with no `-c` tmux draws on its own current client. Charter had this recorded
    as a property of an EMPTY target (`cmd_chat`'s own note, and `_say_on_screen`'s); it is
    not — an empty target is merely the loudest case of something true of every target.
    So on a socket carrying eleven frames, a refusal put on screen without the presser's
    own `#{client_name}` is a refusal that can land on somebody else's terminal, and this
    command has no client to pass: nobody pressed a key, somebody typed into a shell. That
    shell's stderr is the one surface that is certainly theirs.

    **Non-zero, and the `run-shell` objection does not reach this branch.** Every one of
    these commands is *also* fired by tmux — a `bind -n`, a window hook, a palette row —
    and a non-zero status there makes tmux print `'<the whole command>' returned 1` INTO
    THE HARNESS PANE, the one rectangle ADR 0018 says charter never draws in (measured on
    3.7c; `conf_text`'s own docstring records the 127 case). That is why these commands
    return 0 for every other refusal and always will. It does not apply here: a bind
    carries `--chat` expanded from the presser's own window, the resize hook carries
    `--frame`, a palette row's child is handed `CHARTER_SESSION_ID` explicitly
    (`frame/builtin_actions._spawn`) — so on every path tmux drives, *fid* is set and this
    function is unreachable. What is left is a human at a prompt, for whom rc 0 was the
    lie. Measured on tmux 3.7c beside it: `run-shell` shows a child's **stdout** and
    discards its **stderr** entirely, so even on a stale bind from an older charter the
    sentence below costs the harness pane nothing; only the status would show.

    The text names the window rather than only the failure, because "not in a frame" tells
    an operator who typed this in the wrong pane nothing they did not know, and `charter
    docs show frame` is where the rest of it lives — the one place F2, F12 and copy-mode
    scrollback are written down (#747).
    """
    util.err(f"charter: {command} acts on the frame it is run inside, and this shell is "
             f"not in one — nothing was changed.\n"
             f"  Run it in the window `charter <harness>` opened. Inside a tmux you "
             f"already have, charter binds no key, so typing it there is the route.\n"
             f"  charter docs show frame")
    return 1


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


def _drop_panels(socket: str, chat: str) -> None:
    """Tear the panels of *chat* down, because the client has just left its window.

    **`cmd_chat` step 2's call, made on the path that creates the situation the switch
    exists to manage** (#688). #668 states the rule and enforced it on the switch only: a
    background window keeps STALE geometry, so panels left running in one are not idle,
    they are rendering at a width that is no longer their window's — the exact defect
    `panel._component_text`'s `width=slots._width()` guard exists for. `charter claude`
    twice in one workspace left four panel processes doing that per abandoned chat, each
    polling `state.version` at `panel.TICK`, until an `F2` round trip happened to tidy
    them.

    **The socket is compared rather than trusted**, and that is #684's class rather than
    belt and braces: `_relayout_target` resolves the server from the frame's OWN record,
    while the chat this is about was found by reading a window on *socket* — and pane ids
    are PER-SERVER, so a record that disagreed would aim `kill-pane` at a real, live,
    unrelated pane on the other server. A disagreement means charter cannot say where this
    chat is, and the answer to that is to leave it alone.

    Every refusal is a quiet no-op: this is a window nobody is looking at, and a launch
    that has just built a frame must not fail over tidying one.
    """
    # No `if not chat` in front of this: `_relayout_target` reads the pane record through
    # `state.frame_dir`, which resolves through `contain.child` and refuses `""` outright,
    # so the empty answer is already the refusal below. A guard that passes only because a
    # different guard caught it is the shape this repository deletes.
    where = _relayout_target(chat)
    if where is None or where[0] != socket:
        return
    _apply_arrangement(chat, where=where, want=[])


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


def cmd_chrome(args) -> int:
    """`charter frame-chrome <off|dark|light>` — resurface THIS frame's panes, live.

    Started by a palette row (`frame/builtin_actions.py`), and typeable by hand from
    inside a frame. The frame is resolved from `$CHARTER_SESSION_ID` exactly as
    `cmd_density`, `cmd_toggle`, `cmd_palette` and `cmd_respawn` resolve theirs, since one
    bind text is shared by every frame on `SOCKET`.

    **Nothing splits, and that is the whole difference from `cmd_density`.** The surface
    is a pane option and tmux repaints from it on the spot — measured on an attached
    client, on tmux 3.7c and on tmux 3.2 (`tmuxctl.FLOOR`) alike: `set-option -p
    window-style bg=black` on a pane that had already been drawn put ``\x1b[40m`` on that
    client's wire with no `refresh-client` and no re-layout, 507 and 523 bytes
    respectively. So this records the word and sets two options per pane. There is no
    version gate because there is nothing to gate: the floor behaves identically.

    **charter.toml is not touched**, for `cmd_density`'s reason word for word: the
    committed file says what a frame STARTS at, this changes what one running frame IS,
    and the override goes in the frame's own state directory (`state.record_chrome`),
    which `state.reap` deletes entire when the frame ends. Relaunch and the configured
    look is back. That is also what makes the palette row honest for the operator this
    spec's §4 named — the one who upgrades into a look they dislike is one keystroke from
    changing it and never has to find a config key.

    **The word never reaches tmux, and `off` is a REMOVAL rather than a style.** A tmux
    style value is format-expanded at draw time (§4), so the config surface is a closed
    enum and `instance.chrome_options` maps it to constants charter holds. `off` maps to
    no styles at all — which on a LAUNCH means "set nothing" and here has to mean "unset
    what is set", or an operator who chose `off` would keep looking at the surface they
    just turned off. `_unsurface_argvs` is that half, and it is the reason this function
    cannot simply reuse `_surface_argvs`.

    **Always 0, and every refusal is a quiet no-op**, for `cmd_respawn`'s reason: this
    normally runs detached from a palette row, where the only screen left to report on is
    the agent's own — the one rectangle ADR 0018 says charter never draws in.

    Refusals, in order:

    * no `$CHARTER_SESSION_ID` — not fired from inside a frame at all;
    * a *level* outside `instance.FRAME_CHROME` — a closed set of three words charter
      wrote itself, so this can only be a hand-typed argument;
    * a frame with no recorded pane map — nothing to resurface. Unlike `cmd_density` this
      refuses on no tmux VERSION either: it splits nothing, so `_relayout_target`'s
      refusals are not this function's. It reads the version (#631 made which design
      applies depend on it) and the harness pane, and REFUSES on neither — an unreadable
      version takes the frame-wide design, which is correct on every tmux, and a frame with
      no record of its harness pane still gets every pane repainted.

    The word is recorded BEFORE the panes are touched, so a frame whose options fail
    halfway still splits its NEXT pane into the surface the operator asked for
    (`_split_panels` reads `_current_chrome`).
    """
    fid = os.environ.get("CHARTER_SESSION_ID", "")
    if not fid:
        return outside_a_frame("charter frame-chrome")
    # `cmd_density`'s split, for its reason: the empty *fid* is #734's silence and is
    # answered on the asker's own stderr, because there is no frame; a word outside the
    # closed set has a frame, so it is answered on that frame's own attention row.
    level = instance.chrome_level(getattr(args, "level", None))
    if level is None:
        _say_on_screen(fid, f"no chrome level "
                            f"{contain.readable(getattr(args, 'level', None))} — have: "
                            f"{', '.join(instance.FRAME_CHROME)}")
        return 0
    panes = state.panes(fid)
    if not panes:
        return 0
    state.record_chrome(fid, level)
    socket = state.frame_server(fid) or SOCKET
    # Asked once for the whole keypress rather than per pane, and asked at all because
    # #631 made this function version-dependent where it was not before: above
    # `tmuxctl.PANE_BORDER_FLOOR` each panel carries its own edges and the window's stay
    # bare, below it the frame-wide answer is the only one tmux can hold. `version()` is
    # one `tmux -V`; this path is a keypress that already spawns a process, and it is not
    # a refusal — an unreadable version answers the frame-wide design, which is correct on
    # every tmux (`_pane_borders_wanted`).
    #
    # Kept in a name rather than passed straight in, because `_chrome_argvs` below needs
    # the same answer for a different question (#716: which rows of `_CHROME` this tmux has
    # an option for). Two `version()` calls would be two subprocesses for one unchanging
    # fact on a path that is a keypress, and two answers that could disagree.
    v = tmuxctl.version()
    pane_borders = _pane_borders_wanted(v)
    # `_dress_window`'s own read, on the live path — one resolution for the whole keypress,
    # so a panel repainted at the top of this function and the rules repainted at the
    # bottom are two halves of one frame rather than two readings of one file.
    look = instance.look_of(config.FRAME)
    for slot, pane_id in panes.items():
        # `_PANE_ID_RE` is #475's rule applied to a value that arrived off DISK and is
        # about to be a tmux argv — the same check `_relayout_target` makes of the harness
        # pane, made here of each panel's. A frame whose map was truncated or hand-edited
        # skips that pane rather than handing tmux whatever the file said.
        if not _PANE_ID_RE.fullmatch(pane_id):
            continue
        # `.items()` rather than `.values()`, because the SLOT is what says whether this
        # pane has a colour of its own — the same one walk `_split_panels` makes at launch
        # (`instance.component_style`). Without it this loop would repaint a component's
        # own `bg` with the frame-wide word, and `off` would unset it outright: a colour
        # written in charter.toml disappearing on a keystroke with nothing to bring it
        # back until relaunch.
        bg = instance.component_style(config.FRAME, slot)["bg"]
        for argv in _resurface_argvs(socket=socket, pane_id=pane_id, chrome=level,
                                     bg=bg, pane_borders=pane_borders, look=look):
            tmuxctl.run("painting a panel's surface", argv)
    # And the frame's RULES, which are the surface's other half and are not any pane's
    # (`instance.border_bg`): a level change that repainted the panes and left the borders
    # would leave the seam this closed running between them — or, going the other way,
    # leave a grey rule around panes that had just gone back to the terminal's own colour.
    # `off` is the removal here too, and it needs no `-u`: these two options are charter's
    # own at every level (#514), so re-issuing them with no surface IS the removal.
    #
    # The harness pane is only the WINDOW selector — `-w -t <a pane id>` resolves to that
    # pane's window — and nothing is set on it, which is `_chrome_argvs`' own boundary.
    # Skipped rather than refused when the frame has no readable record of it: a frame
    # launched by a charter that predates `record_harness_pane` has none, and its panes are
    # already repainted above. `_PANE_ID_RE` is #475's rule about a value off disk that is
    # about to be a tmux argv, made here exactly as the loop above makes it.
    harness = state.harness_pane(fid) or ""
    if _PANE_ID_RE.fullmatch(harness):
        for argv in _chrome_argvs(
                socket=socket, harness_pane=harness, v=v, look=look,
                surface=None if pane_borders
                else instance.border_bg(config.FRAME, level)):
            tmuxctl.run("styling the frame's own rules", argv)
        # And the harness's OWN three edges, which above the floor no window option
        # reaches any more (`_harness_rule_argvs`). Here `-t <harness>` is the pane
        # itself rather than a window selector — the two border options only, never the
        # two that would paint inside it. `off` and a plane whose panels stopped agreeing
        # both arrive as no surface, and on this path that is the UNSET: a level change
        # that repainted the panels and left yesterday's colour on the rules around the
        # operator's own session is the same half-applied frame the loop above exists to
        # prevent.
        for argv in _harness_rule_argvs(
                socket=socket, harness_pane=harness, pane_borders=pane_borders,
                look=look, surface=instance.agreed_border_bg(config.FRAME, level)):
            tmuxctl.run("styling the rules around the pane charter does not paint", argv)
    return 0


def cmd_toggle(args) -> int:
    """`charter frame-toggle <component>` — show or hide ONE component, live.

    Fired by that component's own `bind -n` (`conf_text`), and typeable by hand from
    inside a frame. The chat is resolved by :func:`_pressers_chat` — the `--chat` the bind
    carries out of the presser's own window, falling back to `$CHARTER_SESSION_ID` —
    because one bind text is shared by every frame on `SOCKET` and a session now holds
    several chats.

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
    fid = _pressers_chat(args)
    # **The `if not fid` this function deleted, back — and back because it now carries a
    # consequence a test can pin** (#734). It was removed as a guard nothing could turn
    # red: with an empty id this command emitted nothing either way, so no mutation of it
    # was observable. It says something now, on a surface that is certainly the asker's,
    # and `test_a_frame_command_outside_a_frame_says_so.py` fails without it. It comes
    # FIRST, before the arrangement is even read: `charter frame-toggle repos` typed in an
    # ordinary shell names a component that IS in this plane's arrangement, so it used to
    # fall past the check below and die in `_relayout_target`, where "you are not in a
    # frame" and "this frame has no recorded harness pane" are one silence.
    if not fid:
        return outside_a_frame("charter frame-toggle")
    name = getattr(args, "component", None)
    frame = config.FRAME
    arrangement = instance.frame_arrangement(frame)
    if name not in arrangement:
        # **The refusal stays exactly as strict; only its silence goes.** This is the ONE
        # guard between an argv word and a `split-window` target and a hook's action text
        # (`test_component_toggle_keys.py`'s hostile-name class), and *name* still travels
        # no further than this line — what is added is a sentence about it, on the frame's
        # own row.
        #
        # Saying it was not affordable until #729. The surface was `display-message`, so a
        # committed or typed name reached a tmux FORMAT evaluator, and the message landed
        # on whichever client tmux picked. The row is charter's own pane, written through
        # `state.say`'s `contain.one_line` and read by this frame's own panel, so neither
        # is true of it any more.
        #
        # It also answers a live frame's dead key: a component's `bind -n` outlives an
        # edit to `charter.toml` that drops the component, so this is the one thing that
        # tells an operator why a key they configured has stopped doing anything.
        _say_on_screen(fid, f"no component {contain.readable(name)} in this plane's "
                            f"arrangement — have: {', '.join(arrangement)}")
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


def _pane_place(socket: str, pane: str | None) -> tuple[str, str] | None:
    """Which tmux SESSION and WINDOW *pane* is in, as ids — or ``None`` (#684).

    **The question `chats.py` cannot ask and `select-window`'s return code does not
    answer.** `chats.of_workspace` decides membership from RECORDS — `state.own_workspace`,
    which reads the pin the launcher wrote down, then the workspace the launch recorded —
    so two chats can share a roster while their windows are in two different tmux
    sessions. (Both records are a launch's; the per-session pointer `charter workspace use`
    writes was a third rung until #791, and this guard is what stood between it and a
    `select-window` across sessions. It is not softened by the rung going away — a
    migration or a hand-edited record reaches the same state.)
    The session is where a chat actually lives (`cmd_launch` makes the workspace the
    session), it is not written down anywhere charter can read, and it moves at runtime.
    So it is asked of tmux, at the moment it matters.

    ``None`` for every way that can fail to answer, because the caller does the same thing
    with each: refuse, and touch nothing. A *pane* is required rather than a name, and
    checked here rather than at the call site, for the reason `_relayout_target` checks the
    same record: the value arrives off disk, and `display-message -p -t ""` does not fail —
    **measured on tmux 3.7c: an unresolvable target answers rc 0, empty stdout and no
    stderr**, and an EMPTY target resolves to the current window, which would have this
    function report the asker's own place as the target's and agree that a switch may
    proceed.

    That measurement is also why the return code alone is not the guard: it is the same
    rc-0-on-the-wrong-thing shape as `select-window -t %N` succeeding against another
    session's pane, which is #684 itself.
    """
    if not _PANE_ID_RE.fullmatch(pane or ""):
        return None
    p = tmuxctl.run("asking which window a chat is in",
                    tmuxctl.server_argv(socket, "display-message", "-p", "-t", pane,
                                        _PANE_PLACE_FORMAT))
    # **The SHAPE of what came back, and deliberately not the return code.** A status of
    # 0 is what an unresolvable target answers with here, so branching on it would be the
    # same mistake one noun over from the one this whole function exists to correct. Both
    # halves are held: the session id is about to be a `-t` target (#475's rule at #475's
    # boundary), and a window id that came back empty beside a session id that did not is
    # an answer charter has no reading of — treated as "no window", which is the sentence
    # it would get anyway one line later, said for the right reason.
    fields = p.stdout.strip().split("\t")
    if len(fields) != 2:
        return None
    sid, wid = fields
    if not _SESSION_ID_RE.fullmatch(sid) or not _WINDOW_ID_RE.fullmatch(wid):
        return None
    return sid, wid


def _session_window(socket: str, session: str) -> str:
    """The window *session* is on right now, as tmux reported it.

    What "did the client actually move" is asked as. `select-window` sets the SESSION's
    current window and the clients attached to it follow, so this is the reading that says
    whether the switch happened — and it is a different fact from `select-window`'s exit
    status, which is 0 for a window of a session the asker is not in (measured on tmux
    3.7c: `select-window -t %N` against another session returned 0 and moved THAT session,
    while the asking client stayed exactly where it was).

    *session* is `#{session_id}`, held to :data:`_SESSION_ID_RE` by the only thing that
    produces one here (:func:`_pane_place`) before it ever reaches this. A session NAME
    would be the wrong currency twice over: it is renameable, and `-t api.1` is parsed by
    tmux as ``window.pane`` rather than as a name with a dot in it.
    """
    # Whatever came back, unexamined, and that is the honest shape rather than a missing
    # guard. The one caller compares this against a window id `_pane_place` has ALREADY
    # held to `_WINDOW_ID_RE`, so equality already implies this is one — and an answer
    # tmux could not resolve (rc 0, empty stdout, measured on 3.7c) compares unequal and
    # is read as "the client did not move", which is the safe direction because what
    # follows a move is a teardown. A regex here would be a second guard for what the
    # comparison already decides, and the deletion sweep is where that shows up.
    return tmuxctl.run("asking which chat this client is on now",
                       tmuxctl.server_argv(socket, "display-message", "-p", "-t",
                                           session, _WINDOW_ID_FORMAT)).stdout.strip()


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
def cmd_chat(args) -> int:
    """`charter frame-chat <chat id>` — put this client on another chat of this workspace.

    Started DETACHED by a palette row (:func:`_draw_palette`), and typeable by hand from
    inside a frame. The chat the keypress came from is :func:`_pressers_chat`'s, for its
    reason: one bind text is shared by every frame on `SOCKET`, and a session now holds
    several chats.

    **Four steps, and every one of them is an existing path** (spec §3.7):

    0. **Where both chats are, asked of tmux, before anything is aimed anywhere** (#684,
       :func:`_pane_place`). Not one of the four, and it is here because every one of them
       assumes something no record on disk can promise: that the target's window is a
       window this client can be moved to. `chats.of_workspace` matches RECORDS
       (`state.own_workspace`), and a record can name a workspace whose tmux session this
       chat's window is not in — a migration, a hand-edited file, or (until #791) a
       pointer `charter workspace use` wrote — so two
       chats can share a roster with their windows in two different tmux sessions — and
       `select-window` at another session's pane returns **0**, moves that session, and
       leaves this client exactly where it was. Steps 1 and 2 then reported a switch that
       had not happened and tore down the panels of the chat still on screen.
    1. `select-window` at the target chat's own harness PANE. A pane id resolves to that
       pane's window — measured on tmux 3.7c and on tmux 3.2 — which is why charter needs
       no window-id record beside the pane record it already keeps
       (`state.record_harness_pane`), and why there is no second thing to keep in step.
       Its status is checked and is **not** what says the switch worked: step 0's reading,
       taken again afterwards, is (:func:`_session_window`). A command that exits 0 having
       acted on the wrong target is indistinguishable from one that acted on the right
       one, so the teardown below is gated on the client having moved rather than on a
       return code.
    2. :func:`_apply_arrangement` with ``want=[]`` on the chat being left, which tears its
       panels down. Not a saving — a correctness rule. A background window keeps STALE
       geometry (§7.4, measured identically on 3.7c and 3.2), so panels left running there
       are not idle, they are rendering at a width that is no longer their window's, which
       is the exact defect `panel._component_text`'s `width=slots._width()` guard exists
       for.
    3. :func:`_apply_arrangement` on the chat being entered, which splits its panels into
       the window **tmux has just resized**. Measured here on 3.7c and 3.2: the very next
       tmux invocation after `select-window` already reports the target window at the
       client's size (200x50 → 100x29 with no sleep at all). So the panels are born at the
       true width and there is nothing stale to repair —
    4. **which is why the switch never asks for a `window-resized` hook, and must not.**
       On tmux 3.2 — `tmuxctl.FLOOR` — `set-hook -w window-resized` answers `invalid
       option`, rc=1: the hook does not exist. A switch that relied on it would be correct
       on the author's tmux and silently wrong on the floor charter promises to run on.
       `_apply_arrangement` measures and re-asserts the layout itself, which is the same
       thing a density change already does and not a second path.

    The bump is step 3's own last line (`_apply_arrangement`), so the new chat's panels
    repaint into the shape that has already settled — #411/#412's rule, and the reason
    this does not write a pointer of its own for anything to read.

    **Both re-layouts are attempted independently, and a failure of the first does not
    cancel the second.** They are two different frames' panes; the one the operator is now
    looking at is the one that matters, and abandoning it because the window they just
    left could not be tidied would leave them on a bare harness pane. The reverse — the
    old chat keeping its panels because the new one could not be laid out — is a frame
    that is merely untidy in a window nobody is looking at.

    **`select-pane` is deliberately not issued here and neither is anything to undo the
    palette's own close.** Measured on 3.7c and on 3.2: `select-pane -t %N` where `%N` is
    in another window of the same session sets that window's active pane and does **not**
    move the client — current window `@0` before and `@0` after. So `_close_palette`,
    which runs in the palette's own process after this one has been started and aims
    `select-pane` at the chat being LEFT, cannot drag the client back off the chat this
    just switched to. That measurement is what makes the switch safe to detach at all;
    without it the two processes would be racing for which window the operator ends on.

    **Always 0**, like every other `frame-*` command: this runs detached with its streams
    on `/dev/null` (`builtin_actions._spawn`), so a non-zero exit is read by nothing —
    and inside a `run-shell` it is what makes tmux print into the harness pane, the one
    rectangle ADR 0018 says charter never draws in. Every refusal goes to
    :func:`_say_on_screen` instead.
    """
    fid = _pressers_chat(args)
    if not fid:
        # **Not fired from inside a frame at all.** This guard has always been here —
        # what it may not do is reach :func:`_say_on_screen`, which is how `charter
        # frame-chat api.2` typed in an ordinary shell drew charter's refusal across
        # somebody else's frame. That was recorded as a property of the EMPTY `-t` target;
        # re-measured for #734 on 3.7c and at the 3.2 floor it is not — `-t` never chooses
        # the screen at all, `-c` does, and with neither, tmux draws on its own current
        # client. See :func:`outside_a_frame` for the measurement. What this branch does
        # now is say so on the asker's own stderr instead of returning 0 into their
        # silence (#734). `docs/frame.md` makes this command the documented fallback when
        # the palette's doorway is refused, so it is the last one that can afford to
        # answer a typo with a success status.
        return outside_a_frame("charter frame-chat")
    target = (getattr(args, "chat_id", None) or "").strip()
    # Asked again rather than trusted from the palette that spawned this: the same
    # command is typeable by hand on a name nobody drew, and `chats.check` is the one
    # rule both askers get (see `choose.switch_to`). It is also a second reading of a
    # plane that has moved since the palette opened — a chat reaped between the keypress
    # and here is refused with its own sentence rather than aimed at.
    out = chats.check(fid, target)
    if not out.ok:
        _say_on_screen(fid, out.message)
        return 0
    socket = state.frame_server(fid) or SOCKET
    # `chats.pane_of` and never a bare `state.harness_pane`: the read is the same one
    # `chats.check` made a moment ago, so this is a SECOND reading of a record that can
    # have moved — a reap, a relaunch — and the fallback the sweep found here turned that
    # into `select-window -t ""`. An empty tmux target resolves to the CURRENT window, so
    # charter would have reported a switch that did not happen and then torn this chat's
    # panels down around it. Its own sentence rather than the check's, because it is a
    # different fact: the check answered about a record, and this is about that record
    # going away underneath the answer.
    pane = chats.pane_of(target)
    if pane is None:
        _say_on_screen(fid, f"cannot switch: chat '{target}' stopped being one while "
                            "charter was switching to it")
        return 0
    # **Where the two chats actually are, asked of tmux before anything is aimed at
    # anything** (#684). `chats.check` has established that both are chats of one
    # workspace and — since this issue — that both are on one tmux SERVER, and neither of
    # those is "in one tmux session". `cmd_launch` makes the workspace the session, but
    # what membership is read from is RECORDS (`state.own_workspace`), and a record naming
    # a workspace whose tmux session this chat's window is not in puts `api.1` — a window
    # of session `api` — into `of_workspace("beta")` beside `beta.1`, in one roster.
    # (Two writers reached that state and both are gone: `switch.to_workspace` wrote the
    # pointer and the launch record until §4j made it a refusal, and `charter workspace
    # use beta` typed inside `api.1` wrote the pointer until #791 stopped membership
    # reading it. This guard is what stood between them and a `select-window` across
    # sessions, and it is not softened by either writer going away — a migration or a
    # hand-edited record reaches the same state with nothing having written anything.)
    here_place = _pane_place(socket, chats.pane_of(fid))
    there_place = _pane_place(socket, pane)
    if there_place is None:
        # The window is gone, learnt BEFORE anything is selected rather than from
        # `select-window`'s status afterwards. Same sentence it used to be said with, one
        # step earlier, and the step is what makes it honest: `display-message` answers rc
        # 0 with nothing for a target it cannot resolve, so this is the reading that says
        # the window is absent rather than the one that says a command exited.
        _say_on_screen(fid, f"cannot switch: chat '{target}' has no window any more")
        return 0
    if here_place is None:
        # The asker's own window cannot be found, so there is no session to compare
        # against and no way to tell afterwards whether the client moved. Refused rather
        # than guessed at, because the guess costs the panels of the chat the operator is
        # still looking at — `_relayout_target(fid)` is about to be handed the same
        # record, and a switch that cannot establish where it is standing must not tear
        # anything down.
        _say_on_screen(fid, "cannot switch: charter cannot find this chat's own window, "
                            "so it cannot tell whether a switch would move this client")
        return 0
    if here_place[0] != there_place[0]:
        # **The refusal #684 is about, and it is about an identity rather than a name.**
        # Measured on tmux 3.7c, own socket: `select-window -t %N` where `%N` is a window
        # of ANOTHER session returns **0** and moves that session's current window, while
        # the asking client does not move at all. So the status check below could never
        # have caught this, and what followed it was `_apply_arrangement(fid, want=[])` —
        # charter tearing down the panels of the chat still on screen, having reported a
        # switch that did not happen. That is verbatim the failure #668's own last commit
        # says it closed, closed there for the `select-window -t ""` spelling only.
        _say_on_screen(fid, f"cannot switch: chat '{target}' is a window of another tmux "
                            "session, so selecting it would move that session and leave "
                            "this client here")
        return 0
    selected = tmuxctl.run("switching to the chat",
                           tmuxctl.server_argv(socket, "select-window", "-t", pane))
    if selected.returncode != 0:
        # The one refusal `chats.check` deliberately does not guess at, and the reason it
        # does not: liveness is a question only tmux can answer, and answering it in the
        # check would be answering it at the instant the palette opened rather than at the
        # instant it matters. Nothing has been torn down at this point — the teardown is
        # below this line on purpose, so a chat whose window is gone costs the operator
        # nothing but a sentence.
        # No `contain.one_line` on *target*: `chats.check` has already held it to
        # `chats.ID_RE`, whose alphabet holds nothing `one_line` touches, so the call
        # would be one whose result is provably its argument — the sweep found it as a
        # survivor for that reason. Since #729 there is no tmux format on this path at
        # all: `_say_on_screen` writes the line into this frame's own state and its
        # attention panel draws it, so the containment this line does not need is
        # `contain.one_line`'s, applied once in `state.say`.
        _say_on_screen(fid, f"cannot switch: chat '{target}' has no window any more")
        return 0
    if _session_window(socket, here_place[0]) != there_place[1]:
        # **The teardown is gated on the client having MOVED, not on a command having
        # exited 0** (#684). The session check above is what stops charter selecting
        # somebody else's window; this is what stops it acting as though a selection
        # worked. They are two different properties and the second is the one the panels
        # ride on: everything below this line assumes the window the operator is looking
        # at is the target's, and `want=[]` on that assumption is the defect rather than a
        # tidy-up. A refusal here leaves both chats exactly as they were.
        _say_on_screen(fid, f"cannot switch: tmux selected chat '{target}' but this "
                            "client did not move, so this chat keeps its panels")
        return 0
    here = _relayout_target(fid)
    if here is not None:
        _apply_arrangement(fid, where=here, want=[])
    there = _relayout_target(target)
    if there is not None:
        _apply_arrangement(target, where=there,
                           want=_visible_now(target, config.FRAME))
    return 0


def _clients_on(socket: str, session: str) -> list[str]:
    """Every client attached to *session*, by `#{client_name}`.

    *session* is `#{session_id}` — held to `_SESSION_ID_RE` by the two things that produce
    one on this path (`_pane_place`, `_plane_session`) — for `_session_window`'s reason: a
    session NAME is renameable and `-t api.1` is parsed by tmux as ``window.pane``.

    ``[]`` for a server that would not answer and for a session nobody is on, because the
    caller does the same thing with both: there is no client to move, so nothing is moved
    and nothing is torn down. `.split()` rather than `.splitlines()` is
    `_workspace_to_focus`'s own reading of tmux's output — a client name holds no
    whitespace, and a server answering a bare newline has told us about no clients rather
    than about one with a blank name.

    **No branch on the return code, and its absence is a deletion rather than an
    omission.** This had `if out.returncode != 0: return []` and the deletion sweep
    reported it as a survivor — correctly: a `list-clients` that failed has an empty
    stdout, so `"".split()` is already `[]` and the branch could not change an answer.
    `_window_seats` and `_plane_session` say the same sentence about the same shape, and
    this repository deletes an equivalent mutant rather than documenting it. What the
    branch might have caught — output on stdout beside a non-zero status — is safe in the
    one direction that matters: a name that is not a client makes `switch-client -c`
    fail, and the reading afterwards then finds nothing of ours on the target and refuses
    without tearing anything down.
    """
    out = tmuxctl.run("asking who is looking at this workspace",
                      tmuxctl.server_argv(socket, "list-clients", "-t", session,
                                          "-F", "#{client_name}"),
                      timeout=5, report=False)
    return out.stdout.split()


def _open_workspace(fid: str, ws: str, *, socket: str,
                    window: str) -> tuple[str, str] | None:
    """Open *ws* on this plane without attaching to it — ``(session id, chat id)``, or
    ``None`` having said why.

    **§4k's other half, reached from a tab instead of from a command line.** `charter -w
    foo` "opens or focuses": `_workspace_to_focus` is the focus, and this is the open, for
    the surface where the operator did not type a command. The old refusal here named
    ``charter <harness> --workspace foo`` for the operator to type by hand; this runs it.

    **Why the reason the old refusal gave was wrong.** It said opening needs "a directory,
    an ordinal, a harness process and an `attach`", and that a switch running detached with
    its streams on `/dev/null` has "no terminal to attach anything to". The first three
    need no terminal, and the fourth is not wanted: arriving is `switch-client`, which
    #793 already does and which needs no tty at all. Measured on tmux 3.7c and at the 3.2
    floor, in a process with `start_new_session=True` and all three streams on
    `/dev/null`: `new-session -d`, `split-window`, a session option and `switch-client`
    all succeed, and the client lands with nothing killed. `_wants_attach` is what carries
    that decision into the launcher, and its docstring had already reserved the seam.

    **And #518 does not reach this, which is worth stating rather than assuming.** #518 is
    about `charter <harness>` resolving a workspace *silently*, and its "creating is not
    free" paragraph is about `charter workspace create` making a workspace DIRECTORY from a
    name the operator typed — "a picker that creates on a typo leaves litter". Nothing here
    types a name or creates a workspace: `switch.to_workspace` has already refused any name
    that is not already a directory under `workspaces/`, so the set of workspaces is the
    same after this call as before it. What is created is a CHAT in a workspace that
    exists, which is the thing §4k says `-w foo` creates. `_pin_workspace` is still not
    reached with ``picked``, so the launcher's own pointer — #518's "never ask twice" — is
    not written by a click either.

    **Which harness**, and it is the one question a tab cannot carry. The chat the operator
    clicked FROM recorded its own (`state.record_identity`, `$CHARTER_HARNESS`), and using
    it makes the click mean "another workspace, same tool" — the only answer available that
    the operator has actually expressed. A chat whose identity predates that record, or
    names a harness this charter cannot launch, falls back to `[harness] default` and, with
    neither, is refused by name rather than opened under something nobody chose.

    **Where.** The workspace's own directory, which is what `charter --workspace foo` typed
    in it would have used and what `state.record_cwd` is for. `cmd_launch` reads it from
    `os.getcwd()`, so it is `os.chdir`'d into and restored in a `finally` — `cmd_reopen`'s
    own arrangement, for its reason: this process goes on to re-lay-out two frames, and a
    launcher left standing in somebody else's directory is exactly the silent wrongness
    §4e's cwd item exists to close.

    **How big, and why *window* is a parameter rather than another read of disk.** The frame
    is laid out for the terminal it is about to be shown on, which is the one looking at the
    switching chat right now — so the size is measured off THAT window and handed to the
    launcher, which has no terminal of its own to measure (`_launch_size`).

    It arrives as an argument because the caller has already proved it. This was
    ``_window_size(socket, state.harness_pane(fid) or "")`` and the deletion sweep was right
    to survive the ``or ""``: it is unreachable, and asking why the two sides cannot differ
    is what found the real problem. `_switch_client` has already required
    `_pane_place(socket, state.harness_pane(fid))` to answer, and `_pane_place` refuses an
    empty target for its own measured reason — **an empty `-t` resolves to the tmux server's
    CURRENT window**, which on a socket serving eleven sessions from three projects is very
    likely another plane's. So the fallback could never fire, and if it ever had it would
    have sized this plane's new frame off another project's terminal. Taking the window
    `_pane_place` returned removes the unreachable branch and the second read of a record
    that could disagree with the one the caller checked.

    **The answer is re-asked, never inferred.** `_plane_session` is the same question that
    returned ``None`` a moment ago, put again now the session exists — so a launch that
    reported success but left nothing on the server is caught here rather than switching a
    client at a session id charter made up.
    """
    # **The name must be free on the whole MACHINE, not merely unresolved on this plane**,
    # and this guard is what keeps #793's cross-plane guarantee across the new door into
    # the launcher. `_plane_session` answered ``None`` a moment ago, and that means "this
    # plane cannot prove a session of this name is its own" — NOT "no such session". One
    # tmux server serves every plane on this machine (eleven sessions from three projects
    # on the operator's own socket the week this was written), and `cmd_launch` decides
    # between starting a session and joining one with `if session in live_sessions:`, a
    # NAME test over all of them. So an open that skipped this would add a chat window to
    # another plane's live session — another project's frame, across every isolation
    # boundary charter has — and then fail the `@charter_plane` veto on the way back out,
    # leaving the operator told the open failed with a window sitting in somebody else's
    # session. §3.3: "Open-or-focus must match on this plane's chat directories, never on a
    # live session name."
    #
    # Deliberately refusing a session this plane may actually own but cannot PROVE it owns
    # — a chat whose `harness_pane` record was lost, or one an older charter left unmarked.
    # Both are recoverable by attaching to it by hand; a window in another project's frame
    # is not, and the asymmetry decides which way an uncertain answer falls.
    if state.workspace_prefix(ws) in _live_sessions(socket):
        _say_on_screen(fid, f"cannot open '{ws}': a session of that name is already "
                            "running on this machine and this plane cannot prove it is "
                            f"its own — it is probably another plane's. Attach to it by "
                            f"hand if it is yours: tmux -L {socket} attach -t "
                            f"{state.workspace_prefix(ws)}")
        return None
    ident = state.identity(fid).get("CHARTER_HARNESS", "")
    h = next((x for x in harness.all() if x.name == ident and x.cli_name), None)
    if h is None:
        fallback = (config.HARNESS or {}).get("default")
        h = next((x for x in harness.all() if x.cli_name == fallback), None)
    if h is None:
        _say_on_screen(fid, f"cannot open '{ws}': this chat records no harness this "
                            "charter can launch, and this plane declares no `[harness] "
                            "default`")
        return None
    from types import SimpleNamespace
    where = workspace.workspace_dir(ws)
    root = where if where.is_dir() else config.ROOT
    # Every field `cmd_launch` and `_choose_workspace` read is named rather than left to a
    # `getattr` default — `_reopen_args`'s rule, which is what makes this a contract that
    # can be read instead of a puzzle. `workspace` is set outright so `_picker_wanted` can
    # never raise a prompt on a path with no operator waiting, `pick` is false for the same
    # reason and for #518's pointer, `rest` is empty so §4k's open-or-focus gate stays
    # reachable and the harness starts at its own prompt with nothing sent to it, and
    # `attach` is the seam.
    args = SimpleNamespace(harness=h.cli_name, rest=[], no_frame=False,
                           workspace=ws, pick=False, attach=False,
                           size=_window_size(socket, window))
    here_dir = os.getcwd()
    try:
        os.chdir(root)
    except OSError:
        _say_on_screen(fid, f"cannot open '{ws}': charter cannot enter "
                            f"{contain.readable(str(root)) or 'its directory'}")
        return None
    try:
        rc = cmd_launch(args)
    finally:
        try:
            os.chdir(here_dir)
        except OSError:
            pass
    opened = _plane_session(socket, ws=ws)
    if opened is None:
        # Said rather than swallowed, and this is the one outcome an operator cannot see
        # for themselves: `cmd_launch`'s own `util.err` went to `/dev/null` with everything
        # else this process writes, so without this line a click would simply do nothing —
        # the exact complaint this change exists to answer, arriving through a new door.
        _say_on_screen(fid, f"could not open workspace '{ws}' — the launcher returned "
                            f"{rc} and started no session")
        return None
    return opened


def _switch_client(fid: str, ws: str, *, said: str) -> None:
    """Move the client(s) reading chat *fid* to this plane's workspace *ws* — §4b.

    **The operator's own requirement, and the whole of it**: *"switching workspace means
    keep the opened chat open in the background, so a user can simultaneously run many
    harnesses in one charter environment. Changing workspace does not mean stopping the old
    chat session."* So this kills nothing and starts nothing. Measured on tmux 3.7c and at
    the 3.2 floor, with a real pty client on a two-window session: after `switch-client -c
    <client> -t $N` every pane on the server is still there with the same pid and
    `pane_dead=0`, and the `attach` process itself is still alive. It is `cmd_chat`'s four
    steps one scope out, and each of them changes in exactly one way:

    0. **Where both ends are, asked of tmux before anything is aimed anywhere** (#684).
       Here is `_pane_place` on this chat's harness pane; there is :func:`_plane_session`,
       which answers for THIS PLANE and refuses to resolve a session name — see it for why
       a name cannot be used, and :data:`_PLANE_OPTION` for what it costs when it is.
    1. **`switch-client -c <client>`, once per client on this session, and never without
       `-c`.** tmux picks its own "current client" for a `switch-client` that names none,
       and on a socket serving eleven sessions from three projects that client is very
       likely somebody else's — #734 measured exactly that leak for `display-message` and
       the cost here is higher than a misdrawn line: it would move another operator's
       terminal off what they were reading. Charter has no presser to name (`cli.py`'s
       palette `client` positional is accepted and ignored since #729, and a panel process
       is not a `run-shell` child of a keypress at all), so the clients it moves are the
       ones it can prove are looking at THIS chat: every client attached to this chat's
       session. They already share one current window (§2.10), so they were already looking
       at the same thing, and moving them together keeps them so.
    2. **`_apply_arrangement(want=[])` on the chat being left**, which is #686's rule
       unchanged: a background window keeps STALE geometry (§7.4, measured identically on
       both versions), so panels left running in one are not idle, they are rendering at a
       width that is no longer their window's. Only this chat has panels to lose — every
       other chat of the workspace is a background window and lost its own the same way.
    3. **`_apply_arrangement` on the chat tmux LANDED on**, unconditionally, which is §4b's
       "#686's treatment one scope out". Which chat that is, charter does not choose:
       `switch-client` restores the target session's own last active window, so the landing
       chat is read back off the server (:func:`_chat_showing`) rather than guessed from
       the seat :func:`_plane_session` happened to match. Re-dressing is not optional and
       not conditional on the geometry having changed — the panels there were torn down
       when that workspace went to the background, so they have to be split into a window
       that tmux has just resized, and that is the same thing `cmd_chat` step 3 does.

    **The teardown is gated on the client having MOVED, not on a command having exited 0**
    — #684's rule, re-asked here because the failure it names exists here too:
    `switch-client` against a client that is not attached, or a session that went away
    between two calls, returns 1, and a partial success across several clients returns 0.
    So the reading afterwards is `list-clients` on the TARGET, and `display-message -p -c
    <client>` is deliberately not used for it: measured at the 3.2 floor, that answers an
    empty string for a client that has demonstrably moved, so a check built on it would
    refuse every switch on the older tmux and pass on the newer.

    **Nothing on this path writes a record**, which is §4j surviving contact with a switch
    that now does something: no `record_workspace`, no `workspace.set_active`, no pointer
    of any kind. The chat left behind is still its workspace's, and the chat arrived at was
    always its own.

    *said* is `switch.to_workspace`'s own success sentence, carried in rather than spelled
    again: one switch says one thing, and the sentence belongs beside the refusals it is
    the alternative to. It is said HERE rather than by the caller because only this
    function knows which chat the operator ended up on, and a notice is drawn by a panel
    out of the frame's own state.

    **Called only after `switch.to_workspace` has said yes**, which is what lets every
    message below interpolate *ws* raw: that check holds it to `workspace.valid_name`,
    whose alphabet (`instance.WORKSPACE_NAME_RE`) has no whitespace and no control
    character in it, so a `contain.one_line` here would be a call provably equal to its
    argument — the shape this repository's deletion sweep reports and this repository
    deletes.
    """
    socket = state.frame_server(fid) or SOCKET
    if tmuxctl.is_operator_socket(socket, own=SOCKET):
        # **This is #812's refusal, and it is still here because the reasoning was never
        # the defect.** What was wrong was the premise: `is_operator_socket` was a
        # leading-slash test, and a chat launched inside one of charter's own panes
        # records charter's own socket as the absolute path `$TMUX` spells it with — so
        # every tab in that chat, the one back included, was refused by this line for a
        # server charter had started itself. It asks about the SERVER now, so a frame on
        # charter's private socket reaches the switch whichever way its record spells it,
        # and a frame genuinely inside somebody else's tmux is refused exactly as before.
        #
        # **A workspace is a tmux session only on charter's OWN server** (§2.1), and such
        # a frame is not on it. Inside an operator's tmux every chat charter opens is a
        # `new-window` in the session that operator was already in (`layout.window_argv`,
        # `_launch_in_operator_tmux`) — whatever workspace it names — so there is no
        # session for another workspace to be, and the two things `switch-client` could
        # do there are both wrong: refuse "already in that workspace" for a workspace this
        # frame is not in, or move the operator's client between two tmux sessions of
        # their own that charter has no business having an opinion about. Refused by name
        # instead, with the route that does work — which is `chats.check`'s own
        # "not on this frame's tmux server" one noun out (#684).
        _say_on_screen(fid, "cannot switch: this chat is a window in your own tmux, "
                            "where a workspace is not a session — open the other "
                            f"workspace with `charter <harness> --workspace {ws}`")
        return
    here = _pane_place(socket, state.harness_pane(fid))
    if here is None:
        # `cmd_chat`'s own sentence, for its own reason: with no reading of where this
        # client is standing there is no way to tell afterwards whether it moved, and a
        # switch that cannot establish that must not tear anything down.
        _say_on_screen(fid, "cannot switch: charter cannot find this chat's own window, "
                            "so it cannot tell whether a switch would move this client")
        return
    there = _plane_session(socket, ws=ws)
    if there is None:
        # **What a workspace with no session yet is answered with, and it is now an OPEN
        # rather than a refusal** — §4k's "if it is live, attach to it; if not, open it and
        # leave the others", reached from a tab instead of a command line. This used to
        # print `charter <harness> --workspace <ws>` for the operator to go and type, on
        # the grounds that opening ends in an `attach` and a detached switch has no
        # terminal for one. Measured on 3.7c and at the 3.2 floor, that is false twice
        # over: a tty-less process creates sessions and panes perfectly well, and the
        # attach is not wanted anyway — arriving is `switch-client`, three lines below.
        # See :func:`_open_workspace`, which also says why #518 does not reach a name the
        # operator picked off a list of workspaces that already exist.
        #
        # **Asked before the open, because an open is not free.** The refusal below for
        # "nobody is attached" is #411's shape — reporting a switch that moved nothing —
        # and it used to sit after a `_plane_session` read that costs one tmux call. An
        # open costs a harness process and a chat ordinal, so a frame nobody is looking
        # at (the operator detached, or an agent with no terminal is driving it) must be
        # refused BEFORE anything is started rather than after. Spelled here rather than
        # hoisted over the whole function: the order of the other refusals is #793's and
        # pinned by its own tests, and moving this one over `already in workspace` would
        # change what a detached frame is told about a workspace it is already in.
        if not _clients_on(socket, here[0]):
            _say_on_screen(fid, "cannot switch: no terminal is attached to this "
                                f"workspace, so charter will not open '{ws}' — there "
                                "would be no client to move into it")
            return
        there = _open_workspace(fid, ws, socket=socket, window=here[1])
        if there is None:
            return
    if there[0] == here[0]:
        # Records can disagree with tmux: `switch.to_workspace` refused this by NAME a
        # moment ago, off `state.workspace_for`, and this is the same question asked of
        # the server. Switching a client to the session it is already on is a no-op that
        # would then tear this chat's panels down and re-dress them for nothing.
        _say_on_screen(fid, f"already in workspace '{ws}'")
        return
    moving = _clients_on(socket, here[0])
    if not moving:
        # Nobody is attached — the operator detached, or this frame is being driven by an
        # agent with no terminal on it. There is no client to move, and reporting a switch
        # that moved nothing is #411's shape arriving through a success.
        _say_on_screen(fid, "cannot switch: no terminal is attached to this workspace, "
                            "so there is no client to move")
        return
    for client in moving:
        tmuxctl.run("switching this terminal to another workspace",
                    tmuxctl.server_argv(socket, "switch-client", "-c", client,
                                        "-t", there[0]),
                    report=False)
    if not set(moving) & set(_clients_on(socket, there[0])):
        _say_on_screen(fid, "cannot switch: tmux did not move this terminal to "
                            f"'{ws}', so this chat keeps its panels")
        return
    landed = _chat_showing(_window_seats(socket, "finding the chat this switch landed on"),
                           there[0])
    left = _relayout_target(fid)
    if left is not None:
        _apply_arrangement(fid, where=left, want=[])
    # Both re-layouts are attempted independently and a failure of the first does not
    # cancel the second — `cmd_chat`'s rule, for its reason: the frame the operator is now
    # looking at is the one that matters, and abandoning it because the workspace they just
    # left could not be tidied would leave them on a bare harness pane.
    arrived = _relayout_target(landed) if landed else None
    if arrived is not None:
        _apply_arrangement(landed, where=arrived,
                           want=_visible_now(landed, config.FRAME))
    # On the chat the operator is now looking at, for `_draw_palette`'s reason: the notice
    # is drawn by a panel out of a frame's own state, so it has to be written to the frame
    # they will be reading a moment from now. A workspace whose landing chat charter could
    # not name is left unsaid rather than announced on a frame nobody is looking at — the
    # client visibly moved, which is the report, and `state.say` on the chat being left
    # would write into panels that are being torn down two lines above.
    if landed:
        _say_on_screen(landed, said, ok=True)


def _pressers_chat(args) -> str:
    """Which chat the keypress that started this process was fired in.

    **One bind text is shared by every frame on `SOCKET`, and a session now holds several
    chats, so neither half of this is optional.** `--chat` is `#{@charter_chat}` expanded
    by tmux in the presser's own window (`conf_text`), which is the only source that can
    tell two chats of one session apart: measured on tmux 3.7c and on 3.2, a `run-shell`
    child — which every bind's action is — reads the SESSION's `set-environment` and never
    the window's `-e`, so `$CHARTER_SESSION_ID` hands every chat in a workspace the same
    id.

    `$CHARTER_SESSION_ID` stays as the fallback, and it is a real one rather than
    politeness. A frame launched by a charter that predates the option has no
    `@charter_chat` on its window — and its bind text is REPLACED by this one the moment a
    newer frame launches on the shared server, since a key table is server-wide — so that
    frame's `F2` starts arriving here with an empty `--chat` and has to resolve the way it
    always did. It is also what a hand-typed `charter frame-toggle repos` inside a frame
    resolves through.

    A value the option could not have held is not one: `_FRAME_ID_RE` is the alphabet a
    frame id travels to tmux under, and this is the far end of the same trip, so a
    `--chat` outside it falls back rather than being carried into a state path. Empty is
    the ordinary case of that (a window with no such option expands to nothing).
    """
    chat = (getattr(args, "chat", None) or "").strip()
    # `fullmatch` alone, with no `chat and` in front of it: `_FRAME_ID_RE` is `+`, so it
    # already refuses the empty string, and a truthiness test would be a second guard no
    # mutation could turn red — the shape `state.frame_workspace` names for `valid_name`
    # and the one the deletion sweep found here.
    if _FRAME_ID_RE.fullmatch(chat):
        return chat
    return os.environ.get("CHARTER_SESSION_ID", "")


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

    **Pressing the hotkey while the palette is already open REOPENS it** (#739). A
    `bind -n` is the ROOT key table, so tmux matches `F2` before any byte reaches the
    palette's pane — which is measured only for a key tmux's own table claims, and is the
    same property that makes the escape hatch work against a wedged overlay. So the second
    press runs this function again, and :func:`_close_open_overlays` is what stops it
    leaving the first palette standing as an invisible pane holding a live process.

    This used to be left unguarded on purpose, and the reason was right about the guard it
    was imagining: "every cheap test for 'is one already open' is a stale-state problem of
    its own", and a guard that REFUSED after an escape-hatch press would be worse than the
    state it prevented. Neither objection reaches the sweep. The mark is a tmux **pane**
    option, so it is tmux's state and dies with the pane — there is nothing charter can
    believe that is out of date — and nothing is ever refused: every press answers with a
    palette.

    **``--pane`` is this process being handed a rectangle, so it is where the pane is
    claimed** (`frame/pane.py`, #606/#611) — and the claim is HERE rather than inside
    :func:`_draw_palette` for the reason `panel.run` is split from `panel._run`, which is
    the same split one surface over. The claim has to sit above `builtin_actions.build`,
    which is the first instant a provider's module can execute in this process
    (`registry.Registry.add` → `importlib.import_module`); `_draw_palette`'s own `try`
    starts *below* that build and its `finally` is `_close_palette`. Folding the claim
    into that `try` would have moved four lines under a `finally` that kills the palette's
    pane, changing when `_close_palette` runs to satisfy an ordering — so the claim gets
    the enclosing function instead, exactly as `panel.run` does, and `_draw_palette` keeps
    the shape it was reviewed in.

    **And the other branch claims nothing, deliberately.** Without ``--pane`` this is a
    `run-shell` child whose stdout is a pipe: it was given no rectangle, it paints in
    none, and `frame/pane.py`'s fallback is written for exactly that process. A claim
    there would record a pipe as "the pane this process was given", which is the sentence
    that module exists to keep true.
    """
    if getattr(args, "pane", False):
        held = pane.claim()
        try:
            return _draw_palette(args)
        finally:
            pane.release(held)
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

    The chat comes from :func:`_pressers_chat`, so `F2` in chat two opens chat two's
    palette rather than whichever chat's id the session variable happens to hold. The
    pane this carves is then told that id explicitly (`_relayout_pane_env`), which is what
    makes every row the palette runs — `frame-density`, `frame-chrome`, `frame-switch` —
    act on the chat the key was pressed in.
    """
    fid = _pressers_chat(args)
    harness = state.harness_pane(fid) or ""
    socket = state.frame_server(fid) or SOCKET
    v = tmuxctl.version()
    if v is None:
        return 0
    _close_open_overlays(socket, harness=harness)
    argv = overlay.open_argv(
        socket, harness=harness,
        command=util.self_relaunch_argv("frame-palette",
                                        "--pane"),
        env=_relayout_pane_env(fid, v))
    if argv is None:
        return 0
    opened = tmuxctl.run("opening the palette", argv)
    for cmd in overlay.modal_argvs(socket, harness=harness,
                                   overlay_pane=opened.stdout.strip()):
        tmuxctl.run("making the palette the surface", cmd)
    return 0


def _close_open_overlays(socket: str, *, harness: str) -> None:
    """Close any palette already open on this frame's window, before opening another.

    **`F2` while a palette is up means REOPEN**, and #739 is what it meant before. A
    `bind -n` is the root key table, so tmux matches the key before any byte reaches the
    overlay's pane — that is not a defect to be fixed at the bind, it is the same property
    that makes the escape hatch work against a wedged surface. So the second press really
    does run this command, and what it must not do is leave the first palette standing:
    Escape closes the one holding the keyboard, and the other stayed as a blank five-row
    pane holding a live Python process, six rows off the harness, for the life of the
    frame — invisible, since a blank gap above the repo table's rule reads as empty
    terminal. `F12` is `select-pane` and does not clear it; a resize does not; no palette
    row does. Only tmux's own prefix + `x` did, which `charter frame` neither binds nor
    documents.

    **Reopen rather than no-op or toggle, and the reason is the double press itself.**
    Both of those require charter to BELIEVE a palette is open, and being wrong makes the
    key do nothing — the worst possible answer for a key whose response is not instant.
    Opening the palette splits a pane, starts a Python process, imports every installed
    provider and paints; #728 measures the neighbouring launch path at ~1.8 s of blank
    terminal, and the palette's own open was reported at about three seconds. **The
    argument does not rest on any particular number, and deliberately not on #729's
    four-second freeze, which #763 has since removed.** It rests on the shape: an operator
    who presses `F2` twice is precisely an operator who thinks the first press did not
    register, and answering them with no palette at all is the complaint they already had,
    made permanent. A faster palette makes that press rarer; it does not make refusing it
    the right answer. This answers every press with a palette. It is also the *fresher* one: the catalogue resolves the
    density, the surface, the workspace and the persona at the moment it opens, so a
    reopen re-reads a plane that may have moved under the first.

    **Nothing here is charter's own record of what is open**, which is what the old
    refusal to guard was right about: `cmd_palette`'s docstring argued that "every cheap
    test for 'is one already open' is a stale-state problem of its own", and a value
    charter writes down can outlive the pane it describes — the hatch option deliberately
    does. `overlay.OVERLAY_OPTION` is a **pane** option, so the answer is tmux's and dies
    with the pane. There is no state to be stale, and this never refuses anything: a
    listing that comes back empty, unparseable, or naming panes that have since gone costs
    one no-op and the palette opens regardless.

    **The palette this kills does not fight back**, and that is measured rather than
    assumed. `_draw_palette` hands the pane back from a ``finally``, so the obvious worry
    is a swept process running `_close_palette` on its way out: that would `select-pane`
    the harness — pulling focus off the palette being opened — and re-arm the hatch with
    NO overlay, leaving `F12` unable to close the new one. It cannot happen. `kill-pane`
    terminates the pane's program without Python cleanup; measured directly, on a pane
    running a process whose ``finally`` writes a file, the file is never written. The
    ``finally`` runs when the palette CHOOSES to close, which is the only time anything
    needs it to.

    **Every overlay on the window, not only the last one.** The reachable route is a
    double `F2`, but it is not the only one — any second open against a live palette does
    it, including a `charter frame-palette` typed in the harness and a second client
    attached to the same session pressing the key — and a frame that has been running
    since before this fix can already be carrying orphans. Sweeping what tmux says is
    there heals those too, rather than making one route safe and calling the class closed.
    """
    listing = overlay.live_argv(socket, harness=harness)
    if listing is None:
        return
    found = overlay.live_panes(tmuxctl.run("finding an open palette", listing).stdout)
    argv = overlay.sweep_argv(socket, found)
    if argv is not None:
        tmuxctl.run("closing the palette already open", argv)


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
    reg = builtin_actions.build(fid, current_density=_current_density(fid),
                                current_chrome=_current_chrome(fid))
    snapshot = gather.cached(fid) or {}
    opened: list[choose.Roster] = []
    try:
        surface = palette.Palette(
            # `leave.open_rows` LAST, and that ordering is the guard §4i's "warns and
            # proceeds" needs: the cursor starts on the first row that can run, so a
            # destructive row at the top of the list would be one `F2 Enter` from stopping
            # the plane. Every harmless row charter has keeps the top.
            catalogue=(choose.open_rows(fid)
                       + palette.rows(reg.offers(fid=fid, snapshot=snapshot))
                       + leave.open_rows(fid)),
            query_only=lambda: _name_rows(fid, opened),
            mouse=True)
        def _then(row):
            # Two ways a chosen row does NOT end the palette, asked in the order they
            # cost: a doorway replaces the surface, and a repeatable action runs and
            # leaves this one standing. Explicit rather than `_picker(...) or _again(...)`
            # — both answer a `Surface`, and a surface's truthiness is not a thing this
            # file gets to assume on `own_the_tty`'s behalf.
            nxt = _picker(row, fid, opened)
            if nxt is not None:
                return nxt
            return _again(row, surface, reg, fid=fid, snapshot=snapshot)

        chosen = palette.own_the_tty(surface, then=_then)
        if chosen is None:
            return 0
        picked = _chosen_name(chosen, opened)
        if picked is not None:
            noun, name = picked
            out = choose.switch_to(noun, fid, name)
            if out.ok and noun == choose.CHAT:
                _start_chat_switch(fid, name)
            if out.ok and noun == choose.WORKSPACE:
                # §4b, and the one row whose outcome this function may not say. A
                # workspace switch ends on a chat of ANOTHER session — whichever window
                # tmux restores — and nothing here knows which that is until the switch
                # has happened. So the sentence goes with the work: `_switch_client` says
                # it on the chat the operator landed on, out of `switch.to_workspace`'s
                # own message, and this returns having started it. That is the same
                # promise a started ACTION makes a few lines below — what it started
                # surfaces through `inflight`, not through a second clock here.
                _start_workspace_switch(fid, name)
                return 0
            # **Which frame's row, and it is not always this one.** The notice is drawn by
            # a panel out of a frame's own state, so it has to be written to the frame the
            # operator will be LOOKING at a moment from now. A chat switch that took moves
            # the client to a sibling frame — `choose.py`'s "the frame IS the chat" — and
            # a chat's name IS its frame id, so that is the id to write. Every other
            # outcome, refused chat switches included, leaves the operator on this frame.
            shown_on = name if (out.ok and noun == choose.CHAT) else fid
            _say_on_screen(shown_on, out.message, ok=out.ok)
            return 0
        if choose.noun_of(chosen) is not None:
            # A picker row that never opened its picker: `_picker` refused it, which it
            # does for exactly one reason and always with that reason in the row's note.
            _say_on_screen(fid, chosen.note)
            return 0
        # **The confirmation's own rows, and they are asked about before `invoke`** — none
        # of them is an action id, and handing `leave:quit:go` to `ActionRegistry.invoke`
        # would report "no such action" for a keypress that means "stop the plane". The
        # per-chat rows are `refused=True`, so Enter does not land on one; a mouse click
        # still can, and it says nothing rather than doing something.
        for verb in (leave.QUIT, leave.CLOSE):
            if leave.goes_through(chosen, verb):
                _start_leaving(fid, verb)
                return 0
        if leave.is_row(chosen):
            # A doorway `_picker` refused (its note says why) or one of the warning's own
            # per-chat rows, which are `refused=True` and describe rather than do. The note
            # is said in the first case and there is nothing to say in the second — and
            # `verb_of` alone tells them apart, because a per-chat row's id is
            # `leave:<verb>:c<n>` and only a DOORWAY's is `leave:<verb>`. A `chosen.note and`
            # in front of this was the sweep's finding and its own answer: a doorway that
            # `_picker` refused always carries the note that says why, so the conjunct could
            # never decide anything.
            if leave.verb_of(chosen) is not None:
                _say_on_screen(fid, chosen.note)
            return 0
        inv = reg.invoke(chosen.id, fid=fid, snapshot=snapshot)
        inv.join(timeout=_ACTION_START_GRACE)
        if not inv.started:
            _say_on_screen(fid, inv.reason)
    finally:
        _close_palette(socket, harness=harness,
                       overlay_pane=os.environ.get("TMUX_PANE", ""))
    return 0


def _start_chat_switch(fid: str, chat: str) -> None:
    """Start :func:`cmd_chat` for *chat*, detached, and return having started it.

    **Fire-and-report, and this is the one place the chat switch could have failed to
    be** (§4g). Every other row the palette runs is a detached process for a measured
    reason `builtin_actions._spawn` records: the palette closes the instant it has
    invoked, `kill-pane` hands SIGHUP to that pane's process group, and a switch that ran
    in this process would be racing its own teardown for the last three of its four tmux
    calls. Detaching also keeps the palette's promise that a row returns immediately: the
    switch is ~20 tmux round trips, and a palette that sat through them would be a pane
    the operator is watching do nothing.

    **What makes detaching SAFE is a measurement, not an assumption.** On 3.7c and on
    3.2, `select-pane -t %N` on a pane in another window of the same session does not
    move the client (current window `@0` before and after) — so `_close_palette`, which
    runs from the `finally` below and aims `select-pane` at the chat being LEFT, cannot
    pull the client back off the window `cmd_chat` is switching to. The two processes are
    not racing for the operator's screen; only one of them moves it.

    `builtin_actions._spawn` and not a `Popen` here, because *which* frame the child acts
    on must be stated rather than inherited: this process is a `run-shell` child of a tmux
    server shared between every frame on the machine, and its own `$CHARTER_SESSION_ID`
    may be another chat's (`state.record_identity` measures exactly that). The child is
    handed the PRESSER's chat as its own id and the target on its argv, which is the same
    split `frame-toggle`'s `--chat` already makes.
    """
    builtin_actions._spawn(util.self_relaunch_argv("frame-chat", chat), fid=fid)


def _start_workspace_switch(fid: str, ws: str) -> None:
    """Start :func:`cmd_switch` for workspace *ws*, detached, and return having started it.

    :func:`_start_chat_switch` one noun out, and every word of its argument applies
    unchanged: the palette closes the instant it has invoked, `kill-pane` hands SIGHUP to
    that pane's process group, and a switch that ran in this process would be racing its
    own teardown for the tmux calls that matter. The frame is stated on `_spawn`'s own
    `$CHARTER_SESSION_ID` rather than inherited, because this is a `run-shell` child of a
    server shared between every frame on the machine.

    **`frame-switch --workspace`, which is the same front door the `workspaces` bar
    clicks** (`frame/builtins._WORKSPACE_SWITCH`) and the same one an operator types. One
    argv rather than a second in-process path is what keeps a click, a palette row and a
    typed command from drifting into three switches.

    Safe to detach for the measurement `_start_chat_switch` records: `_close_palette` aims
    `select-pane` at the chat being LEFT, and on 3.7c and at the 3.2 floor `select-pane`
    on a pane in another window does not move a client — so it cannot pull the operator
    back off the workspace this is switching them to. What it CAN do is race the teardown
    of this chat's panels, and that is harmless in the one direction: `_close_palette`
    kills the overlay pane, `_apply_arrangement(want=[])` kills the panel panes, and
    neither is the other's.
    """
    builtin_actions._spawn(
        util.self_relaunch_argv("frame-switch", "--workspace", ws), fid=fid)


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
    # **A refused row does not open, whichever doorway it is** — `choose.open_rows`' rule,
    # and the same one: a surface over a target charter cannot name would be an offer it
    # already knows it cannot honour. `None` sends the row back to :func:`_draw_palette`,
    # which says the note on the operator's own screen.
    #
    # `row.note` alone, and the `verb is not None and` that used to be in front of it is
    # gone: the deletion sweep could not turn it red, and it was right — the branch below
    # already answers `None` for every noun row with a note, so the conjunct only ever
    # restated a decision the next four lines make. A guard that passes because a DIFFERENT
    # guard caught the case is the shape this repository deletes rather than documents.
    if row.note:
        return None
    verb = leave.verb_of(row)
    if verb is not None:
        # **§4f's warning, drawn at the moment the operator is deciding.** The plan is built
        # HERE and not when the palette opened, so the rows describe the plane as it is
        # under the keypress — and so an `F2` pressed to reach `detach` never pays for a
        # scan of the frame root. The confirming row is the last one, under the list it is
        # about, and every chat row above it is `refused` (see `frame/leave.py`).
        servers = _plane_servers()
        live, _windows, _active = _plane_live(servers)
        # `focus` is not read on this path — the confirmation draws rows and writes no
        # manifest — so the `or ""` that used to be here was a fallback nothing could
        # observe, and the sweep said so. The one caller that DOES record a focus is
        # `cmd_quit`, which spells its own.
        p = leave.plan(live=live, focus=state.own_workspace(fid),
                       only=fid if verb == leave.CLOSE else "")
        return palette.Palette(catalogue=leave.confirm_rows(p, verb=verb),
                               label=verb, mouse=True)
    noun = choose.noun_of(row)
    if noun is None:
        return None
    return palette.Palette(catalogue=_roster(noun, fid, opened).rows,
                           label=noun, mouse=True)


def _again(row, surface, reg, *, fid: str, snapshot) -> "palette.Palette | None":
    """Run *row* and hand *surface* back to be drawn again — or ``None`` for every row
    that is not repeatable, which ends the palette exactly as before. **#746.**

    **One palette opening, not one per row.** `repo: select the next row` and its
    `previous` twin are the only actions charter offers whose natural use is *repeated*,
    and with `[frame] mouse = false` shipped as the default they are the repo table's only
    interaction model. Measured by the report: moving the selection three rows down a
    fourteen-row table cost fourteen keystrokes and three ~3-second cycles, because each
    Enter closed the pane, killed the process and re-split, re-imported and re-drew the
    whole surface for the next one. Everything about that cost is the palette OPENING; the
    action itself is `state.record_selection` plus `state.bump`, two file writes, done in
    this process (`builtin_actions._select` says why it does not spawn).

    **The query and the cursor are deliberately left alone.** `Palette._refilter` resets
    the selection to the top on every edit, and calling it here would move the cursor off
    the row the operator is holding Enter on — turning a repeat into "type the filter
    again". So this writes `heading` directly and touches nothing else on the surface:
    what comes back is the same list, the same filter, the same row under the cursor.

    **The heading is where the outcome goes, and that is forced rather than chosen.** The
    overlay pane is `resize-pane -Z`'d over the whole window (`overlay.modal_argvs`, whose
    own measurement is that a zoomed pane's siblings are not drawn) — so the repo table
    the selection is moving through is NOT on screen behind the palette. A repeat whose
    only feedback was the table would be moving a highlight nobody can see. `Invocation`
    already carries the sentence the action answered with (`selected auth`), so the
    surface the operator IS looking at says what just happened, and the next Enter says
    the next one.

    **And that is why this does not use `state.say`**, which is the frame's own surface
    for exactly this kind of sentence (#729) and is the right one everywhere else in this
    function. It is drawn by a panel, on the attention row, in a pane the zoom is covering
    — so a repeat that wrote a notice would be writing to a surface the operator cannot
    see, and would then have to wait for a palette that is deliberately not closing before
    they could. A notice per press would also outlive its own occasion: the dwell is
    seconds and the presses are faster than that, so the row would settle on whichever
    move happened to be last. The header is redrawn by the same keystroke that caused it.

    A refusal lands in the same place for the same reason, and the palette stays up: the
    row is still listed, still says why in its note, and an operator who pressed it has
    been told without losing the pane. That is `_say_on_screen`'s job for a palette that
    is CLOSING and this one's for a palette that is not.
    """
    try:
        act = reg.get(row.id)
    except frame_actions.ActionError:
        return None           # a picker row, or an id no provider on this machine has
    if not act.repeat:
        return None
    inv = reg.invoke(row.id, fid=fid, snapshot=snapshot)
    inv.join(timeout=_ACTION_START_GRACE)
    surface.report(inv.reason if not inv.started else (inv.note or inv.error))
    return surface


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
    `choose.pin_reason` and `frame/switch.py` build it from one read, so the row and the
    refusal cannot describe one frame two ways.

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

    The decision is `frame/switch.py`'s and the tmux half is this function's: which frame,
    where the answer is shown, and — for a workspace since §4b — the client that actually
    moves (:func:`_switch_client`).

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
        return outside_a_frame("charter frame-switch")
    ws = getattr(args, "workspace", None)
    persona_name = getattr(args, "persona", None)
    if ws:
        # **The one noun this command performs itself** (§4b). A persona switch IS
        # `switch.to_persona` — a file and a bump — so its outcome is complete the moment
        # that call returns. A workspace switch is a client moved between tmux sessions
        # and three re-layouts around it (:func:`_switch_client`), which is the same split
        # `cmd_chat` makes one noun down: the check says which names may be switched to,
        # and the command owns every reading only a live server can give.
        out = switch.to_workspace(fid, ws)
        if out.ok:
            _switch_client(fid, ws, said=out.message)
            return 0
    elif persona_name:
        out = switch.to_persona(fid, persona_name)
    else:
        return 0
    _say_on_screen(fid, out.message, ok=out.ok)
    return 0


def _say_on_screen(fid: str, message: str, *, ok: bool = False) -> None:
    """Put one line on the frame's own screen. Best effort, never raises.

    **On charter's own attention row, not on the tmux client's message line** (#729).
    This was `display-message -d 4000` and it moved for two separately measured reasons,
    either of which is on its own enough:

    * **It froze the screen for exactly as long as it spoke.** A tmux client does not
      redraw its PANES while a message is up. Measured with an outer terminal mirroring an
      inner session, on tmux 3.7c and at the 3.2 floor alike: the pane's content changed
      at 0.02s and the operator's screen did not catch up until 4.03s. The freeze tracks
      `-d` linearly (`-d 200` → 0.20s, `-d 750` → 0.74s), so those four seconds bought
      nothing and hid the repaint the message existed to announce — every workspace,
      persona and chat switch, on an operation `docs/frame.md` measures at a third of a
      second.
    * **It could not name the screen it drew on.** `-t` is `display-message`'s target for
      FORMAT evaluation, not its client; the client is `-c`, and with no `-c` tmux picks
      its own current client. Measured on both versions with two sessions on one server
      and a terminal attached to each: a message aimed at `-t <a pane of session sa>` was
      drawn on `sb`'s terminal and not on `sa`'s at all. `cmd_chat`'s own guard already
      recorded this leak by hand for an EMPTY target and read it as a property of the
      emptiness — it is not. A well-formed `%N` naming the right frame's own harness pane
      leaks identically, because the pane was never what chose the screen. On a control
      plane with eleven frames on one socket, that is a refusal about one frame drawn
      across another operator's.

    The row has neither problem. It is a pane charter already paints, so writing to it
    costs no client freeze; it is read by that frame's own panel out of that frame's own
    state directory, so it cannot be drawn on a frame it is not about; and every client
    attached to that frame sees it, which is strictly more than the one client `-c` could
    name and removes the "which of several clients pressed the key" question entirely.

    Because the row is a surface with an owner, this takes the FRAME the notice belongs
    to — which is not always the frame the outcome was computed for. A successful chat
    switch moves the operator to a sibling frame (`choose.py`: "the frame IS the chat"),
    so its caller passes the chat being switched TO; every other outcome leaves the
    operator where they are.

    *ok* picks the dwell and nothing else — `state.NOTICE_SECONDS` for an outcome that
    happened, `state.REFUSAL_SECONDS` for one that did not, and see those constants for
    why a refusal earns longer. There is deliberately no second visual treatment keyed off
    it: every refusal `switch.py` produces already reads as one ("cannot switch: …",
    "no workspace 'x' — have: …"), and the row is one line where a marker only spends
    columns the message itself needs.

    The `bump` is what makes the notice APPEAR promptly rather than at whatever the panel
    next happened to repaint for: a panel polls `state.version`, so writing the notice
    without moving the version would leave it invisible until something unrelated bumped
    the frame — the exact shape of #727, reached from the writing side instead of the
    expiring side. `state.say` writes before this returns, so the version a panel then
    reads always has the notice already behind it.
    """
    # One prefix for every outcome, as it always was. A second word saying "refused" only
    # ate columns off a row that truncates — measured against a 100-column client:
    # `charter: refused — cannot switch: …` ran off the end.
    state.say(fid, "charter: " + message,
              seconds=state.NOTICE_SECONDS if ok else state.REFUSAL_SECONDS)
    state.bump(fid)


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



#: How much of a chat's pane a quit captures, in lines from the bottom.
#:
#: **Bounded at tmux rather than in Python, and the number came off a measurement.** One
#: 200-column pane at charter's shipped `history_limit = 50000` took the shared tmux server
#: from 3.7 MB to **130 MB** (Phase 5's own §6.2 figure, re-measured for this change), and
#: `capture-pane -p -S -` pipes that whole history through this process. Asking tmux for the
#: last N lines instead means the bound is applied where the memory is, not after it has
#: been copied twice.
#:
#: Two thousand lines is far more than the last thing an operator was reading and far less
#: than a session's whole history. §4f is explicit that this preserves a **record** and is
#: never replayed, so what it has to be is enough to answer "what was it doing", not enough
#: to reconstruct the run.
_TRANSCRIPT_LINES = 2000

#: A second bound, in bytes, because a line has no length limit. A pane full of one very
#: long line is not a hypothetical — a `git log --oneline` of a big repo, a base64 blob, a
#: minified stack trace — and the line bound above says nothing about it. Applied to what
#: came back, keeping the END, because the end is what was on screen.
_TRANSCRIPT_BYTES = 512 * 1024

#: What tmux is asked for. `-e` keeps the escape sequences, so the captured text still has
#: its colours when a pager shows it; `-N` is what stops `-e` **trimming trailing spaces**,
#: which would silently rewrite the alignment of anything drawn in columns; `-J` is
#: deliberately absent, because joining wrapped lines would reflow a transcript captured at
#: one width into paragraphs at another.
_CAPTURE_FLAGS = ("-p", "-e", "-N")


def _capture_transcript(socket: str, pane_id: str, dest) -> bool:
    """Write *pane_id*'s scrollback to *dest*. ``True`` when something was written.

    **This reads the harness's own pane, which ADR 0018 forbids — and it is the SECOND
    exception, not the first.** `_pane_last_words` already runs `capture-pane -p -S -` on
    exactly this pane on both launch paths. The ADR's own closing words are that conflating
    rendering with observation *"is how a boundary like this erodes one convenient exception
    at a time"*, so this change amends the ADR rather than quietly adding to it: see
    `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md`, which now states the
    rule as *charter may READ that pane at two moments it is about to be destroyed, and
    never draws in it*.

    The write goes through `config.write_for`: the destination is under `config.STATE_DIR`
    and holds whatever the harness printed, which on a coding-agent's pane can include a
    file it was shown. 0600 is not a boundary (`SECURITY.md:43-46` is honest about that) and
    it is the same floor every other file charter writes there gets.

    ``False`` for every way this can fail — a server that would not answer, a pane that is
    already gone, a capture that came back empty, a write that could not land — because the
    caller does one thing with all of them: record no transcript for that chat, and say so
    in the manifest by leaving the field empty. A quit is never worth failing over a
    diagnostic, which is `_pane_last_words`' own `report=False` argument.
    """
    out = tmuxctl.run("capturing what this chat had on screen",
                      tmuxctl.server_argv(socket, "capture-pane", *_CAPTURE_FLAGS,
                                          "-S", f"-{_TRANSCRIPT_LINES}",
                                          "-t", pane_id),
                      timeout=10, report=False)
    if out.returncode != 0 or not out.stdout.strip():
        return False
    text = out.stdout
    try:
        # The frame root, which is `dest`'s parent: `config.write_for` opens a file and does
        # not make directories, and a transcript is the FIRST thing a plane might ever write
        # there that is not inside a chat's own directory. In production the root exists
        # because a chat's directory is in it; asked for anyway, because the alternative is
        # a capture that fails with ENOENT on the one path that has no other reader.
        config.private_mkdir(dest.parent)
    except OSError:
        return False
    # **Cut from the END, in BYTES, with no comparison to get the boundary wrong in.** The
    # end is what the operator was looking at, so that is the half that is kept.
    #
    # This was `if len(text.encode(...)) > _TRANSCRIPT_BYTES: text = text[-_TRANSCRIPT_BYTES:]`
    # and the deletion sweep found the comparison unpinnable — `>` and `>=` cannot differ,
    # because at exactly the cap `text[-N:]` returns the whole string either way. Chasing
    # that found the real defect underneath it: **it measured BYTES and then trimmed
    # CHARACTERS**, so a capture of two-byte characters was left at twice the cap. Measured
    # at a cap of 16: `"é" * 16` is 16 characters and 32 bytes, and the old line answered 32.
    #
    # Slicing the encoded form is byte-exact, and `errors="ignore"` on the way back is what
    # makes it safe: the only place a byte cut can land inside a character is the leading
    # edge, and a partial character there is dropped rather than written as a replacement
    # glyph the pager would show as noise. It is also cheaper than what it replaces — one
    # encode rather than an encode for the length plus a slice.
    text = text.encode("utf-8", "replace")[-_TRANSCRIPT_BYTES:].decode("utf-8", "ignore")
    try:
        config.write_for(dest, text)
    except OSError:
        return False
    return True


#: What every window on a server is asked, in one call, when a quit needs to know which
#: windows are this plane's chats.
#:
#: The chat comes from the OPTION and never from `#{window_name}`, for `_live_chats`' own
#: measured reason: with `allow-rename on` a pane's own output renamed a `-n`-named window
#: to `PWNED` on 3.7c and on 3.2 while the option was untouched. The window ID is what the
#: kill is aimed at — never a session name, which in another plane is another plane's
#: session (§3.3: `default` is a name every plane has), and never the chat's own recorded
#: pane id, which is a value from before the server may have restarted.
_CHAT_WINDOW_FORMAT = f"#{{{_CHAT_OPTION}}}\t#{{window_id}}\t#{{window_active}}"


def _chat_seats(socket: str) -> list[tuple[str, str, bool]] | None:
    """Every chat *socket* reports, as ``(chat id, window id, is its session's current)``.

    ``None`` when the server would not answer at all, which is `_live_chats`' tri-state and
    is carried for the same reason: a server that answers "no windows" because it was wedged
    is not a server with nothing on it, and a quit that read the two the same would record
    nothing and kill nothing while reporting that it had done both.

    **One listing for all three questions, because it is one listing.** An earlier version
    of this asked twice — once for the window map a kill is aimed at, once for which chat was
    on screen — on the grounds that the two answers have different lifetimes. They do not
    have different SOURCES, and `cmd_launch`'s own note about asking `tmux -V` twice applies
    unchanged: *two subprocesses for one unchanging fact*. The three fields arrive together
    or not at all.

    Rows charter cannot read are dropped rather than guessed at, and the chat id is held to
    :data:`_FRAME_ID_RE` and the window id to :data:`_WINDOW_ID_RE` on the way out — these
    values came off a tmux option and are about to be a `-t` target and a state directory's
    name, which is #475's boundary exactly.
    """
    out = tmuxctl.run("listing the chats this plane has open",
                      tmuxctl.server_argv(socket, "list-windows", "-a", "-F",
                                          _CHAT_WINDOW_FORMAT),
                      timeout=5, report=False)
    if out.returncode != 0:
        return None
    seats: list[tuple[str, str, bool]] = []
    for line in out.stdout.splitlines():
        fields = line.split("\t")
        if len(fields) != 3:
            continue
        chat, window, active = fields
        if _FRAME_ID_RE.fullmatch(chat) and _WINDOW_ID_RE.fullmatch(window):
            seats.append((chat, window, active == "1"))
    return seats


def _plane_live(servers) -> tuple[set[str] | None, dict[str, dict[str, str]], set[str]]:
    """Which of this plane's chats are live, where each one's window is, and which was shown.

    *servers* is every tmux server this plane's chats record. Asked once per server rather
    than once per chat, because that is one round trip against a list charter has to read
    whole anyway: a chat's liveness is not a question about the chat, it is a question about
    its server's window list.

    The set is ``None`` when **any** server refused to answer, and that is the conservative
    direction rather than a shortcut. `_live_chats` documents why a wedged server must not
    read as an empty one; with two servers in play, a quit that trusted the half that
    answered would record only the chats of one server and kill only those — and then tell
    the operator it had stopped the plane. ``None`` makes `leave.plan` mark every chat
    "charter could not ask", which the warning says out loud.

    The third value is which chats were their session's CURRENT window, so a reopen can put
    the operator back on the tab they were looking at rather than on whichever window it
    created first. An empty set costs that and nothing else: the fallback is the first chat
    of the focused workspace, which is one keypress away.
    """
    windows: dict[str, dict[str, str]] = {}
    known: set[str] = set()
    active: set[str] = set()
    unknown = False
    for server in servers:
        seats = _chat_seats(server)
        if seats is None:
            unknown = True
            continue
        windows[server] = {chat: window for chat, window, _ in seats}
        known |= set(windows[server])
        active |= {chat for chat, _, showing in seats if showing}
    return (None if unknown else known), windows, active


def _plane_servers() -> list[str]:
    """Every tmux server this plane's chats say they are on, charter's own first.

    Charter's own socket is always included, even on a plane whose every chat records
    another: it is where a chat with no recorded server will be (`builtin_actions._server`'s
    own fallback, spelled the same way), and asking it costs one `list-windows` against a
    socket that may not even have a server — which answers rc 1 and is read as "nothing
    live there" by the caller.
    """
    found = [SOCKET]
    for fid in leave.plane_chats():
        server = state.frame_server(fid) or SOCKET
        if server not in found:
            found.append(server)
    return found


def _warn_about(p, *, on: str, verb: str) -> None:
    """Put the per-chat warning where whoever asked for the quit can read it.

    **§4f's rule is that the warning names, per chat, what will and will not come back — at
    the moment the operator is deciding.** For the palette that moment is
    `leave.confirm_rows`, drawn before any keypress commits anything, and this is the
    same sentences said again on the way past for the OTHER caller: `charter frame-quit`
    typed by hand, which has no confirmation surface in front of it.

    Said again rather than only once, deliberately. §4i is explicit that quit *"warns and
    proceeds; it does not refuse"*, so the record of what was lost has to survive the
    keypress — and stderr is the only surface left once the frame's own panes are about to
    stop existing.

    *on* names the chat the notice row belongs to, or ``""`` when this was typed outside a
    frame. It is only used for the summary: the per-chat lines go to stderr either way,
    because the attention row is one line and this is one line per chat.
    """
    util.warn(leave.summary(p, verb=verb))
    for c in leave.stopping(p):
        util.warn(f"  {leave.title(c)} — {leave.note(c)}")
    if on:
        _say_on_screen(on, leave.summary(p, verb=verb))


def cmd_quit(args) -> int:
    """`F2 → charter: quit` — record this plane, then stop every harness on it.

    **The order is the design, and it is `trace`'s rule rather than a preference**
    (§4e/§4i): the manifest is written BEFORE anything is killed, because a record that
    depends on the thing it records succeeding is not a record. So:

    1. read the plane off disk — every chat directory, its workspace, persona, harness,
       cwd and durable session id (`leave.plan`);
    2. ask each server which of them is live, and which window each one is in;
    3. **capture each live chat's scrollback** while its pane still exists (§4f);
    4. **write the manifest** (`frame/reopen.py`) — a FILE in the frame root, which `reap`
       skips because it is not a directory, which is the only reason any of this survives a
       restart at all;
    5. drop the transcripts of chats the manifest no longer names;
    6. **kill**, one `kill-window` per chat, aimed at a window id tmux itself just reported;
    7. **prune `inflight`** — after the kills, not before, so a record is only discarded
       once the work behind it is genuinely stopped.

    **Plane-scoped, and that is §3.3's doing rather than a widening.** One tmux server
    serves every plane on the machine and session names carry no plane, so `kill-server`
    would take another plane's frames and `kill-session -t default` would take whichever
    plane's `default` tmux resolved first. The set of things this stops is exactly *the
    chats this plane has directories for*, which is the only filter that can be trusted.

    That is also what makes the `inflight` prune honest. Those records carry no fid, no chat
    and no workspace (§2.15), so a per-FRAME quit could not prune at all; a plane-scoped one
    prunes exactly what it killed. The two facts are one fact.

    **It warns and proceeds** (§4i). Refusing would leave an operator unable to quit while
    any agent was working, which on a control plane is most of the time. The one thing that
    stops it is being unable to RECORD: a quit that killed the plane after failing to write
    the manifest is the invasive quit this whole design exists to prevent, so that is a
    refusal with its own sentence.
    """
    fid = _pressers_chat(args)
    servers = _plane_servers()
    live, windows, active = _plane_live(servers)
    # The workspace of the chat the quit was PRESSED in, and `""` for a `charter frame-quit`
    # typed outside a frame — where a reopen falls back to the first frame it recorded.
    #
    # `or ""` and no `if fid` in front of it: the deletion sweep could not turn that
    # conditional red and it was right, because `own_workspace("")` already answers `None`
    # (`frame_dir` refuses the empty id) and the `or` converts it. A guard that passes only
    # because a DIFFERENT guard caught the case is the shape this repository deletes.
    focus = state.own_workspace(fid) or ""
    p = leave.plan(live=live, focus=focus)
    doomed = leave.stopping(p)
    if not doomed:
        util.warn(leave.NOTHING_OPEN)
        return 0
    _warn_about(p, on=fid, verb=leave.QUIT)
    kept = _record_the_plane(doomed, focus=focus, active=active, windows=windows)
    if kept is None:
        util.err("charter frame: refusing to quit — this plane could not be recorded, so "
                 "nothing here would come back. Nothing was stopped.")
        return 1
    stopped = _stop_chats(doomed, windows=windows)
    # AFTER the kills. `inflight` clears only on `finish()` (§2.15), so every record
    # belonging to a harness this quit just stopped is stranded — `still_running` reports
    # one for 30 minutes and `live` holds it for 24 hours, which would leave the frame's
    # spinner animating for a plane doing nothing and the dispatch-overlap nudge naming
    # agents this quit killed. Pruning BEFORE the kills would discard the record of work
    # that is still running if a kill then failed; pruning after is the only order in which
    # the tracker cannot end up quieter than the plane.
    pruned = inflight.prune_all()
    util.ok(f"charter: stopped {stopped} of {len(doomed)} chats and recorded "
            f"{kept} to reopen — bring them back with `charter reopen`"
            + (f" ({pruned} in-flight records cleared)" if pruned else ""))
    return 0


def _record_the_plane(doomed, *, focus: str, active, windows) -> int | None:
    """Capture and record *doomed*. The number of chats recorded, or ``None``.

    Split out of :func:`cmd_quit` so that "what a quit writes down" is one function a test
    can drive with no tmux underneath it — the capture is the only part that needs a server,
    and it is the part that is allowed to fail per chat.

    ``None`` for a manifest that did not land, which is the one thing that stops a quit.
    Everything else degrades per chat and is visible in what comes back: a capture that
    failed leaves the transcript field empty, so the reopen simply has nothing to offer.
    """
    frames: list = []
    entries: dict[str, list] = {}
    order: list[str] = []
    for c in doomed:
        if not c.workspace:
            # **Stopped, and honestly not recorded.** A reopen rebuilds one tmux session per
            # workspace and hands the launcher a `--workspace`; there is nothing to rebuild a
            # chat into that says nothing about its workspace, and choosing one for it would
            # be §4j's re-homing arriving as a convenience. `reopen.read` refuses such a
            # record anyway (`_usable` holds the workspace to `valid_name`), so writing one
            # would put a line in the manifest that every reader discards — a record that
            # looks like a promise and is not. The warning already told the operator
            # (`leave.NOT_REOPENED`), and `cmd_quit`'s own line says how many of how many
            # were kept.
            continue
        server = state.frame_server(c.chat) or SOCKET
        transcript = ""
        dest = reopen_state.transcript_path(c.chat)
        pane_id = state.harness_pane(c.chat) or ""
        if dest is not None and _PANE_ID_RE.fullmatch(pane_id) and c.chat in windows.get(
                server, {}):
            if _capture_transcript(server, pane_id, dest):
                transcript = dest.name
        if c.workspace not in entries:
            entries[c.workspace] = []
            order.append(c.workspace)
        entries[c.workspace].append(reopen_state.Chat(
            chat=c.chat, workspace=c.workspace, persona=c.persona, harness=c.harness,
            cwd=c.cwd, resume=c.resume, transcript=transcript,
            active=c.chat in active))
    for ws in order:
        frames.append(reopen_state.Frame(workspace=ws, chats=tuple(entries[ws])))
    if not reopen_state.write(frames, focus=focus):
        return None
    # Keyed on what was RECORDED and not on what was stopped: a transcript is a file in the
    # frame root, and the only collector it has is this call. Keeping one for a chat no
    # manifest names would leave it there for good.
    reopen_state.prune_transcripts({c.chat for f in frames for c in f.chats})
    return sum(len(f.chats) for f in frames)


def _stop_chats(doomed, *, windows) -> int:
    """`kill-window` each of *doomed*, on its own server. How many tmux accepted.

    **A WINDOW id, and never a session name or a recorded pane.** A session name in another
    plane is another plane's session (§3.3), and a pane id recorded before a server restart
    can name a pane that is now somebody else's — while a window id from the listing taken
    moments ago on that very server names this chat's window and nothing else. Killing a
    session's LAST window destroys the session, measured on 3.7c and at the 3.2 floor and
    already relied on by `cmd_launch`'s early-death path, so a workspace whose chats are all
    stopped ends exactly as it would have.

    A chat whose window is not in the listing is not counted and not aimed at: it is already
    stopped, and it stays in the manifest because it was open when the plane was last read.
    """
    stopped = 0
    for c in doomed:
        server = state.frame_server(c.chat) or SOCKET
        window = windows.get(server, {}).get(c.chat, "")
        if not window:
            continue
        out = tmuxctl.run(f"stopping chat {c.chat}",
                          tmuxctl.server_argv(server, "kill-window", "-t", window),
                          timeout=10, report=False)
        if out.returncode == 0:
            stopped += 1
    return stopped


def cmd_close(args) -> int:
    """`F2 → chat: close` — stop this one chat and do not bring it back.

    **Quit's teardown, one target, and one file more.** The plan, the warning and the kill
    are `cmd_quit`'s, asked with `leave.plan(only=…)` so that "what does stopping this cost"
    has one answer rather than one per verb. What close adds is `state.record_closed`, and
    that file is the whole difference between the two commands:

    * quit RECORDS the chat, so a reopen brings it back;
    * close MARKS it, so no reopen — this one's or a later quit's — ever does.

    The marker exists because a missing `exit` file means *"was open"* (§2.17: `kill-pane`,
    `kill-window` and `kill-session` write nothing, measured), and that default is
    deliberately towards restoring. Without a mark, closing a chat and then quitting would
    bring the closed chat back, because closing wrote no exit code either.

    **It also drops the chat's transcript**, which quit deliberately keeps: a transcript
    exists to be offered on the way back, and this chat is not coming back.

    **No refusal for a busy chat, and that is a limit rather than a decision.** §4i asks
    `workspace: close` to refuse while its harness is working; `inflight` cannot answer it —
    its records carry no fid, no chat and no workspace (§2.15) — so a refusal here would
    have to be based on a reading charter does not have. Saying so is the honest option: the
    confirmation row says the chat will not come back, and that is the sentence an operator
    needs before pressing it.
    """
    fid = _pressers_chat(args)
    target = (getattr(args, "chat_id", None) or "").strip() or fid
    if not target:
        return outside_a_frame("charter frame-close")
    if not chats.ID_RE.fullmatch(target):
        util.err(f"charter frame-close: '{contain.one_line(target)}' cannot name a chat")
        return 1
    servers = _plane_servers()
    live, windows, _active = _plane_live(servers)
    p = leave.plan(live=live, focus="", only=target)
    if not p.chats:
        util.err(f"charter frame-close: no open chat '{contain.one_line(target)}' on this "
                 "plane")
        return 1
    doomed = leave.stopping(p)
    if not doomed:
        # Its window is already gone, so there is nothing to stop — but the MARK is still
        # worth writing, and that is the whole reason this is not an early refusal: a chat
        # whose harness ended on its own is exactly the one a later quit would record as
        # "was open" and bring back uninvited.
        state.record_closed(target)
        _forget_transcript(target)
        util.ok(f"charter: chat {target} was already stopped — marked closed, so it will "
                "not come back")
        return 0
    _warn_about(p, on=fid if fid != target else "", verb=leave.CLOSE)
    # The mark FIRST, for `cmd_quit`'s ordering reason turned around: this is the record,
    # and a record written after the kill it describes is one a crash in between loses.
    # Losing it here is not merely untidy — it is the chat coming back after the operator
    # closed it.
    state.record_closed(target)
    _forget_transcript(target)
    stopped = _stop_chats(doomed, windows=windows)
    if not stopped:
        util.err(f"charter frame-close: tmux would not stop chat {target} — it is marked "
                 "closed and will not be reopened, but its harness may still be running")
        return 1
    util.ok(f"charter: closed {target} — it will not be reopened")
    return 0


def _forget_transcript(fid: str) -> None:
    """Drop *fid*'s captured scrollback, and drop it from any manifest naming it.

    **Both halves, because either one alone leaves the chat half-closed.** The file is what
    `chat: previous transcript` offers, and a manifest entry is what `charter reopen` acts
    on — so a close that removed only the file would still reopen the chat with nothing to
    offer, and one that removed only the entry would leave a transcript in the frame root
    with nothing left to collect it (`reopen.prune_transcripts` is that collector, and it
    keeps whatever the manifest names).

    Rewriting the manifest here is the one place a command other than a quit writes it, and
    it is a REMOVAL rather than a merge: what goes back is the manifest that was read with
    one chat's entry gone, so a manifest charter could not read is left exactly as it was
    rather than replaced by charter's reading of it.
    """
    m = reopen_state.read()
    if m is not None and any(c.chat == fid for c in m.all_chats()):
        frames = [reopen_state.Frame(workspace=f.workspace,
                                     chats=tuple(c for c in f.chats if c.chat != fid))
                  for f in m.frames]
        reopen_state.write([f for f in frames if f.chats], focus=m.focus, at=m.at)
    reopen_state.prune_transcripts(
        {c.chat for c in (reopen_state.read() or reopen_state.Manifest(
            at=0, focus="", frames=())).all_chats()})


def _restore_recorded_chat(rec, fid: str) -> None:
    """Move the two id-keyed things a recorded chat owns onto its new id *fid*.

    **The persona pointer.** `state.identity`'s `CHARTER_PERSONA` is the launch PIN, which
    is empty for every chat that was not launched with one — so a chat's actual persona
    lives in `persona.for_session(<chat id>)`, a file keyed on an id that is about to stop
    existing. `persona.set_active(..., terminal_id="")` is `switch.to_persona`'s own call:
    the session's pointer and nothing else, so a reopen does not repoint the terminal the
    operator happens to be typing `charter reopen` in.

    **The transcript.** `chat: previous transcript` looks the file up by the chat id it is
    offered on, so a capture named for the old chat would be invisible to the new one. It is
    RENAMED rather than pointed at, for `frame/reopen.TRANSCRIPT_SUFFIX`'s reason: one
    naming rule and no second file to keep in step. `os.replace` and not a copy, so the
    move is atomic and there is never a moment with two copies of one capture.

    A recycled ordinal is the case worth naming rather than discovering: `new_chat_id` walks
    upward from 1 and `reap` frees the ordinal a quit's chats held, so a reopen very often
    gets the SAME id back. Then the source and destination are one path and there is nothing
    to move — which is what the equality test says, and why it is an equality test rather
    than an `exists` check.

    Never raises. A persona that could not be pointed at leaves the chat on the plane's
    default, which is visible on its own panel; a transcript that could not be moved leaves
    the row with nothing to offer. Neither is worth failing a relaunch over, which is the
    promise every writer in `frame/state.py` makes for the same reason.
    """
    from . import persona as persona_mod
    if rec.persona and persona_mod.valid_name(rec.persona):
        try:
            persona_mod.set_active(rec.persona, session_id=fid, terminal_id="")
        except (OSError, ValueError):
            pass
    old = reopen_state.transcript_path(rec.chat)
    new = reopen_state.transcript_path(fid)
    if old is None or new is None or old == new:
        return
    try:
        os.replace(old, new)
    except OSError:
        return


#: What `charter reopen` says when there is nothing recorded. It names the thing that
#: writes one, because "nothing to reopen" on its own reads like a defect to an operator who
#: has just restarted their machine and lost a frame — the honest answer is that a plane is
#: recorded by QUITTING it, and a terminal that merely died detaches (§4i).
NOTHING_RECORDED = (
    "charter reopen: nothing recorded to put back. A plane is recorded when you quit it "
    "(`F2 → charter: quit`); a terminal that closed on its own only detached, so its "
    "harnesses are still running — `tmux -L charter attach` reaches them.")


def cmd_reopen(args) -> int:
    """`charter reopen` — put back the plane the last quit recorded.

    **What comes back, per chat: the workspace, the persona, the harness and the
    directory** — §4e's own four — plus, for Claude Code alone, the conversation, by
    appending ``--resume <id>`` to the harness's own argv. **What does not: the selection,
    the pane map, and the live scrollback.** §2.5's reasons for destroying the first two are
    still right, `panes` names tmux ids that died with the server, and §4f is explicit that a
    captured transcript is *offered and never replayed* — the reopened pane starts clean and
    `chat: previous transcript` is where the old one went.

    **A chat that cannot be resumed still comes back, empty, and says so.** Silently not
    reopening it would make a chat vanish across a restart, which is the opposite of what
    "less invasive" asked for. The directory, the workspace and the persona are restored
    either way; only the conversation is gone, and the line this prints for that chat names
    which of the reasons it was (§4f's own four sentences, in `frame/leave.py`).

    **Each chat is launched through `cmd_launch` and none of them attaches.** One launcher
    rather than a second path, because everything a chat needs — the session, the window,
    the hooks, the panels, the options, the identity `-e` overlay — is that function's, and
    a reopen with its own copy of it would be a second answer to "what a chat is" that
    drifts on the first change to either. `Reopening` is the whole of the difference:
    it suppresses open-or-focus, the client move and the attach, and carries the new chat id
    back out.

    **The attach happens once, at the end, to the workspace the quit was invoked from.** A
    reopen that attached per chat would block on the first one; one that attached to
    whichever session tmux happened to make current would put the operator somewhere they
    did not leave.

    **The manifest is consumed, chat by chat.** A record describes one quit, and a second
    `charter reopen` against the same file would open every chat a second time with nothing
    on screen to tell the duplicates apart — but deleting it whole would throw away the
    record of the chats that did NOT come back, which is exactly the state an operator would
    want to retry. So what is left behind is precisely the retry (:func:`_consume`), and it
    is written after the launches and before the attach, because the attach does not return
    until the operator leaves.
    """
    m = reopen_state.read()
    if m is None or not m.frames:
        util.err(NOTHING_RECORDED)
        return 1
    # **Before anything is started**, and it is the #687/#690 shape rather than politeness:
    # `cmd_launch` answers a non-tty stdout with `bypass(argv)`, which is an `os.execvp`.
    # A `charter reopen` in a pipeline would therefore not report anything at all — it
    # would BECOME the first recorded harness, in this process, with the rest of the plane
    # unrestored and the manifest still on disk.
    if not sys.stdout.isatty():
        util.err("charter reopen: needs a terminal — it starts a tmux frame and attaches "
                 "to it. Nothing was reopened.")
        return 1
    if tmuxctl.version() is None:
        util.err(tmuxctl.absent_message())
        return 1
    # **Refused inside a tmux the operator already has, and stated rather than half-done.**
    #
    # **The test is `operator_server`, deliberately, and NOT #812's `is_operator_socket`.**
    # This is not a question about whose server it is; it is a question about whether this
    # process is already inside ONE, and both answers refuse. On somebody else's server
    # the reason is the paragraph below. On charter's own — reached when a `charter
    # reopen` is typed at a shell inside a frame — every chat would build correctly and
    # the single `_attach_after_reopen` at the end would then attach a NESTED client
    # inside the pane it was typed in, which is the pre-ADR-0018 shape ADR 0018 removed.
    # Refusing costs one message; the operator's own shell is one `detach` away.
    #
    # `cmd_launch` builds a frame in somebody else's tmux as a WINDOW on THEIR server
    # (`_launch_in_operator_tmux`), and that path is awake for the whole life of the frame —
    # it reads the harness's exit status itself instead of installing the `pane-died` hooks,
    # which is what makes it correct there and what makes it BLOCK exactly as `attach` does.
    # A reopen driving it would stop on the first chat and never build the second, and the
    # `Reopening` seam is not wired through it either: `restoring.fid` would stay ``""`` and
    # every chat would be reported as "did not come back" having actually come back. Two
    # honest options existed and this is the smaller one — the alternative is suppressing
    # `_wait_for_harness` on a path whose exit-code contract depends on it, which is its own
    # change with its own two-version verification.
    if tmuxctl.operator_server() is not None:
        util.err("charter reopen: not from inside a tmux you already have — charter builds "
                 "a frame there as a window on your own server, and that launcher stays "
                 "awake for the life of each frame, so reopening several chats would stop "
                 "at the first. Run it from an ordinary shell. Nothing was reopened, and "
                 "the record is left in place.")
        return 1
    back: list[Reopening] = []
    for f in m.frames:
        for c in f.chats:
            r = _reopen_one(c)
            if r is not None:
                back.append(r)
    if not back:
        util.err("charter reopen: none of the recorded chats could be started — the "
                 "record is left in place so this can be tried again.")
        return 1
    util.ok(f"charter: reopened {len(back)} of {len(m.all_chats())} chats")
    _consume(m, {r.chat.chat for r in back})
    return _attach_after_reopen(m, back)


def _consume(m, done) -> None:
    """Take the chats in *done* out of the manifest, and drop it when nothing is left.

    **Consumed rather than deleted, because a partial reopen has two halves and only one of
    them is over.** A record describes one quit: leaving it whole would open every chat a
    second time on the next `charter reopen`, with nothing on screen to tell the duplicates
    apart — but deleting it whole would throw away the record of the chats that did *not*
    come back, which is exactly the state an operator would want to retry. So what is left
    behind is precisely the retry: the chats still owed, in their own frames, with the same
    focus.

    Called after the launches and before the attach, because the attach does not return
    until the operator leaves.
    """
    left = [reopen_state.Frame(workspace=f.workspace,
                               chats=tuple(c for c in f.chats if c.chat not in done))
            for f in m.frames]
    left = [f for f in left if f.chats]
    if not left:
        reopen_state.forget()
        return
    if reopen_state.write(left, focus=m.focus, at=m.at):
        util.warn(f"charter reopen: {sum(len(f.chats) for f in left)} chat(s) still "
                  "recorded — `charter reopen` again to retry just those")


def _reopen_one(c) -> "Reopening | None":
    """Put one recorded chat back. The launch's own record, or ``None`` when it did not run.

    **The harness is resolved off the registry by its own `name`**, which is what
    `state.identity` recorded (`harness.base.name` — ``claude-code``, not ``claude``), and
    a chat whose harness this charter no longer registers falls back to the plane's
    ``[harness] default``. That fallback is reported rather than silent: the operator asked
    for their plane back and is getting one chat of it under a different runtime, which is a
    thing to be told.

    **`--resume` is appended to the harness's own argv and nowhere else.** `Harness.launch_argv`
    is ``[self.binary, *extra]`` with no override in the registry, so the pass-through IS
    the seam — there is no resume member on `Harness` and Phase 5's Task 9 Step 4 refuses one
    on `harness/base.py`'s own bar. It is gated on `leave.resumable_harness`, so a recorded
    id belonging to a harness that does not take the flag is not handed to it.

    **The directory is `os.chdir`'d into rather than passed**, because that is where
    `cmd_launch` reads it from — one `os.getcwd()` on each of its two paths — and adding a
    parameter for it would be a second way to say the same thing. Restored in a `finally`:
    the next chat in the manifest has its own directory, and a launcher left standing in
    somebody else's is exactly the silent wrongness §4e's cwd item exists to close.
    """
    h = next((x for x in harness.all() if x.name == c.harness), None)
    if h is None or not h.cli_name:
        fallback = (config.HARNESS or {}).get("default")
        h = next((x for x in harness.all() if x.cli_name == fallback), None)
        if h is None:
            util.warn(f"charter reopen: {c.chat} recorded harness "
                      f"{contain.readable(c.harness) or 'nothing'}, which this charter "
                      f"cannot launch, and this plane declares no `[harness] default` — "
                      f"not reopened")
            return None
        util.warn(f"charter reopen: {c.chat} recorded harness "
                  f"{contain.readable(c.harness) or 'nothing'} — reopening it under "
                  f"{h.cli_name}, this plane's default")
    rest: list[str] = []
    if c.resume and leave.resumable_harness(c.harness):
        rest = ["--resume", c.resume]
    where = c.cwd if c.cwd and os.path.isdir(c.cwd) else str(config.ROOT)
    if where != c.cwd:
        util.warn(f"charter reopen: {c.chat}'s directory "
                  f"{contain.readable(c.cwd) or 'was never recorded'} — reopening in "
                  f"{where}")
    r = Reopening(c)
    argv_args = _reopen_args(c, harness_name=h.cli_name, rest=rest, reopening=r)
    here = os.getcwd()
    try:
        os.chdir(where)
    except OSError:
        util.warn(f"charter reopen: cannot enter {contain.readable(where)} — {c.chat} not "
                  "reopened")
        return None
    try:
        rc = cmd_launch(argv_args)
    finally:
        try:
            os.chdir(here)
        except OSError:
            pass
    if rc != 0 or not r.fid:
        util.warn(f"charter reopen: {c.chat} did not come back (launcher returned {rc})")
        return None
    util.info(f"  {c.chat} → {r.fid} · {leave.RESUMES if rest else 'empty'}"
              + (" · workspace is missing" if c.workspace
                 and not workspace.workspace_dir(c.workspace).is_dir() else ""))
    return r


def _reopen_args(c, *, harness_name: str, rest, reopening):
    """The namespace `cmd_launch` is driven with, built once so its fields are visible.

    Every field `cmd_launch` and `_choose_workspace` read is named here rather than left to
    a `getattr` default, which is what makes this a readable contract instead of a puzzle:
    ``workspace`` is the recorded one — set explicitly, so `_picker_wanted` never asks a
    question on a path with no operator waiting — ``pick`` is false for the same reason,
    ``no_frame`` is false because a reopen without a frame is a bare harness, and
    ``reopening`` is the seam.
    """
    from types import SimpleNamespace
    return SimpleNamespace(harness=harness_name, rest=list(rest), no_frame=False,
                           workspace=c.workspace or None, pick=False,
                           reopening=reopening)


def _attach_after_reopen(m, back) -> int:
    """Put the operator on the chat they left, then hand them the terminal.

    The chat is the one the manifest marked `active` in the FOCUS workspace — the session
    that had the client when the quit ran. Failing that, the first chat of that workspace;
    failing that, the first chat reopened at all. Each fallback is one step further from
    "where you were" and none of them is nowhere, which matters because the alternative is
    attaching to whichever window tmux made current — an answer nobody chose.

    `select-window` is aimed at the new chat's own harness PANE, because a pane id resolves
    to its window on 3.7c and at the 3.2 floor (measured, and already relied on by
    `cmd_chat`), and because a session NAME as a `-t` is parsed by tmux as ``window.pane``
    for any workspace with a dot in it.

    The attach is `tmuxctl.interact` for `cmd_launch`'s own reason: this IS the operator's
    terminal now, not an admin command whose output charter should own.
    """
    wanted = next((r for r in back
                   if r.chat.active and r.chat.workspace == m.focus), None)
    if wanted is None:
        wanted = next((r for r in back if r.chat.workspace == m.focus), None)
    if wanted is None:
        wanted = next((r for r in back if r.chat.active), back[0])
    session = state.workspace_prefix(wanted.chat.workspace)
    pane_id = state.harness_pane(wanted.fid) or ""
    if _PANE_ID_RE.fullmatch(pane_id):
        tmuxctl.run("putting you back on the chat you left",
                    tmuxctl.server_argv(SOCKET, "select-window", "-t", pane_id),
                    report=False)
    attach_cmd = tmuxctl.server_argv(SOCKET, "attach", "-t", session)
    attached = tmuxctl.interact(attach_cmd)
    if attached.returncode != 0:
        tmuxctl.report_failure("attaching to the reopened frame", attach_cmd, attached)
        return attached.returncode
    return 0


#: The pager a captured transcript is shown in, and the ONE thing charter will run for it.
#:
#: **`$PAGER` is deliberately not honoured**, and that is a containment decision rather than
#: a limitation. A `$PAGER` is conventionally a COMMAND LINE (`less -R`, `bat --paging
#: always`), so honouring it means either splitting a string the operator's environment
#: supplied — which is shell parsing charter would be reimplementing — or handing it to a
#: shell, which is the injection `harness.base.launch_argv` returns a list to prevent. What
#: charter runs here is one argv it wrote itself.
#:
#: `-R` because the capture keeps its escape sequences (`_CAPTURE_FLAGS`' `-e`), so without
#: it the transcript reads as a wall of `ESC[0m`. `-+X` and no other flags: charter does not
#: get to decide how somebody's `less` behaves beyond making the colours work.
_PAGER = ("less", "-R")

#: What a chat with no captured transcript is told, instead of a row that does nothing.
#: It names WHY there is none rather than only that there is none: a chat that has never
#: been quit has no capture, and that is the ordinary state of every chat that is running
#: normally.
NO_TRANSCRIPT = ("no previous transcript for this chat — one is captured when a plane is "
                 "quit, and offered on the chat that comes back")

#: What an operator is told when charter has the transcript and no pager to show it in.
#: The path is given, because a file the operator can open themselves is a strictly better
#: answer than a refusal, and because leaving a dead `cat` pane in their frame would be
#: worse than either (§2.14: a dead pane with `remain-on-exit` is a window that does not
#: close, in a frame whose prefix key charter hides).
NO_PAGER = "charter cannot find `less` to show it in — the captured text is at {path}"


def cmd_transcript(args) -> int:
    """`F2 → chat: previous transcript` — open what this chat had on screen before.

    **Offered, never replayed** (§4f). The text goes into a NEW tmux window running a pager;
    nothing is written into the harness's own pane. Replaying bytes into the live pane would
    present a session that is not running as though it were — the convincing-empty this
    project refuses everywhere else — and it would put a previous run's output above a new
    run's prompt with nothing marking the seam.

    **This is the READING half of the ADR 0018 amendment this change carries.** The capture
    is the other half (`_capture_transcript`); between them, charter now reads the harness's
    pane at two moments and still draws in it at none. See the ADR.

    **Always 0**, like every other `frame-*` command: this runs detached with its streams on
    `/dev/null` (`builtin_actions._spawn`), so a non-zero exit is read by nothing — and
    inside a `run-shell` a non-zero exit is what makes tmux print into the harness pane, the
    one rectangle ADR 0018 says charter never draws in. Every refusal goes to
    :func:`_say_on_screen` instead.
    """
    fid = _pressers_chat(args)
    if not fid:
        return outside_a_frame("charter frame-transcript")
    path = reopen_state.transcript_path(fid)
    if path is None or not path.is_file():
        _say_on_screen(fid, NO_TRANSCRIPT)
        return 0
    if shutil.which(_PAGER[0]) is None:
        _say_on_screen(fid, NO_PAGER.format(path=path))
        return 0
    pane_id = chats.pane_of(fid)
    if pane_id is None:
        _say_on_screen(fid, "charter has no usable record of this chat's harness pane, so "
                            "it cannot place a window beside it — relaunch this chat")
        return 0
    socket = state.frame_server(fid) or SOCKET
    # **The target is the SESSION ID, and every other spelling was measured failing.** This
    # is the #664/#695 shape and it cost a hand-run to find: `kill-window -t %N` resolves a
    # pane to its own window, so `new-window -t %N` looks like it should too. It does not.
    # On tmux 3.7c **and** at the 3.2 floor, with a session called `alpha.2` and its own
    # `$0`/`@0`/`%0`:
    #
    #     new-window -t %0        rc 1   can't specify pane here
    #     new-window -t @0        rc 1   create window failed: index 0 in use
    #     new-window -t alpha.2   rc 1   can't specify pane here      <- #695, again
    #     new-window -t $0        rc 0
    #
    # `-t` here is a target-WINDOW and a window id is read as the index to insert at, which
    # is by definition taken; a session NAME with a dot in it is parsed as `window.pane`,
    # which is the collision `state._UNSAFE` already exists for. The session id is the one
    # unambiguous spelling, and `_pane_place` is what turns this chat's recorded pane into
    # one — held to `_SESSION_ID_RE` on the way out, at #475's boundary.
    place = _pane_place(socket, pane_id)
    if place is None:
        _say_on_screen(fid, f"tmux would not say which session this chat is in, so charter "
                            f"cannot place a window there — the captured text is at {path}")
        return 0
    session_id, _window_id = place
    # `--` and an argv, never a joined string: tmux shell-interprets a single argument and
    # does not interpret separate ones (`harness.base.launch_argv`'s own measured rule), and
    # the path here is charter's own file under a state directory whose name carries the
    # plane's.
    opened = tmuxctl.run(
        "opening this chat's previous transcript",
        tmuxctl.server_argv(socket, "new-window", "-t", session_id, "-n",
                            f"transcript {fid}", "--", *_PAGER, str(path)))
    if opened.returncode != 0:
        _say_on_screen(fid, f"tmux would not open a window for the transcript — it is at "
                            f"{path}")
    return 0


def _start_leaving(fid: str, verb: str) -> None:
    """Start the quit or the close, detached, and return having started it.

    :func:`_start_chat_switch`'s argument, one verb over, and every word of it applies:
    the palette closes the instant it has invoked, `kill-pane` hands SIGHUP to that pane's
    process group, and a teardown that ran in this process would be racing its own — with
    a sharper edge here, because one of the windows it kills is very likely the one this
    palette is drawn over.

    `builtin_actions._spawn` and never a bare `Popen`, so `$CHARTER_SESSION_ID` is STATED
    rather than inherited: this is a `run-shell` child of a tmux server shared between
    every frame on the machine, and its own variable may be another chat's
    (`state.record_identity` measures exactly that). `--chat` carries the presser's own
    chat, which is the only value that can tell two chats of one workspace apart — the
    same split `frame-toggle` already makes.
    """
    builtin_actions._spawn(
        util.self_relaunch_argv(f"frame-{verb}", "--chat", fid), fid=fid)


def _say_it_was_quit(fid: str) -> None:
    """Tell the operator their plane was recorded, on the shell they are back in.

    **The gap this closes was found by asking where the quit's own sentence goes, and the
    answer was nowhere.** `charter: quit` is started detached with `stdout` and `stderr` on
    `/dev/null` — that is `builtin_actions._spawn`'s measured requirement, because the
    palette's pane is killed the instant a row is invoked and a write to a closed pty is an
    `EIO` in a process that was working correctly. And it cannot use the frame's attention
    row either (`_say_on_screen`), because the frame it would draw on is the one it just
    stopped. So the operator pressed a row, their screen went back to a shell, and nothing
    anywhere named `charter reopen`.

    `cmd_launch` is the process that hands them that shell, so it is the one that can say
    it. Gated on the MANIFEST naming this chat, which is what makes the sentence true rather
    than likely: a launcher whose harness merely exited, or whose operator detached, reaches
    the same lines and says nothing.

    Never raises and never guesses: a manifest charter cannot read is one it says nothing
    about, exactly as `reopen.read` answers `None` for it.
    """
    m = reopen_state.read()
    if m is None or not any(c.chat == fid for c in m.all_chats()):
        return
    n = len(m.all_chats())
    back = len([c for c in m.all_chats() if c.resume])
    util.info(f"charter: this plane was quit — {n} chat(s) recorded, {back} with a "
              f"conversation to resume.\n  put it back with: charter reopen")
