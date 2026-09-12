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

Then the writes: the workspace, the todo, the chat, the private brief, the tally, the line.
"""

from __future__ import annotations

import os
import sys

from . import (commands_frame, config, contain, dispatch, handoff, harness, persona, todos,
               util, workspace)
from .frame import chats, state, switch, tmuxctl

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
NOTHING_ELSE = (
    "charter handoff: {why}\n  What stays: {stays}. Nothing else was written.")
OPENED = "charter handoff: opened chat {chat} in workspace '{ws}', started on the brief"


def _printed_command(*, ws: str, msg: str, create: bool, vision: str | None,
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
        extra=extra if named else [msg], create=create, vision=vision,
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
    source_chat = fid if in_a_chat else (fid or handoff.NO_CHAT)
    source_ws = state.workspace_for(fid) if in_a_chat else workspace.resolve()
    msg = handoff.first_message(handoff.stamp(source_chat, source_ws), brief)
    if not in_a_chat or tmuxctl.is_operator_socket(state.frame_server(fid)
                                                   or commands_frame.SOCKET,
                                                   own=commands_frame.SOCKET):
        command, named = _printed_command(ws=ws, msg=msg, create=bool(args.create),
                                          vision=args.vision, persona_name=args.persona)
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
    dup = todos.duplicate_of(ws, text)
    if dup:
        # Reported and continued, not refused: a second chat on the same brief may be
        # exactly what was approved, and charter makes no judgement about the content of
        # work (CONTEXT.md).
        util.info(f"already on '{ws}'s list: {contain.one_line(dup)} — not recorded twice")
    else:
        todos.add(ws, text)
    opened = commands_frame.open_in_background(ws, caller=fid, first_message=msg,
                                               persona=args.persona or "")
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
    # Private state, never committed and never listed by `clear_shape`: the brief is what
    # the chat was opened to do, which is the same kind of durable fact as its workspace.
    state.record_brief(opened.chat, brief)
    # No workspace name, no persona, no brief. A LOCAL workspace's name would otherwise
    # reach a committed file through the tally (plan Open question 16).
    dispatch.record_handoff(placement="here" if ws == source_ws else "elsewhere",
                            created=bool(args.create))
    print(OPENED.format(chat=opened.chat, ws=ws))
    return 0
