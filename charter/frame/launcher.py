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
hook then kills the window (`_pane_last_words` answered `[]` in all 40 runs). So every
refusal the pane SAYS is recorded under the chat, where the launch that opened it reads it
back (`state.record_launch`, `commands_frame._await_the_launcher`) — and an **attended** one
is also shown in the pane, which then waits for the operator to press **Enter**. (The one
refusal the pane neither says nor records is a DECLINE: the operator answered that question
here a moment ago and was told `charter: nothing started.` as they did, and a decline is
reachable only from an attended open, which is exactly the open nobody reads the record for.
:func:`already_said` is where that is decided.) Only the WAIT is conditional among the rest,
and it is a LINE rather than a keystroke: reading one key means putting the
pane's terminal into raw mode, and a `tcsetattr` from a pane on a Linux CI runner left the
launcher killed by a signal with the refusal gone with its window
(:func:`_wait_for_the_operator` records that measurement). The pane is charter's to write
in while no harness has run in it (ADR 0018, as its 2026-09-12 amendment states).

**And the same pane is where a new or changed profile is ASKED about** (Task 3). A profile
with no launch record, or one that no longer matches it, is not refused where somebody is
in front of it: :func:`answered` prints its command and its environment and reads one line,
`run this? [y/N]` (`charter/profiletrust.py`). A no exits with the workspace picker's own
cancel code and says nothing more — the operator has just answered. An open nobody is at
gets a refusal in place of the question, and so does one with no terminal on both of its
ends, because a question nobody can answer is a chat that never starts and never says why.
**After a yes the whole chain runs again from the top** before the `exec` (ruling 27).

**Nothing here is on the import path.** `cli` and `commands_frame` reach this module at call
time, because every `charter hook …` process builds the parser and derives config, and
ruling 43 took profile code off that path.
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path
from typing import Callable, Mapping, NamedTuple

from .. import config, contain, profiles, util, wiring
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
#: The one refusal that is a QUESTION rather than an answer: a caller with a terminal asks
#: it (`profiletrust.ask_in_terminal`) and a caller with none prints its text. It is also
#: the only kind `commands_frame._launch` defers to the pane (N2a's nit), which is why that
#: branch is on the kind: a pane is where a press finds the terminal it did not have.
KIND_ASK = "ask"
#: The same profile, at an open nobody is at — a reopen, a restore, a handoff.
KIND_UNATTENDED = "unattended"
#: A yes charter could not write down (N2b's nit).
KIND_RECORD = "record"
#: A yes the record no longer backs: it moved between the write and the re-run (S1).
KIND_MOVED = "moved"
#: The operator read the command and said no. Not a rule firing: their own answer.
KIND_DECLINED = "declined"
KIND_EXEC = "exec"
#: `wiring.KIND_WIRING`, named here so a reader of this list sees every kind a launch can
#: refuse with. The constant lives beside the check that produces it.
KIND_WIRING = wiring.KIND_WIRING


class Refusal(NamedTuple):
    """Why a profile did not start, and what a CLI returns for it."""

    #: One of the KIND_* constants above, and the ONLY thing a caller branches on
    #: (ruling 27): a sentence gets reworded, and a comparison against one is a guard that
    #: stops guarding on the day it does. Two questions are asked of it often enough to
    #: have one home each — :func:`is_a_question` and :func:`already_said`.
    kind: str
    #: The sentence, without charter's own prefix: each caller says it its own way. **``""``
    #: for a refusal whose sentence has already been said** — today that is `KIND_DECLINED`,
    #: which `profiletrust.ask_in_terminal` answered on the terminal it asked on. Callers
    #: ask :func:`already_said` rather than testing this for emptiness.
    text: str
    #: What a CLI returns for it: `MISSING_EXIT` for `KIND_PATH`, its own number for
    #: `KIND_EXEC`, `profiletrust.DECLINED_EXIT` for `KIND_DECLINED` — the workspace
    #: picker's own cancel code, because a decline is a cancel — and `REFUSED_EXIT` for the
    #: rest.
    exit: int


# Every sentence says the rule worked and names the fix in the same breath (CONTEXT.md,
# *A refusal is the rule working*), and every profile-derived value in one is contained
# (ruling 35): `charter.local.toml` is a file a chat can write, and a `\r` or an ESC in a
# command could otherwise redraw the line to show a harmless command while another one runs.
UNKNOWN_PROFILE = ("no profile named '{name}' — have: {names}. Nothing was started. "
                   "charter harness list shows every profile and why any was refused.")
MISMATCHED_KIND = ("profile '{name}' is a {kind} profile, not a {asked} one — nothing was "
                 "started. Run it by its own name: charter {name}")
IGNORED = "profile '{name}' is refused — {why}"
NOT_ON_PATH = ("profile '{name}' runs {cmd}, which is not on PATH — nothing was started. "
               "Install it, or give the profile a command with a full path in "
               "charter.local.toml.")
EXEC_FAILED = ("profile '{name}' could not be started — {cmd} failed to run ({why}). Nothing "
               "is running here; fix the command in charter.local.toml.")
UNPROVEN_CHAT = ("this launch was given chat '{chat}' but is not that chat's pane, so it "
                 "runs with no frame and records nothing for '{chat}'.")

#: What an attended pane says under its refusal. The pane is charter's while no harness has
#: run in it, and the keypress is what keeps the window from closing over the sentence
#: (ruling 42).
PRESS_ENTER = "  press Enter to close this chat."


def is_a_question(r: Refusal | None) -> bool:
    """Is *r* the refusal that is a QUESTION rather than an answer?

    One home for a test three callers make — :func:`attempt` to know what to ask,
    `commands_frame._launch` and `_the_pane_will_ask` to know what a pane will ask instead —
    because each of them means the same thing by it and a fourth kind of question would
    otherwise have to find all three.
    """
    return r is not None and r.kind == KIND_ASK


def already_said(r: Refusal) -> bool:
    """Has *r*'s sentence already reached the operator, so that saying it again would be
    charter repeating itself?

    The other test three callers make — :func:`start`, :func:`cmd_frame_launch` and
    `commands_frame._launch` — and it has one home for the same reason. Today exactly one
    kind answers yes: a decline, which `profiletrust.ask_in_terminal` has already answered
    with `charter: nothing started.` on the terminal it put the question on, and whose
    `text` is `""` because of it. Printing it again under charter's red ✗ would report the
    operator's own answer as a rule firing on them.
    """
    return r.kind in _ALREADY_SAID


#: The kinds :func:`already_said` answers yes for. A set rather than an `==`, because it is
#: the list that is the decision: a second such kind is one entry here rather than an edit
#: at each of the three sites that ask.
_ALREADY_SAID = frozenset({KIND_DECLINED})


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

    *attended* decides what the last link answers: an open somebody is in front of gets the
    question (`KIND_ASK`), and one nobody is at gets a refusal in its place.

    **The wiring probe is last, and that is the order it costs in.** It runs the profile's
    own command (137-718 ms measured) and writes into the folder it asks about, so a profile
    refused for any earlier reason is never probed — and a profile whose command charter may
    not run yet is never probed at all (ruling 1). Every call is a FRESH probe: a launch
    never reads `wiring.cached`, because that file is as writable by a chat as
    `charter.local.toml` is (ruling 21).
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
    asked = _approval_refusal(p, attended=attended)
    if asked is not None:
        return asked
    # `Path.cwd()`: the directory `_launch` stands in before tmux, and the pane's own start
    # directory after. Claude Code resolves `enabledPlugins` there, so it is the directory
    # the question is actually about.
    why = wiring.refusal(p, cwd=Path.cwd())
    if why:
        return Refusal(KIND_WIRING, why, REFUSED_EXIT)
    return None


def _approval_refusal(p: profiles.Profile, *, attended: bool) -> Refusal | None:
    """The last link: has the operator seen this command? (`profiletrust`)

    **`main` never runs an unapproved declared command.** `charter.local.toml` is a file a
    chat can write with no diff to show for it, and its `command` goes to tmux rather than
    through a harness's own permission prompt — so a profile with no launch record, or one
    that no longer matches it, is shown and asked about before it runs. A replacement of a
    built-in (`[harness.claude]`) is a declaration and asks with the rest; a built-in the
    file does not replace comes out of charter's own registry and never asks.

    One line, because it is a seam rather than a rule: `profiletrust` owns the record, the
    two sentences and the question, and this is where the chain reaches them.
    """
    from .. import profiletrust

    return profiletrust.refusal(p, attended=attended)


def answered(p: profiles.Profile, ask: Refusal, *,
             again: Callable[[], Refusal | None]) -> Refusal | None:
    """Put *ask*'s question to whoever is at this process's terminal, and on a yes run the
    whole chain *again* before answering ``None``.

    **One function because it is one rule**, and it had grown two homes — `attempt` for the
    launcher's own chain and `commands_frame._asked_here` for the one before tmux. What
    differs between them is only which chain to re-run, so that is the parameter and the
    rest is here.

    Four answers, and each is its own kind, because every caller of this branches on one
    (ruling 27) and none of them may tell them apart by their text:

    * **no terminal** — *ask* itself comes back, and its text is `NEEDS_ASKING`: somebody
      typed this and there is nowhere to put the question (review 3). Nothing is read from
      the stream, which is a stronger claim than "does not block": a read on a pipe returns
      at once and looks exactly like a question that was answered.
    * **a yes charter could not record** — `KIND_RECORD`, and *again* is NOT run (N2b's
      nit). Going round would find no record and ask the identical question a second time,
      which is the one thing an operator cannot fix by answering.
    * **anything else** — `KIND_DECLINED`, with no text at all, because
      `profiletrust.ask_in_terminal` has already said `charter: nothing started.` on the
      terminal the question went to. The exit code is the workspace picker's cancel (130).
    * **a yes whose chain still asks** — `KIND_MOVED` (S1). The record lives under
      `.charter/`, which a chat can write (ruling 13), so *again* can come back holding the
      very same question. Asking twice is N2b's case one race along, and the sentence that
      used to come out of here was `NEEDS_ASKING` — *no terminal here to ask in* — said on a
      terminal that plainly had one. It says what actually happened instead.

    **The whole chain, from the top** (ruling 27). A yes answers the approval question and
    nothing else: the plane can move while the operator is reading the prompt — a
    `.gitignore` edited, the command uninstalled, and from Task 4 a profile that stops being
    wired — and a yes that walked straight past the checks behind it would be the one place
    in charter where saying yes to one question waives the rest.
    """
    from .. import profiletrust

    if not profiletrust.can_ask(sys.stdin, sys.stdout):
        return ask
    said = profiletrust.ask_in_terminal(p, stdin=sys.stdin, stdout=sys.stdout)
    if said.why:
        return Refusal(KIND_RECORD,
                       profiletrust.RECORD_NOT_WRITTEN.format(
                           name=contain.readable(p.name),
                           # The PATH limit and not the display one: this sentence ends in
                           # "fix that path and run it again", and a path clipped at 160 is
                           # one the reader cannot act on (`contain.PATH_DISPLAY_LIMIT`).
                           path=contain.readable(config.STATE_DIR / profiletrust.RECORD,
                                                 contain.PATH_DISPLAY_LIMIT),
                           why=contain.readable(said.why)),
                       REFUSED_EXIT)
    if not said.yes:
        return Refusal(KIND_DECLINED, "", profiletrust.DECLINED_EXIT)
    left = again()
    if is_a_question(left):
        return Refusal(KIND_MOVED,
                       profiletrust.CHANGED_WHILE_ASKING.format(
                           name=contain.readable(p.name)),
                       REFUSED_EXIT)
    return left


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

    **The ask is here**, because this is the one function every path reaches — the pane,
    `--no-frame`, and a launch whose output is a pipe — and a question asked in only some
    of them is an approval that depends on how the harness was started.
    """
    env = environment(p, os.environ, framed=fid is not None)
    r = refusal(p, root=config.ROOT, attended=attended, env=env)
    if is_a_question(r):
        r = answered(p, r, again=lambda: refusal(p, root=config.ROOT, attended=attended,
                                                 env=env))
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
    printed where the operator is already looking, so nothing here holds a pane open for
    them to read it.

    **A decline says nothing more.** `profiletrust.ask_in_terminal` has already answered it
    on the terminal the question went to, and repeating it under charter's red ✗ would
    report the operator's own answer as a rule firing.
    """
    r = attempt(p, rest, fid=fid, attended=attended)
    if r is None:
        return 0
    if not already_said(r):
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
    # No `, ""` default: the next line asks whether there is a chat at all, and `None`
    # answers it exactly as `""` does — a fallback no test can tell apart from its absence
    # is a line the deletion sweep reports, rightly.
    chat = os.environ.get("CHARTER_SESSION_ID")
    if not chat:
        return None
    from ..commands_frame import SOCKET

    server = state.frame_server(chat) or SOCKET
    row = tmuxctl.live_pane_by_pid(server, os.getpid())
    if row is not None:
        # The window NAME on charter's own server, the `@charter_chat` option on the
        # operator's — where a name is only a label anything may rewrite (ruling 33). Read
        # by name off `tmuxctl.LivePane`, because which of the two proves a chat is the one
        # thing here it would be worst to get subtly wrong.
        proof = row.chat if tmuxctl.is_operator_socket(server, own=SOCKET) else row.window
        if proof == chat:
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

    Ruling 42, measured: printing and exiting is exactly what nobody reads. So the sentence
    is left under the chat **on both paths this runs on** — the record is what an unattended
    open reads back (`commands_frame._await_the_launcher`), and an attended one that the
    operator closes before reading has left it somewhere all the same. A decline never
    reaches here at all (:func:`cmd_frame_launch` returns on :func:`already_said` first), and
    does not need to: it says nothing the operator has not just been told.

    **Only the wait is conditional.** An attended pane holds itself open until the operator
    presses Enter — a LINE and not a keystroke, for the reason
    :func:`_wait_for_the_operator` measures — and a pane nobody is at never stops on
    anything.
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
    # `list(args.rest)` and not `args.rest or []`: `nargs=REMAINDER` answers with a list on
    # every path, `[]` included, so the fallback was a branch nothing could reach.
    rest = list(args.rest)
    # `nargs=REMAINDER` keeps the `--` that told argparse to stop parsing; it is the
    # separator and not part of the harness's own argv (`commands_frame._launch` strips it
    # the same way).
    if rest[:1] == ["--"]:
        rest = rest[1:]
    r = attempt(p, rest, fid=fid, attended=args.attended,
                on_exec=lambda: _handed_over(fid))
    if r is None:
        return 0
    if already_said(r):
        # **The one refusal a pane neither shows, records nor waits on.** The operator
        # answered the question in this very pane a moment ago — there is nothing here they
        # have not read, nothing for them to press, and nothing for another process to read
        # back, because no other process is waiting on an attended open. The window closes
        # the way it does for any other exit, on the picker's own cancel code.
        return r.exit
    return _refused_in_pane(r.text, r.exit, fid=fid, attended=args.attended)
