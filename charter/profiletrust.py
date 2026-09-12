"""What the operator approved a profile to run — the launch record, and the ask that fills it.

Charter writes down each profile's `kind`, `command` and `env` the moment it launches one.
A profile with no record, or one that no longer matches, is **new or changed**: charter
shows what it would run and asks `run this? [y/N]` before it runs it. Built-ins never ask —
their command comes out of charter's own registry, which no file and no chat can edit.

**Why there is a question here at all**, when nothing else in charter stops to ask:

* once `charter.local.toml` is ignored, an edit to it leaves **no diff** — no review, no
  `git status`, nothing on a branch for anybody to notice;
* nothing stops a chat editing plane config, and a chat is an agent with a shell;
* the command goes to **tmux**, so it never passes a harness's own permission prompt on the
  way — by the time a harness could ask about it, that harness IS the command;
* Codex trusts hooks by hash for exactly this reason, and a profile's `command` is the same
  shape of thing: a line of config that becomes a process.

**The limit, at full volume** (ruling 13). A chat that can edit `charter.local.toml` can
also edit `.charter/harness-profiles-launched.json`, which is this record: both sit under
paths no guard covers, and a path pattern is host policy rather than charter's (ADR 0014).
So the ask catches a command the operator did not change themselves **unless whatever
changed it also forged the record**. That is worth having — it closes the accident and the
careless edit, and it is the difference between a command that ran unseen and one that was
read out loud first — and it is not a boundary. The docs say so in the same words.

**Nothing here is on the import path** (ruling 43). Every `charter hook …` process builds
the parser and derives config; profile code is charged to all of them if it joins that, and
CI's deletion sweep cannot finish when it is. `launcher` and `commands_frame` reach this
module at call time, and this module reaches `launcher` the same way.
"""

from __future__ import annotations

import json
from typing import TYPE_CHECKING, Iterable, NamedTuple

from . import config, contain, profiles

if TYPE_CHECKING:                                   # pragma: no cover - typing only
    from .frame import launcher

#: The record, under `config.STATE_DIR` — which `charter init` ignores, and which
#: `config.private_mkdir` creates at 0700.
RECORD = "harness-profiles-launched.json"

#: What a launch the operator declined returns. The workspace picker's own cancel code
#: (`commands_frame._PICKER_CANCELLED`): 130 is the shell's "ended by SIGINT", which is what
#: saying no to a question is, and it is deliberately not 0 — a script that got 0 back would
#: take a frame that was never started for one that ran and exited cleanly.
DECLINED_EXIT = 130

#: The two answers :func:`approval_needed` gives, as one word each. They are read back into
#: every sentence below, so they are words rather than flags.
NEW = "new"
CHANGED = "changed"

#: Every sentence says the rule worked and names the fix in the same breath (CONTEXT.md,
#: *A refusal is the rule working*), and every profile-derived value in one is contained
#: (ruling 35).
NEEDS_ASKING = (
    "profile '{name}' is {state}, and charter asks before it runs a command it has not "
    "been shown before — but there is no terminal here to ask in, so nothing was started. "
    "Run it where you can answer: charter {name}")
UNATTENDED = (
    "profile '{name}' is {state}, and nobody is at this open to approve it — nothing was "
    "started. Run it once yourself so charter can ask: charter {name}")
RECORD_NOT_WRITTEN = (
    "you approved profile '{name}', but charter could not record that at {path} ({why}), "
    "so it will not start it — it would only ask you again. Nothing was started; fix that "
    "path and run it again.")
#: What a yes buys nothing, because the record moved under it (S1). Its own sentence rather
#: than asking again — N2b's reason, one race along: a second question is one the operator
#: cannot settle either, because whatever answered the first one can answer the next.
CHANGED_WHILE_ASKING = (
    "profile '{name}' is {state} again — it moved while that question was on screen, so "
    "what you approved is not what would run now. Nothing was started; run it again and "
    "read the command it shows you: charter {name}")
#: And the reopen's, which lives here beside its two siblings rather than in
#: `commands_frame`: all three read :data:`NEW` and :data:`CHANGED` into a sentence, and
#: split across two modules a reword of either word is two edits with nothing holding them
#: together. `commands_frame` fills in the chat, which is the half only a reopen knows.
REOPEN_UNAPPROVED = ("charter reopen: {chat} runs profile '{name}', which is {state} — not "
                     "reopened, because a reopen has nobody to ask. Run charter {name} once "
                     "to approve it, then charter reopen.")

#: The prompt, line by line. The headline says which state it is in and why that is a
#: question; the rows are what would run; the last line is the question itself, and it is
#: last so that it is the thing still on screen when the cursor stops.
HEADLINE = {NEW: "charter: profile '{name}' is new — it has not run on this machine before.",
            CHANGED: "charter: profile '{name}' has changed since it last ran."}
QUESTION = "run this? [y/N] "

#: What a decline says, on the terminal the question was asked on. `_choose_workspace` says
#: exactly this when the workspace picker is cancelled, and a person who has just answered
#: two of charter's questions should not have to learn two vocabularies for "no".
NOTHING_STARTED = "charter: nothing started."

#: The answers that mean yes. Everything else — a bare Enter, a typo, end of input — is no,
#: because the fat-fingered path has to be the one that starts nothing.
_YES = ("y", "yes")


class Answer(NamedTuple):
    """What the operator said, and what charter managed to do about it.

    Two fields rather than a `bool`, because a yes has two outcomes and they are not the
    same launch (the fourth review's nit): one is approved and recorded, and one is
    approved and **not** recorded, which must refuse rather than run — re-running the chain
    after it would find no record and ask the identical question a second time (N2b).
    """

    #: Whether the answer was yes.
    yes: bool
    #: Why a yes could not be written down — ``""`` on every other path.
    why: str


def fingerprint(p: profiles.Profile) -> dict:
    """What is recorded for *p*: its kind, its command and its environment.

    **As DECLARED, before `~` is expanded.** The file is what an edit changes, and `$HOME`
    is not something a chat moves — a record of the expansion would read as *changed* for
    every profile on the day somebody's home directory did move, and would ask again about
    a command nobody touched.
    """
    return {"kind": p.kind, "command": list(p.command), "env": dict(p.env)}


def _read() -> dict:
    """Every profile's record, or ``{}``.

    **Unreadable, malformed or missing all read as `{}`**, so everything declared asks
    again. It fails towards ASKING and never towards running: a file charter cannot read
    says nothing about what the operator approved, and treating silence as a yes is the one
    state this record exists to keep out.
    """
    try:
        got = json.loads((config.STATE_DIR / RECORD).read_text())
    except (OSError, ValueError):
        return {}
    return got if isinstance(got, dict) else {}


def last_launched(name: str) -> dict | None:
    """*name*'s recorded fingerprint, or ``None`` when there is not one.

    An entry that is not a fingerprint is not one either: the file is a plain JSON object a
    chat can write, so `{"claude-work": "approved, honest"}` has to read as no record at
    all rather than as something to compare against.
    """
    got = _read().get(name)
    return got if isinstance(got, dict) else None


def record_launched(p: profiles.Profile) -> str:
    """Write *p* down as launched. ``""``, or why it could not be — **never raises**.

    Called from a launch that is about to `exec` and from a press that runs under a hook,
    where an exception is a turn that ends with a traceback instead of a chat. The caller
    decides what a failure means, because the two callers do not agree: the ask refuses the
    launch over it (N2b), and nothing else has to.

    One object keyed by profile name, read and rewritten whole, so approving one profile
    today does not make another ask again tomorrow.

    What comes back is what the filesystem said and not where it said it: the path is
    charter's own and every caller that quotes one already knows it
    (:data:`RECORD_NOT_WRITTEN`), so putting it in here would print it twice.
    """
    try:
        config.private_mkdir(config.STATE_DIR)
        config.replace_for(config.STATE_DIR / RECORD,
                           json.dumps({**_read(), p.name: fingerprint(p)}, indent=2) + "\n")
    except OSError as e:
        return e.strerror or str(e)
    return ""


def _state_and_record(p: profiles.Profile) -> tuple[str, dict]:
    """*p*'s approval state and the fingerprint it was compared against — **read once**.

    One read rather than two, and that is why this exists instead of the prompt asking
    `last_launched` again for itself: between two reads the file can change, and the second
    answer would have to carry a fallback for a record the first one had already seen. A
    fallback nothing can reach is a line the deletion sweep is right to call dead.
    """
    if p.source == profiles.BUILTIN:
        return "", {}
    was = last_launched(p.name)
    if was is None:
        return NEW, {}
    return ("" if was == fingerprint(p) else CHANGED), was


def approval_needed(p: profiles.Profile) -> str:
    """:data:`NEW`, :data:`CHANGED`, or ``""`` when *p* may run.

    **A built-in never asks.** Its command comes out of charter's own registry rather than
    out of a file, so what a chat could have written decides nothing about it — and a
    question that never carries risk is one an operator learns to answer yes to without
    reading. A profile the file DECLARES with a built-in's name (`[harness.claude]`) is a
    declaration and asks with the rest: the name is the built-in's, and the command is the
    file's.
    """
    return _state_and_record(p)[0]


def can_ask(stdin, stdout) -> bool:
    """Is there a terminal here to put a question on? (ruling 23)

    **Both, and that is the whole point of asking twice.** `charter claude-work > log` has
    somebody at the keyboard and nowhere to print the question; `charter claude-work <
    /dev/null` has the screen and nothing to read back. Either one asked anyway is a
    process that never returns, which is worse than any refusal.
    """
    return stdin.isatty() and stdout.isatty()


def _whole(text: str) -> str:
    """*text* escaped to printable ASCII and **not clipped** — the prompt's containment.

    **A sentence clips; a prompt does not**, and that is the difference between this and
    `contain.readable`. A refusal is one line that ends in a remedy, so a 200-character
    name in the middle of it pushes the remedy off the screen and `readable`'s 160-character
    clip is right there. This prompt is the opposite surface: it exists so that somebody can
    read the command before approving it, and a command approved with its tail hidden is
    the one outcome the whole feature is against — `claude --settings <400 bytes of
    somewhere else>` would be clipped to something that looks fine. It wraps instead.

    `contain.escaped` for the escaping itself, so this shares one implementation of *what
    may reach a terminal* with every other surface rather than growing a second: every byte
    outside U+0020..U+007E comes out as a reversible escape, whatever its category. The
    blankness rule is `readable`'s, for `readable`'s reason: after `escaped` the string is
    printable ASCII, so "renders as nothing" is exactly "is spaces", and a word that renders
    as nothing must not read as no word at all.

    **Asked as a question about the whole string rather than as a strip**, which is the one
    thing here that is not `readable`'s spelling. `shown.strip(" ")` and `shown.lstrip(" ")`
    are equivalent for deciding this — either leaves nothing exactly when every character is
    a space — so a sweep can never tell the two apart and the line comes back a survivor
    that no test could have pinned. `set(shown) <= {" "}` IS the decision: every character is
    a space, empty included, and there is no second spelling of it to drift to.
    """
    shown = contain.escaped(str(text))
    return contain.BLANK if set(shown) <= {" "} else shown


def _words(command: Iterable[str]) -> str:
    """A command as the prompt shows it, **contained word by word** (ruling 35), whole.

    It comes out of a file a chat can write and it is about to be drawn on a terminal, so a
    `\\r` or an ESC in it could otherwise redraw this very prompt to show a harmless command
    while another one is approved. The `was` line gets the same treatment for the same
    reason one level along: the record is under `.charter/`, which is as writable as the
    file (ruling 13).
    """
    return " ".join(_whole(word) for word in command)


def _names(env: Iterable[tuple[str, str]]) -> str:
    """An environment as the prompt shows it — `NAME=value` a pair at a time, contained.

    Pairs rather than a mapping, because the two callers hold it differently:
    `profiles.Profile.env` is a tuple of pairs already sorted, and a record read back out of
    `.charter/` is a `dict` whose `.items()` the caller sorts. Annotated as the pairs both
    of them are, so a third caller cannot quietly hand it a third shape.
    """
    return " ".join(_whole(f"{name}={value}") for name, value in env)


def _prompt(p: profiles.Profile, state: str, was: dict) -> str:
    """The whole question as one block of text, ending in :data:`QUESTION`.

    The command and the environment on rows of their own, because they are two different
    decisions — which program, and which account — and a single line runs them together.
    """
    lines = [HEADLINE[state].format(name=_whole(p.name)),
             f"  command  {_words(p.command)}"]
    if p.env:
        lines.append(f"  env      {_names(p.env)}")
    if state == CHANGED:
        # What it WAS, so the operator can read the change rather than only the new
        # command: "is this the command you meant" is a different question from "did you
        # mean to swap these two". One line, because it is the thing being compared against
        # rather than the thing being approved.
        #
        # `dict.get` with a default and not `or`: *was* is a record a chat can write, so an
        # entry missing either key is ordinary rather than exotic — and `if x` drops the
        # half that is empty, because a `was` row that begins with a space reads as a
        # command that begins with one.
        lines.append("  was      " + " ".join(
            x for x in (_names(sorted(was.get("env", {}).items())),
                        _words(was.get("command", []))) if x))
    return "\n".join([*lines, QUESTION])


def ask_in_terminal(p: profiles.Profile, *, stdin, stdout) -> Answer:
    """Show *p*'s command on *stdout*, read one line off *stdin*, and record a yes.

    Streams rather than `input()`/`print()`, for `frame/picker.py`'s reason: a prompt is
    exactly the shape of test this repo keeps getting wrong, and one that needed a real tty
    to prove that `n` starts nothing would be testing the terminal instead of the rule.

    End of input and `KeyboardInterrupt` both decline. A closed stdin is a launch driven
    from a script, and ^C at a prompt means "not this" — neither may come out of here as a
    traceback in a pane somebody is looking at.

    **A profile that needs no approval by the time this runs is not asked about** (S1), and
    that state is reachable rather than defensive: `launcher.refusal` read the record to
    decide there was a question, this reads it again to build the prompt, and between the
    two a second terminal, the pane a `+` opened, or the chat this module's docstring names
    can have written it. A yes is the honest answer — the operator's approval is on disk —
    and it is the only one that does not either crash on a headline for a state with no
    question in it or put a question nobody needs on a screen.
    """
    state, was = _state_and_record(p)
    if not state:
        return Answer(True, "")
    stdout.write(_prompt(p, state, was))
    stdout.flush()
    try:
        line = stdin.readline()
    except KeyboardInterrupt:
        line = ""
    if line.strip().lower() not in _YES:
        # On the same stream the question went to, because it is the second half of one
        # exchange — and in the words `_choose_workspace` already uses for a cancel.
        stdout.write(f"\n{NOTHING_STARTED}\n")
        stdout.flush()
        return Answer(False, "")
    return Answer(True, record_launched(p))


def refusal(p: profiles.Profile, *, attended: bool) -> "launcher.Refusal | None":
    """Why *p* may not start yet, or ``None`` — the last link in `launcher.refusal`'s chain.

    Two kinds and never two sentences to tell apart (ruling 27): an **attended** open gets
    :data:`launcher.KIND_ASK`, which the caller answers by asking if it has a terminal and
    by printing this text if it has not; an unattended one gets
    :data:`launcher.KIND_UNATTENDED`, because a reopen, a restore and a handoff have nobody
    to put the question to and a pane that waits on one never starts and never says why.
    """
    from .frame import launcher

    state = approval_needed(p)
    if not state:
        return None
    said = {"name": contain.readable(p.name), "state": state}
    if attended:
        return launcher.Refusal(launcher.KIND_ASK, NEEDS_ASKING.format(**said),
                                launcher.REFUSED_EXIT)
    return launcher.Refusal(launcher.KIND_UNATTENDED, UNATTENDED.format(**said),
                            launcher.REFUSED_EXIT)
