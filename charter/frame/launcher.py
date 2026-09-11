"""The one place a harness profile starts: charter, in the pane, becoming the harness.

Every chat pane's first process is `charter frame-launch --profile <name> -- <rest>`
(`layout.session_argv` and `layout.chat_window_argv` on charter's own server,
`layout.respawn_argv` after the `cat` placeholder in the operator's own tmux). It runs the
guards again in the pane and then `os.execvpe`s the profile's command with the profile's
`env` applied. Three reasons, the first a constraint (spec, *How a harness starts*):

* **`env` reaches the harness without `layout.CARRIABLE`.** That allow-list raises on every
  other name and the chat-handoff plan forbids adding to it, so a profile's environment can
  only arrive on the far side of tmux. It never passes through tmux's own argument parser
  either, which has already cost this repo #957 and #961. Only the profile's NAME travels.
* **One place runs the checks for every open** — the CLI, `+`, a tab, reopen, a handoff.
* **`exec` keeps the launcher's pid for the harness**, so the pane id charter records,
  `remain-on-exit` and the `pane-died` path see exactly what they saw before. Measured on
  tmux 3.7c and at the 3.2 floor, on both servers, before any of this was built:
  `workspaces/harness-profiles/refs/task2-measure/` in the plane.

**A launcher is in a frame only when tmux says so** (rulings 29 and 33). Before it execs,
and before it claims or prints anything, its own pid must be the `#{pane_pid}` of a LIVE
pane belonging to the chat it claims, read from tmux on that chat's server:

* on charter's own server, the window's NAME — charter's config is loaded there and nothing
  charter runs emits a title escape before the exec;
* in the operator's tmux, the `@charter_chat` window option, set before `respawn-pane` —
  there a hook, a plugin or `allow-rename on` output can rename a window, so the name is
  only a label (`commands_frame._CHAT_OPTION` measures it).

`$TMUX_PANE` and `$CHARTER_SESSION_ID` are never proof: a model's own tool shell inherits
both from its chat (measured — shell pid 4707 under `pane_pid` 53118), so a
`charter frame-launch` run from one is a launch with no frame that writes no chat's record,
which reopen follows. It is a guard rail against a model's accidental misuse and not a
boundary: a process that deliberately starts its own tmux pane in a window named like a
chat passes it.

**A refusal in the pane has to reach the operator without the dead pane** (ruling 42).
Measured 2026-09-11: `_launch`'s eager dead-status ask completes 6-14 ms after the start
while a Python launcher's first line runs at 19-22 ms, and on charter's server the teardown
hook then kills the window (`_pane_last_words` answered `[]` in all 40 runs). So an
**attended** launcher shows its refusal in the pane and waits for a key — the pane is
charter's while no harness has run in it (ADR 0018, as the spec amends it) — and an
**unattended** one records it under the chat, where the launch that opened the chat reads
it back (`state.record_launch`, `commands_frame._await_the_launcher`).

**Nothing here is on the import path.** `cli` and `commands_frame` reach this module at call
time, because every `charter hook …` process builds the parser and derives config, and
ruling 43 took profile code off that path.
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path
from typing import Callable, Mapping

from typing import NamedTuple

from .. import config, contain, profiles, util
from . import state, tmuxctl

#: What a launcher that refused in the pane exits with, after printing why.
REFUSED_EXIT = 3

#: The command is not on `PATH` — the shell's own number, and `bypass`'s (`commands_frame`).
MISSING_EXIT = 127

#: And its sibling: a command that exists and cannot be executed.
NOT_EXECUTABLE_EXIT = 126

#: Every kind of refusal this module produces. Callers branch on the KIND and never on the
#: text (ruling 27): a sentence gets reworded, and a comparison against one is a guard that
#: stops guarding on the day it does.
KIND_IGNORED = "ignored"
KIND_PATH = "path"
KIND_NOT_YET = "not-yet"
KIND_EXEC = "exec"


class Refusal(NamedTuple):
    """Why a profile did not start, and what a CLI returns for it."""

    kind: str      # one of the KIND_* constants above
    text: str      # the sentence, without charter's own prefix — a caller says it its way
    exit: int      # MISSING_EXIT for KIND_PATH, its own number for KIND_EXEC, else REFUSED_EXIT


# Every sentence says the rule worked and names the fix in the same breath (CONTEXT.md,
# *A refusal is the rule working*), and every profile-derived value in one is contained
# (ruling 35): `charter.local.toml` is a file a chat can write, and a `\r` or an ESC in a
# command could otherwise redraw the line to show a harmless command while another one runs.
UNKNOWN_PROFILE = ("no profile named '{name}' — have: {names}. Nothing was started. "
                   "charter harness list shows every profile and why any was refused.")
KIND_MISMATCH = ("profile '{name}' is a {kind} profile, not a {asked} one — nothing was "
                 "started. Run it by its own name: charter {name}")
IGNORED = "profile '{name}' is refused — {why}"
NOT_ON_PATH = ("profile '{name}' runs {cmd}, which is not on PATH — nothing was started. "
               "Install it, or give the profile a command with a full path in "
               "charter.local.toml.")
DECLARED_NOT_YET = (
    "profile '{name}' comes from charter.local.toml, and this charter cannot yet ask before "
    "a declared command runs, so it runs none — nothing was started. Until it can, launch a "
    "built-in the file does not replace: {builtins}.")
EXEC_FAILED = ("profile '{name}' could not be started — {cmd} failed to run ({why}). Nothing "
               "is running here; fix the command in charter.local.toml.")
UNPROVEN_CHAT = ("this launch was given chat '{chat}' but is not that chat's pane, so it "
                 "runs with no frame and records nothing for '{chat}'.")

#: What an attended pane says under its refusal. The pane is charter's while no harness has
#: run in it, and the keypress is what keeps the window from closing over the sentence
#: (ruling 42).
PRESS_ENTER = "  press Enter to close this chat."


def _nothing() -> None:
    """The undo an :func:`attempt` with nothing to undo runs."""


def argv(profile: str, rest: list[str], *, attended: bool) -> list[str]:
    """What tmux is handed where the harness's own argv went.

    Unattended unless told (review 3): a pane nobody is at must never wait on a question,
    so `--attended` is what an open with somebody in front of it adds rather than what a
    reopen or a handoff has to remember to take away.

    `util.self_relaunch_argv` for its own reason (#390): `python -m charter` prepends the
    child's cwd to `sys.path`, and a chat's cwd is a workspace clone that may hold its own
    `charter/` package.
    """
    return util.self_relaunch_argv("frame-launch", "--profile", profile,
                                   *(("--attended",) if attended else ()), "--", *rest)


def environment(p: profiles.Profile, base: Mapping[str, str], *,
                framed: bool) -> dict[str, str]:
    """The environment *p*'s command is exec'd with.

    **`CHARTER_HARNESS` stays the KIND's registry name**, because hooks compare it to
    `claude-code` for session ids, resume and the working spinner. The profile rides beside
    it as `CHARTER_HARNESS_PROFILE`, set here at the exec rather than through tmux — it
    never joins `commands_frame._FRAME_IDENTITY`, which would put it on a tmux `-e`.

    **An unframed launch drops `CHARTER_SESSION_ID` and only that** (review 8). The defect
    is the pair: `hooks._record_harness_session` acts on a session id beside a launched
    `CHARTER_HARNESS`, so `charter claude --no-frame` typed inside a chat would otherwise
    write its own harness session into that chat's state. `CHARTER_ROOT`,
    `CHARTER_WORKSPACE` and `CHARTER_PERSONA` stay: a bare harness started from a chat's
    shell staying in that chat's plane and workspace is what its operator wants, and
    `CHARTER_PERSONA=forge charter claude --no-frame` means what it says.
    """
    env = dict(base)
    if not framed:
        env.pop("CHARTER_SESSION_ID", None)
    env.update(profiles.expanded_env(p))
    env["CHARTER_HARNESS"] = p.harness
    env["CHARTER_HARNESS_PROFILE"] = p.name
    return env


def resolve(name: str) -> tuple[profiles.Profile | None, str]:
    """*name*'s profile, or ``(None, why it was refused)`` — ``(None, "")`` for a name this
    plane has never heard of.

    Two answers rather than one sentence, because the callers say different things about a
    name nothing declares: `_launch` names the profiles there are, a reopen says the chat is
    not coming back, and a `+` says the chat's own profile is gone. A REFUSED profile has
    one answer for all of them — its own reason — and never falls back to the built-in of
    that name (ruling 37): the operator said how that name runs, and running the built-in in
    its place runs the command they replaced.
    """
    read = profiles.current()
    found = read.profiles.get(name)
    if found is not None:
        return found, ""
    shown = contain.readable(name)
    return None, next((r.reason for r in read.refused if r.name == shown), "")


def unknown_profile(name: str) -> str:
    """What a launch says for a name this plane declares nothing for."""
    return UNKNOWN_PROFILE.format(name=contain.readable(name),
                                  names=", ".join(profiles.current().profiles))


def display_command(p: profiles.Profile, rest: list[str]) -> list[str]:
    """What charter SHOWS for a launch: *p*'s own command, and what the open added to it.

    Never :func:`argv`, which is `python -P -m charter frame-launch --profile …` — true of
    every chat and an answer to a question nobody asked. An early death names the command
    the operator wrote down, and so does a tmux that refused it.

    Contained word by word (ruling 35): the command comes out of a file a chat can write,
    and this text goes to a terminal.
    """
    return [contain.readable(word) for word in profiles.expanded_command(p)] + list(rest)


def refusal(p: profiles.Profile, *, root: Path, attended: bool,
            env: Mapping[str, str]) -> Refusal | None:
    """Why *p* may not start, or ``None``.

    The spec's order, and it is the order the checks cost in: the file is ignored, the
    command is on `PATH`, the profile is approved. Asked before tmux wherever a terminal is
    attached — where a refusal is a `return` with nothing to tear down — and again in the
    pane immediately before the exec, because the plane can move in between: that is the
    whole reason there are two calls and not one.

    **The ignore check is asked of DECLARED profiles only.** A built-in comes out of
    charter's own registry rather than out of the file, so what git would do with that file
    decides nothing about it. A declared replacement of a built-in (`[harness.claude]`) is
    declared, so `charter claude` is refused with the rest and never falls back (ruling 19).

    *attended* decides nothing here yet: Task 3's approval is what asks, and this is the seam
    it fills (`_approval_refusal`).
    """
    if p.source != profiles.BUILTIN:
        why = profiles.ignored_refusal(root)
        if why:
            return Refusal(KIND_IGNORED,
                           IGNORED.format(name=contain.readable(p.name), why=why),
                           REFUSED_EXIT)
    program = profiles.expanded_command(p)[0]
    # The `PATH` the exec will use, not this process's: a profile that needs another one
    # sets it in `env`, and in a pane the base is the tmux client's (measured — tmux
    # overwrites a `-e PATH`), which is charter's own.
    if shutil.which(program, path=env.get("PATH")) is None:
        return Refusal(KIND_PATH,
                       NOT_ON_PATH.format(name=contain.readable(p.name),
                                          cmd=contain.readable(program)),
                       MISSING_EXIT)
    return _approval_refusal(p, attended=attended)


def _approval_refusal(p: profiles.Profile, *, attended: bool) -> Refusal | None:
    """Task 2's B1 refusal: a declared profile does not run until something can ask first.

    **`main` must never run an unapproved declared command between two merges.** Nothing
    yet stands for the operator's approval of a `command` — Task 3's launch record is what
    will — and `charter.local.toml` is a file a chat can write with no diff to show for it.
    So every profile the file declares is refused here, a replacement of a built-in
    included, through `charter <profile>`, `+`, a tab, reopen and a handoff alike; only the
    built-ins no declared profile replaces launch.

    Task 3 replaces this body with `profiletrust.refusal(p, attended=attended)` and deletes
    :data:`DECLARED_NOT_YET`, which is why *attended* is already the parameter it takes: an
    open nobody is at refuses where it would have asked.
    """
    if p.source == profiles.BUILTIN:
        return None
    builtins = ", ".join(name for name, q in profiles.current().profiles.items()
                         if q.source == profiles.BUILTIN)
    return Refusal(KIND_NOT_YET,
                   DECLARED_NOT_YET.format(name=contain.readable(p.name), builtins=builtins),
                   REFUSED_EXIT)


def _exec_exit(e: OSError) -> int:
    """What an `execvpe` that raised returns — `bypass`'s own pair, so `charter <profile>
    && …` behaves the way `<command> && …` would have."""
    if isinstance(e, FileNotFoundError):
        return MISSING_EXIT
    if isinstance(e, PermissionError):
        return NOT_EXECUTABLE_EXIT
    return REFUSED_EXIT


def attempt(p: profiles.Profile, rest: list[str], *, fid: str | None, attended: bool,
            on_exec: Callable[[], Callable[[], None]] = lambda: _nothing) -> Refusal | None:
    """Run the chain and hand this process to *p*'s command. A :class:`Refusal`, or ``None``.

    ``None`` is unreachable in the field — `os.execvpe` replaces this process — and is what
    a test standing in for it gets back, so a stand-in never has to raise to be believed.

    *on_exec* is what a caller records the moment the pane stops being charter's, and it
    returns the undo for it: an `execvpe` that raises after it leaves a pane running nothing
    at all, and a pane running nothing must not also claim to be a chat (N7's nit).
    """
    env = environment(p, os.environ, framed=fid is not None)
    r = refusal(p, root=config.ROOT, attended=attended, env=env)
    if r is not None:
        return r
    undo = on_exec()
    if fid is not None:
        state.record_profile(fid, p.name)
    cmd = profiles.expanded_command(p)
    try:
        os.execvpe(cmd[0], [*cmd, *rest], env)
    except OSError as e:
        undo()
        return Refusal(KIND_EXEC,
                       EXEC_FAILED.format(name=contain.readable(p.name),
                                          cmd=contain.readable(cmd[0]),
                                          why=contain.readable(e.strerror or e)),
                       _exec_exit(e))
    return None


def start(p: profiles.Profile, rest: list[str], *, fid: str | None, attended: bool) -> int:
    """:func:`attempt`, with the refusal said on charter's own terms. The exit code.

    What `charter <profile> --no-frame` and a launch with no terminal run — the refusal is
    printed where the operator is already looking, so nothing waits for a key there.
    """
    r = attempt(p, rest, fid=fid, attended=attended)
    if r is None:
        return 0
    util.err(f"charter: {r.text}")
    return r.exit


def framed_chat() -> str | None:
    """The chat this process is the pane of, or ``None`` — asked of tmux, never of the
    environment (rulings 29 and 33; see the module docstring).

    The claimed chat is `$CHARTER_SESSION_ID`, which tmux put on this pane's window. It
    counts only when tmux itself says this process's pid is the `#{pane_pid}` of a live pane
    that belongs to that chat, on that chat's own server — `state.frame_server`, written
    before the pane existed, so nothing races charter's own pane record and nothing waits.

    A claim that cannot be proven says so in one line rather than downgrading in silence: a
    chat whose window an operator's hook renamed would otherwise lose its session id and its
    resume id with no word said.
    """
    chat = os.environ.get("CHARTER_SESSION_ID", "")
    if not chat:
        return None
    from ..commands_frame import SOCKET

    server = state.frame_server(chat) or SOCKET
    row = tmuxctl.live_pane_by_pid(server, os.getpid())
    if row is not None:
        _pane, _session, window, option = row
        # The window NAME on charter's own server, the `@charter_chat` option on the
        # operator's — where a name is only a label anything may rewrite (ruling 33).
        if (option if tmuxctl.is_operator_socket(server, own=SOCKET) else window) == chat:
            return chat
    util.err(f"charter: {UNPROVEN_CHAT.format(chat=contain.readable(chat))}")
    return None


def _wait_for_the_operator() -> None:
    """Hold the pane open until the operator acknowledges the refusal (ruling 42).

    **A LINE, not a single raw keystroke, and that is a portability decision taken on
    evidence rather than taste.** Reading one keypress means putting the pane's terminal
    into raw mode, and a `tcsetattr` from a pane on a Linux CI runner left the launcher
    gone with an EMPTY `#{pane_dead_status}` — tmux's own reading for *killed by a signal*
    (`commands_frame._UNKNOWN_DEATH_CODE` records the measurement) — so the refusal went
    with the window after all, which is the single thing ruling 42 exists to prevent. Both
    cases in `tests/test_a_chat_pane_starts_as_charter_and_becomes_the_harness.py` were red
    on that runner and green on two tmux versions here, which is how it was found.

    The pane's own line discipline does the waiting instead: no mode change, nothing to
    restore, and Enter is a key every operator has. A Ctrl-C at the prompt is left to mean
    what it always means — the chat ends on 130 rather than on the refusal's own number,
    and it ends either way.

    A pane whose stdin is not a terminal has nobody to press anything and must not block:
    that is the suite, and a `frame-launch` run by hand out of a pipe.
    """
    if not sys.stdin.isatty():
        return
    sys.stdin.readline()


def _handed_over(fid: str | None) -> Callable[[], None]:
    """Record that the pane is the harness's now, and hand back the undo for it.

    What :func:`attempt`'s *on_exec* is for here: the launch that opened an unattended chat
    waits for one of these two answers rather than for a timeout it would pay on every chat
    that started perfectly well (`commands_frame._await_the_launcher`).
    """
    if fid is not None:
        state.record_launch(fid)
    # Nothing to undo: an `execvpe` that raises is about to be recorded as the refusal it
    # is, over this very record, by `_refused_in_pane` below.
    return _nothing


def _refused_in_pane(text: str, code: int, *, fid: str | None, attended: bool) -> int:
    """Say *text* in the pane the way somebody will actually read it, and return *code*.

    Ruling 42, measured: printing and exiting is exactly what nobody reads. An attended pane
    waits for a key; an unattended one leaves the sentence under the chat, because the
    process that opened it is the one with a terminal.
    """
    util.err(f"charter: {text}")
    if fid is not None:
        state.record_launch(fid, code, text)
    if attended:
        util.err(PRESS_ENTER)
        _wait_for_the_operator()
    return code


def cmd_frame_launch(args) -> int:
    """`charter frame-launch --profile <name> [--attended] -- <rest>` — a chat pane's own
    first process, and never a command an operator types.

    The frame proof runs FIRST (ruling 35): before the profile is resolved, before anything
    is claimed, and before a single byte is printed, because the proof holds only while the
    pane has printed nothing.
    """
    fid = framed_chat()
    p, why = resolve(args.profile)
    if p is None:
        return _refused_in_pane(why or unknown_profile(args.profile), REFUSED_EXIT,
                                fid=fid, attended=args.attended)
    rest = list(args.rest or [])
    # `nargs=REMAINDER` keeps the `--` that told argparse to stop parsing; it is the
    # separator and not part of the harness's own argv (`commands_frame._launch` strips it
    # the same way).
    if rest[:1] == ["--"]:
        rest = rest[1:]
    r = attempt(p, rest, fid=fid, attended=args.attended,
                on_exec=lambda: _handed_over(fid))
    if r is None:
        return 0
    return _refused_in_pane(r.text, r.exit, fid=fid, attended=args.attended)
