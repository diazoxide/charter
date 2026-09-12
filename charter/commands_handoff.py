"""``charter handoff`` — open a chat in a workspace you name, started on an approved brief.

    charter handoff <workspace> [--create --vision "<vision>"] [--persona <name>] <<'BRIEF'
    <the brief>
    BRIEF

**The consent is the harness's own permission prompt for that exact spelling**, not
anything in this file: the brief becomes the new chat's first user message and runs with
the operator's authority, so `charter init` writes an `ask` rule for `charter handoff *`
and `hooks.pretooluse`'s A7 refuses the spellings that rule was measured not to match
(`docs/handoff.md`). What this file adds is every refusal a permission prompt cannot see,
and it asks all of them **before the first write**, because charter fails toward no change:
a handoff that created the workspace and then discovered the brief was empty has already
cost somebody a cleanup.

The order is the spec's, and the order is the design:

1. the workspace name, the flag pairs, the persona — questions answered from the plane;
2. the brief on stdin, read before the frame check because the command charter prints when
   there is no frame has to carry it;
3. the frame — this shell has to be a chat charter can open a background window beside;
4. `commands_frame.background_refusal`, the seam's own answer, asked while it is still free.

Then the writes: the workspace, the todo, then the chat — which carries the private brief in
with it, so the file exists before the harness that reads it — then the tally, the strip and
the line.

**The strip is what makes a background chat visible** (the spec's *The strip*). A handoff
opens a window nobody is looking at, so the last thing this command does is move where the
eye goes: the target workspace goes to the front of the plane's tab order, its tab is marked
arrived until any terminal on this plane looks at it, and the calling chat's own attention
row names the chat that was opened. All of it after the open, so a refused handoff points at
nothing.
"""

from __future__ import annotations

import os
import sys

from . import (commands_frame, config, contain, dispatch, handoff, harness, persona, todos,
               util, workspace)
from .frame import chats, notify, state, switch, tmuxctl

BAD_NAME = ("charter handoff: '{ws}' cannot name a workspace — nothing was opened. A "
            "workspace name is letters, digits, '.', '_' and '-', and does not start with "
            "a dot.")
VISION_WITHOUT_CREATE = (
    "charter handoff: --vision describes a workspace this call creates, and '{ws}' is not "
    "being created — nothing was opened. Set an existing workspace's vision with: charter "
    "workspace vision --workspace {ws} \"<the goal>\"")
CREATE_WITHOUT_VISION = (
    "charter handoff: --create needs --vision — a workspace with no vision is never "
    "proposed as a target, so it would be created unfindable. Nothing was opened.")
ALREADY_EXISTS = (
    "charter handoff: workspace '{ws}' already exists, and --create only makes a new one — "
    "nothing was opened. Drop --create to hand off into it.")
NO_SUCH_WORKSPACE = (
    "charter handoff: no workspace '{ws}' on this plane — nothing was opened. Create it in "
    "the same call: charter handoff {ws} --create --vision \"<what it is for>\"")
NO_SUCH_PERSONA = (
    "charter handoff: no persona '{p}' — have: {have}. Nothing was opened.")
#: The kind, never the matched text — `hooks._secret_kind`'s own discipline, reused rather
#: than a second classifier (plan Open question 20).
SECRET_BRIEF = (
    "charter handoff: the brief looks like it carries a secret ({kind}) — nothing was "
    "opened. A "
    "brief travels to the new chat as a command-line argument any local process can read "
    "while the harness starts, so it never carries a secret. Name where the credential "
    "lives (a vault and key) instead of pasting it.")
NOT_IN_A_FRAME = (
    "charter handoff: this shell is not a chat in a charter frame, so there is no frame to "
    "open a chat in the background of — nothing was opened.\n"
    "  Run this in a new terminal instead:\n  {command}")
YOUR_OWN_TMUX = (
    "charter handoff: this chat is a window in a tmux you already had, where charter's "
    "launcher stays awake for the life of the harness it starts, so it cannot open a chat "
    "in the background — nothing was opened.\n"
    "  Run this in a new terminal instead:\n  {command}")
#: Said under the command above when nothing names a harness. charter prints the word it
#: cannot know rather than picking one — the brief is already in that argv.
NO_HARNESS_NAMED = (
    "  Nothing here says which harness to start and this plane declares no `[harness] "
    "default`, so put the one you want where `{placeholder}` stands (opencode takes "
    "`--prompt` in front of the message; `claude` and `codex` take it as it is).")
#: The byte bound belongs to `commands_frame`, which measures the MESSAGE. Only this
#: command knows that message is a stamp, a blank line and a brief — so only this command
#: can say why a brief that fits alone was refused.
STAMPED_MESSAGE = (
    "  The bytes counted are the message the new chat is sent: the stamp line, a blank "
    "line, then your brief, which is {n} bytes on its own. A brief that fits alone can be "
    "over once it is stamped.")
#: What a partway failure says. It names what is left behind and then what did NOT happen,
#: and it stops short of "nothing else was written": the launcher that failed had already
#: claimed a chat directory and put this brief in it (`commands_frame.Opening.brief`), and
#: that directory is the launcher's to leave and `state.reap`'s to clear. What the operator
#: is actually asking is whether something is out there working on this.
NOTHING_ELSE = (
    "charter handoff: {why}\n  What stays: {stays}. No chat opened, so the brief was sent "
    "to nobody and nothing is running.")
OPENED = "charter handoff: opened chat {chat} in workspace '{ws}', started on the brief"
#: Said when the chat opened and its workspace could not be marked arrived. The chat is
#: real and is named; what is missing is the thing that would have pointed at it, so the
#: line hands that job back to the operator rather than reporting a failure they cannot
#: place. The workspace tab still moved to the front of the strip — that is a different
#: record and it did not fail — so "look at it" is a thing they can act on now.
UNMARKED = ("charter handoff: {chat} is open in '{ws}', but charter could not mark that "
            "workspace's tab as arrived — its state directory refused the write. Nothing "
            "on the strip will point at the new chat; its tab did move to the front.")


def _printed_command(*, ws: str, msg: str, create_vision: str | None,
                     persona_name: str | None) -> tuple[str, bool]:
    """``(the command to run in a new terminal, whether a harness could be named)``.

    The harness is the one THIS process is running inside — a handoff never changes harness
    (spec: Limits) — read from `harness.current()`, which trusts ``$CHARTER_HARNESS``
    first. A plane's `[harness] default` is the fallback, which is `_same_harness_as`'s
    second rung said for a shell that is not a chat.
    """
    h = harness.get(harness.current())
    if h is None:
        # `harness.current()` hands back ``$CHARTER_HARNESS`` verbatim even for a name
        # charter has never met, so `get` answering None covers both "nothing said" and
        # "something charter cannot launch".
        fallback = (config.HARNESS or {}).get("default")
        h = next((x for x in harness.all() if x.cli_name == fallback), None)
    # A harness charter has not measured the first message of answers `None` here, and is
    # treated as no harness at all rather than handed the positional spelling as a guess —
    # `background_refusal` refuses that harness for the same reason one seam over.
    extra = h.first_message_argv(msg) if h is not None else None
    named = extra is not None
    return handoff.terminal_command(
        cli_name=h.cli_name if named else handoff.UNKNOWN_HARNESS, workspace=ws,
        extra=extra if named else [msg], create_vision=create_vision,
        persona=persona_name), named


def cmd_handoff(args) -> int:
    ws = args.workspace
    if not workspace.valid_name(ws):
        util.err(BAD_NAME.format(ws=contain.one_line(ws)))
        return 1
    if args.vision and not args.create:
        util.err(VISION_WITHOUT_CREATE.format(ws=ws))
        return 1
    if args.create and not args.vision:
        util.err(CREATE_WITHOUT_VISION)
        return 1
    # `cmd_workspace_use`'s rule, asked rather than re-invented: the always-present
    # workspace exists whether or not its directory does, so `--create default` on a fresh
    # plane is a name collision rather than a creation.
    exists = ws in workspace.list_workspaces() or ws == config.DEFAULT_WORKSPACE
    if args.create and exists:
        util.err(ALREADY_EXISTS.format(ws=ws))
        return 1
    if not args.create and not exists:
        util.err(NO_SUCH_WORKSPACE.format(ws=ws))
        return 1
    if args.persona and (not persona.valid_name(args.persona)
                         or args.persona not in persona.list_personas()):
        util.err(NO_SUCH_PERSONA.format(p=contain.one_line(args.persona),
                                        have=switch._some(persona.list_personas())))
        return 1

    brief, refusal = handoff.read_brief(sys.stdin, ws)
    if refusal:
        util.err(refusal)
        return 1
    # The guard's own classifier, so the brief a handoff refuses and the command a leak
    # guard refuses are the same set of shapes.
    from . import hooks
    kind = hooks._secret_kind(brief)
    if kind:
        util.err(SECRET_BRIEF.format(kind=kind))
        return 1

    fid = os.environ.get("CHARTER_SESSION_ID", "")
    # A chat of THIS plane whose harness pane is this process's: the two halves `state.is_live`
    # documents, because a frame id can be inherited by a shell that is not in the frame.
    in_a_chat = chats.is_chat(fid) and state.is_live(fid, pane=os.environ.get("TMUX_PANE"))
    # Neither stamp value asks `in_a_chat`, and the deletion sweep is what settled that.
    # `chats.is_chat("")` is False, so a chat's id is never empty and `fid or NO_CHAT` is
    # `fid` for every chat; and `state.workspace_for` ENDS in `workspace.resolve()` for an
    # id it knows nothing about, so it already answers the outside-a-frame case. Two
    # conditionals no input could make observable, which in this repository is the same
    # finding as dead code.
    source_chat = fid or handoff.NO_CHAT
    source_ws = state.workspace_for(fid)
    msg = handoff.first_message(handoff.stamp(source_chat, source_ws), brief)
    if not in_a_chat or tmuxctl.is_operator_socket(state.frame_server(fid)
                                                   or commands_frame.SOCKET,
                                                   own=commands_frame.SOCKET):
        # `--create` without `--vision` was refused above, so the vision is there whenever
        # the workspace is being made, and `None` says it is not.
        command, named = _printed_command(ws=ws, msg=msg,
                                          create_vision=args.vision if args.create else None,
                                          persona_name=args.persona)
        said = (NOT_IN_A_FRAME if not in_a_chat else YOUR_OWN_TMUX).format(command=command)
        if not named:
            said += "\n" + NO_HARNESS_NAMED.format(placeholder=handoff.UNKNOWN_HARNESS)
        util.err(said)
        return 1
    refusal = commands_frame.background_refusal(ws, caller=fid, first_message=msg)
    if refusal:
        size = len(os.fsencode(msg))
        # Compared against the seam's own sentence rather than re-deriving its bound here:
        # two places counting bytes is how the note comes to appear beside a refusal that
        # was about something else.
        if refusal == commands_frame.LONG_FIRST_MESSAGE.format(
                n=size, max=commands_frame.FIRST_MESSAGE_MAX_BYTES):
            refusal += "\n" + STAMPED_MESSAGE.format(n=len(os.fsencode(brief)))
        util.err(f"charter handoff: {refusal}")
        return 1

    # ---- past every refusal; from here charter writes ---------------------------------
    if args.create:
        # `ensure` scaffolds, so the workspace is a workspace and not a bare directory.
        # Never `set_live`: a created workspace is LOCAL, and a LIVE one would commit the
        # todo recorded below into a repository nobody asked to publish it to.
        workspace.ensure(ws)
        workspace.set_vision(ws, args.vision)
    text = handoff.todo_text(brief, source_chat=source_chat, source_workspace=source_ws)
    # `by_title`, and it is not a tweak: every handoff todo ends in the same nine-word
    # provenance sentence, and compared over the whole text that boilerplate reads as
    # agreement — `Fix the widget` and `Ship the release` scored 0.750, so the second
    # handoff into a workspace recorded nothing at all. What two todos are ABOUT is their
    # first lines (`todos.duplicate_of`).
    dup = todos.duplicate_of(ws, text, by_title=True)
    if dup:
        # Reported and continued, not refused: a second chat on the same brief may be
        # exactly what was approved, and charter makes no judgement about the content of
        # work (CONTEXT.md).
        util.info(f"already on '{ws}'s list: {contain.one_line(dup)} — not recorded twice")
    else:
        todos.add(ws, text)
    # The brief rides INTO the launch rather than being written after it: the chat id is
    # allocated inside `cmd_launch`, so the earliest moment this file can exist is the
    # moment the id does — and that is still before the harness, whose SessionStart is
    # what reads it back (`commands_frame.Opening.brief`).
    opened = commands_frame.open_in_background(ws, caller=fid, first_message=msg,
                                               persona=args.persona or "", brief=brief)
    if not opened.ok:
        # What stays has to be TRUE, so the duplicate case says what actually happened:
        # a reader told "the todo is recorded" who then finds one older row would read the
        # sentence as a lie about the one thing a failed handoff leaves behind.
        stays = [f"the todo was already on '{ws}'s list" if dup
                 else f"the todo is recorded in '{ws}'"]
        if args.create:
            stays.append(f"the workspace '{ws}' was created")
        util.err(NOTHING_ELSE.format(why=opened.message, stays=", and ".join(stays)))
        return 1
    # No workspace name, no persona, no brief. A LOCAL workspace's name would otherwise
    # reach a committed file through the tally (plan Open question 16).
    dispatch.record_handoff(placement="here" if ws == source_ws else "elsewhere",
                            created=bool(args.create))
    # **The strip, and every line of it is after the open** — a handoff that did not open
    # returned above, so a refused one moves no tab, marks nothing and says nothing on the
    # attention row. What is on screen is only ever a chat that exists.
    #
    # The move first and the mark second, which is the order they are read in: the tab goes
    # to the front of the plane's order, and then it is marked as one nobody has looked at.
    switch.bring_to_front(ws)
    if ws != source_ws and not workspace.record_arrival(ws):
        # **A handoff into the workspace you are IN marks nothing** (spec, step 8): you are
        # looking at it, so there is no look still owed, and its chats strip already shows
        # the new tab. The tab still MOVES — the plane's order is about where work is, and
        # the workspace this chat just handed work to is where it is.
        #
        # **And a mark that could not be written is SAID rather than swallowed.** The chat
        # is open either way — nothing below is conditional on this — but the strip is the
        # only thing that was going to point at it, so a silent failure here is exactly the
        # invisible work this command exists to end, arriving through the fix for it. The
        # operator is told which workspace to go and look at, because that is the thing the
        # mark would have done for them.
        # `warn` and not `err`: the handoff DID what it was asked — the chat is open and
        # started on the brief — and a `✗` would tell the operator the command failed
        # when what failed is the pointing. `!` is the mark for "this worked and there
        # is something you need to know", which is exactly this.
        util.warn(UNMARKED.format(ws=ws, chat=opened.chat))
    # The calling chat's own attention row, which is the only screen this command has: its
    # stdout goes to a Bash tool call the operator may never read, and the chat it opened
    # is somewhere else by construction.
    commands_frame._say_on_screen(fid, f"handoff → {opened.chat} opened in workspace "
                                       f"'{ws}'", ok=True)
    # ONCE, at the end, for everything this command wrote — the todo, the tally, the order
    # and the mark — which is `plane_changed_everywhere`'s own rule about being called at
    # completion rather than per unit of work. The full fan-out and not `bump_everywhere`
    # here: a handoff made a chat (and with `--create` a workspace), so the repo tables
    # every frame on this plane draws have genuinely changed.
    notify.plane_changed_everywhere()
    print(OPENED.format(chat=opened.chat, ws=ws))
    return 0
