"""Leaving: what a quit is about to stop, and what it says before it does.

§4i's rule is *detach is the default, quit is a choice*, and §4f's is that the warning
**names, per chat, what will and will not come back — at the moment the operator is
deciding, not after.** This module is that sentence, per chat, as data.

**Nothing here touches tmux and nothing here kills anything** — `frame/chats.py`'s rule,
kept for a sharper reason: this is what the confirmation surface draws, and a module that
shelled out per row would put ~one round trip per chat in front of a palette the operator
is waiting on. Liveness arrives as an ARGUMENT (`live`), asked once by
`commands_frame`, which is where every other tmux question in the frame is asked.

**Quit is plane-scoped, and that is a correction to the brief this shipped under rather
than an interpretation of it.** §4i asks for *"a warning naming every workspace and chat
that will stop"* and the operator's own words are *"all harness sessions will be closed"*.
§3.3 then bounds it from the other side: one tmux server serves every plane on the machine
and session names carry no plane, so the blast radius has to be filtered, and **the filter
can only come from disk.** So the set is exactly *this plane's chat directories*, and the
kill is per WINDOW — never `kill-server`, and never `kill-session` on a workspace name,
which in another plane is another plane's session (`default` is a name every plane has).

The consequence for `inflight` is worth stating here rather than discovering later: because
the quit is plane-scoped and `inflight` is plane-scoped, the prune is exactly co-extensive
with what was killed. A genuinely per-FRAME quit could not prune at all — `inflight`
records carry no fid, no chat and no workspace (§2.15) — so the two facts are the same
fact, not a coincidence.
"""

from __future__ import annotations

import os
from typing import NamedTuple

from . import chats, reopen, state

#: What the operator is told when a quit would stop nothing.
NOTHING_OPEN = "no chats are open on this plane — nothing to quit"

#: How the pieces of one chat's note are joined. One line per chat, whatever charter has
#: to say about it, because the confirmation surface draws one row per chat and
#: `overlay.Row` has exactly one note.
JOIN = " · "

#: The two things this module offers, as the words their rows and their commands are
#: spelled with. One constant per verb rather than a literal at each of the four places
#: (the doorway id, the confirm id, the command name, the sentence) so a rename cannot
#: half-land.
QUIT, CLOSE = "quit", "close"


class Doomed(NamedTuple):
    """One chat a quit is about to stop, with everything the warning needs about it.

    Assembled once and handed to both the warning and the teardown, so what the operator
    was told and what charter then did cannot come from two readings of the plane. That is
    `chats.Chat`'s reason and `_draw_palette`'s: an offer charter cannot honour is worse
    than no offer.
    """

    #: The chat id — the frame directory's name, and `@charter_chat` on its window.
    chat: str
    #: The workspace it says it is in (`state.own_workspace`, the membership question), or
    #: ``""`` when it says nothing. A chat that says nothing is still stopped and still
    #: recorded; what it loses is a session to be rebuilt into, which :attr:`homeless` is.
    workspace: str
    #: The persona resolved under this chat's own id, or ``""``.
    persona: str
    #: `harness.base.name`, or ``""`` when charter has no identity record for the chat.
    harness: str
    #: The recorded cwd, or ``""``.
    cwd: str
    #: The harness's own session id, or ``""`` when there is none to resume from.
    resume: str
    #: Which tmux server the chat is recorded on — charter's own, or an operator's.
    server: str
    #: Whether tmux still reports a window for this chat. ``None`` when charter could not
    #: ask its server at all, which is a third answer and not a "no": a quit that read a
    #: wedged server as "nothing is live" would record nothing and kill nothing while
    #: telling the operator it had done both.
    live: bool | None
    #: Whether this chat was the one on screen in its session.
    active: bool
    #: The exit code the pane hook recorded, or ``None``. Per §2.17 the file means
    #: precisely *"this harness ended on its own"* — `kill-pane`, `kill-window` and
    #: `kill-session` write nothing — so ``None`` is the ordinary answer for a running
    #: chat AND for one the machine took down, and the two are not distinguishable.
    exit_code: int | None
    #: Whether the operator closed this chat (`state.was_closed`). A closed chat is not
    #: recorded and not brought back; it is listed by nothing and appears here only so
    #: :func:`plan` has one place that drops it.
    closed: bool
    #: Whether the workspace this chat belongs to still has a directory on disk. §4j
    #: forbids re-homing a chat, so a missing workspace is *reported*, never repaired.
    homeless: bool
    #: Whether the recorded cwd is still a directory.
    cwd_gone: bool


class Plan(NamedTuple):
    """Every chat a quit would stop, grouped as the manifest will hold it."""

    chats: tuple[Doomed, ...]
    #: The workspace the quit was invoked from — where a reopen puts the operator back.
    focus: str

    def workspaces(self) -> tuple[str, ...]:
        """The workspaces this plan touches, in first-appearance order.

        Order off the chat list rather than sorted, so the summary sentence names them in
        the order the rows below it are drawn — a list an operator reads twice in two
        orders is a list they have to re-read.
        """
        seen: list[str] = []
        for c in self.chats:
            if c.workspace and c.workspace not in seen:
                seen.append(c.workspace)
        return tuple(seen)

    def resumable(self) -> tuple[Doomed, ...]:
        """The chats whose conversation charter can actually ask for back."""
        return tuple(c for c in self.chats if c.resume)


def plan(*, live, focus: str, only: str = "") -> Plan:
    """Every chat on THIS PLANE that a quit would stop, read off disk.

    *live* is the set of chat ids the frame's tmux server reports, or ``None`` when it
    would not answer — `commands_frame._live_chats`' own tri-state, carried through rather
    than collapsed, for the reason that function documents at length: a server that answers
    "no windows" because it was wedged is not a server with nothing on it, and treating the
    two the same would make a quit silently record nothing.

    *only* narrows the plan to one chat, which is the whole of what `chat: close` is —
    **same plan, same warning, same teardown, one target.** A second enumeration for the
    single-chat case would be a second answer to "what does stopping this cost".

    **A chat the operator CLOSED is not here**, and that is the one filter with a direction
    to it. Everything else this includes on the restoring side: a chat with no `exit` file
    is recorded because *nothing means we do not know it stopped* (`state.was_closed`'s own
    note), a chat whose workspace has gone is recorded and flagged rather than dropped
    (§4j: re-homing is forbidden and #789 removed the last of it), and a chat with an
    unreadable identity is recorded under its id alone.

    Order is `chats.of_workspace`'s — ordinal within a workspace — with workspaces in the
    order their first chat appears, so the rows the operator reads are the tabs they had.
    """
    from .. import persona as p_mod
    from .. import workspace as ws_mod
    out: list[Doomed] = []
    for fid in plane_chats():
        if only and fid != only:
            continue
        if state.was_closed(fid):
            continue
        ws = state.own_workspace(fid) or ""
        ident = state.identity(fid)
        cwd = state.chat_cwd(fid) or ""
        out.append(Doomed(
            chat=fid,
            workspace=ws,
            persona=p_mod.for_session(fid) or "",
            harness=ident.get("CHARTER_HARNESS", "").strip(),
            cwd=cwd,
            resume=state.kept_harness_session(fid) or "",
            server=state.frame_server(fid) or "",
            live=None if live is None else (fid in live),
            active=False,
            exit_code=state.exit_code(fid),
            closed=False,
            homeless=bool(ws) and not ws_mod.workspace_dir(ws).is_dir(),
            cwd_gone=bool(cwd) and not os.path.isdir(cwd),
        ))
    return Plan(chats=tuple(out), focus=focus)


def plane_chats() -> list[str]:
    """Every chat directory on this plane, in tab order, whatever workspace it says.

    **`chats.of_workspace` cannot answer this**, and the difference is the point: that
    function asks "which chats say they are in *this* workspace", so a chat whose
    `workspace` record was lost — or whose workspace has been deleted — belongs to no
    workspace and would be invisible to every one of its calls. A quit that skipped such a
    chat would kill it (its window is on this plane's server) and record nothing about it,
    which is the one outcome this whole design exists to prevent.

    So the scan is the frame root itself, with `chats.is_chat` as the only filter — Stage
    5a's version discriminator, asked rather than re-derived — and `is_dir()` beside it for
    `chats.of_workspace`'s own measured reason (#733): the frame root also holds the
    manifest, the transcripts, and whatever a half-finished `os.replace` left behind, and a
    loose FILE called `api.2` there is a name that would otherwise reach a tmux target.

    Sorted by `chats._order`, so the ordering is the bar's rather than the filesystem's,
    and the two cannot drift: it is the same function `of_workspace` sorts with.
    """
    try:
        names = [e.name for e in os.scandir(state._root()) if e.is_dir()]
    except OSError:
        return []
    inside = sorted((n for n in names if chats.is_chat(n)), key=chats._order)
    # Grouped by workspace WITHOUT losing the ordinal order inside a group, which is what
    # a second `sorted` on a stable sort buys: `sorted` is guaranteed stable, so this
    # re-orders the groups and leaves each group's chats exactly as `chats._order` left
    # them. A chat that says nothing about its workspace sorts last, deliberately — it is
    # the migration and truncation case (`plane_chats`' whole reason for existing), and a
    # nameless group at the end reads as the leftovers it is rather than as a workspace
    # called "".
    return sorted(inside, key=_group)


def _group(fid: str) -> tuple[int, str]:
    """Which workspace group *fid* sorts into, asked once.

    Its own function rather than an inline lambda because `state.own_workspace` reads three
    files per call (the identity record, the per-session pointer, the workspace record —
    `chats.of_workspace` measures it), and a lambda that tested it and then returned it
    would read all three twice for every chat on the plane.
    """
    ws = state.own_workspace(fid)
    return (0, ws) if ws else (1, "")


def stopping(p: Plan) -> tuple[Doomed, ...]:
    """The chats a quit will actually try to stop.

    Everything in the plan whose server has not told charter the chat is gone. A chat tmux
    reports as absent is already stopped, and a chat charter could not ASK about
    (``live is None``) is attempted anyway — the kill is a `kill-window` on a window tmux
    itself named, so an attempt against a chat that is no longer there costs one rc that
    charter ignores, while skipping one that IS there leaves a harness running behind a
    manifest that says it was stopped.
    """
    return tuple(c for c in p.chats if c.live is not False)


def summary(p: Plan, *, verb: str = QUIT) -> str:
    """The one line above the rows: what stops, and how much of it comes back.

    Counted from the plan rather than composed by the caller, so the number in the
    confirmation's heading, the rows under it and the line `commands_frame` prints on stderr
    are one reading of the plane.

    *verb* decides which sentence, and it has a default because `confirm_rows` asks for
    quit's by name while the stderr warning asks for whichever verb it is running — a
    `chat: close` that printed *"quit — stop 1 chat"* above its own row would be describing
    the wrong command, which is exactly the confusion `_close_summary` exists to prevent one
    surface over.
    """
    doomed = stopping(p)
    if not doomed:
        return NOTHING_OPEN
    if verb == CLOSE:
        return _close_summary(doomed)
    wss = p.workspaces()
    back = len([c for c in doomed if c.resume])
    return (f"quit — stop {_count(len(doomed), 'chat')} in "
            f"{_count(len(wss), 'workspace')}; "
            f"{back} of {len(doomed)} can resume the conversation")


def _count(n: int, noun: str) -> str:
    """``1 chat`` / ``3 chats``. Spelled once, because the summary says it twice and a
    second spelling is a second plural rule."""
    return f"{n} {noun}" if n == 1 else f"{n} {noun}s"


#: What a chat with a session id to resume from is promised. The verb is deliberately
#: about the CONVERSATION and not about the pane: §4f is explicit that
#: `claude --resume` re-renders the conversation and restores no scrollback, and a note
#: saying "comes back" full stop would be promising the screen.
RESUMES = "conversation resumes"

#: What a chat charter cannot resume is told, and it names the reason rather than the
#: symptom. §2.8: the context gauge and the session id have exactly one writer, Claude
#: Code's `statusLine` hook, so no other harness has an id to ask with. An operator reading
#: "reopens empty" alone would reasonably file a bug.
NO_RESUME_HARNESS = "reopens empty — {harness} records no session id to resume from"

#: The same outcome for the harness that CAN resume, when this particular chat has no id
#: yet — a chat whose harness had not taken a turn, or one already running when the charter
#: that keeps the id was installed (`state.kept_harness_session`'s own migration case).
NO_RESUME_YET = "reopens empty — no session id recorded for this chat yet"

#: Said when charter does not know which harness a chat was, which is the migration case
#: and an ordinary one.
NO_RESUME_UNKNOWN = "reopens empty — charter has no record of this chat's harness"


def note(c: Doomed) -> str:
    """The one line that says what this chat loses and what it keeps.

    **Resume first, because it is the thing the operator is deciding about.** The rest are
    qualifications, and each is present only when it is TRUE of this chat — a note that
    listed every possible caveat would make the one that applies impossible to find.

    Every clause is a fact charter has already read off disk, and none of them is a
    prediction it cannot check: whether an id exists, whether the workspace directory is
    there, whether the recorded directory is there, and whether a transcript was captured
    is decided by the caller and appended, because only the quit knows if its capture
    landed.
    """
    if not c.workspace:
        # **The one chat this design cannot bring back, and the note says so instead of
        # promising anything.** A reopen rebuilds a tmux session per workspace and hands the
        # launcher a `--workspace`; a chat that says nothing about its workspace has nothing
        # to be rebuilt into, and inventing one would be the re-homing §4j forbids arriving
        # as a convenience. So `reopen.read` refuses the record and this row promises no
        # resume, whatever id the chat happens to hold. It is the migration and truncation
        # case (`plane_chats`' own reason for scanning the frame root) rather than a state
        # charter mints.
        return JOIN.join([NOT_REOPENED] + _ended(c))
    parts = [_resume_clause(c)]
    if c.homeless:
        parts.append(f"workspace '{c.workspace}' is gone — reopens saying so")
    if c.cwd_gone:
        parts.append(f"its directory {c.cwd} is gone — reopens in the plane root")
    elif not c.cwd:
        parts.append("no directory recorded — reopens in the plane root")
    return JOIN.join(parts + _ended(c))


#: What a chat charter cannot place is told. Its own constant because two callers need the
#: same words — the note, and `commands_frame._record_the_plane`, which is what actually
#: leaves it out of the manifest.
NOT_REOPENED = "charter has no record of its workspace — it cannot be reopened"


def _ended(c: Doomed) -> list[str]:
    """The clause a chat that already finished carries, or nothing.

    Its own function because both branches of :func:`note` end with it and a chat that
    cannot be reopened is still a chat whose exit code is worth reading — §2.17's file means
    precisely *"this harness ended on its own"*, which is the one ending charter can
    distinguish and the one an operator most often wants to know about.
    """
    return [] if c.exit_code is None else [f"already ended on its own ({c.exit_code})"]


def _resume_clause(c: Doomed) -> str:
    """Which of the four resume sentences this chat gets.

    Split out so the four are a table a test can walk rather than a chain inside a longer
    function, and so the ORDER of the two "cannot" cases is explicit: an unknown harness is
    asked about before a missing id, because "charter does not know what this was" is a
    different thing to tell an operator than "this one has not taken a turn yet", and the
    second would be a guess dressed as a fact.
    """
    if c.resume:
        return RESUMES
    if not c.harness:
        return NO_RESUME_UNKNOWN
    if resumable_harness(c.harness):
        return NO_RESUME_YET
    return NO_RESUME_HARNESS.format(harness=c.harness)


def resumable_harness(name: str) -> bool:
    """Whether *name* is a harness charter can ask for a conversation back.

    **Claude Code alone, and asked of the registry rather than spelled here** (§2.8 and
    §4e): `record_harness_session` has exactly one caller, Claude Code's `statusLine` hook,
    so it is the only harness that has ever written an id. A literal ``"claude-code"`` in
    this module would be a second place that knows which harness resumes, and the day a
    second one starts writing an id the two would disagree.

    Deliberately NOT a member on `Harness`. Phase 5's Task 9 Step 4 refuses one on
    `harness/base.py`'s own bar — *"a fifth member needs the same kind of argument, not just
    a use"* — and the bar is not met: `launch_argv` is `[self.binary, *extra]` with no
    subclass override anywhere in the registry, so the pass-through **is** the seam, and
    what decides whether to use it is whether charter has an id, which is this question.
    """
    from ..harness import claude_code
    return name == claude_code.NAME


def title(c: Doomed) -> str:
    """The left-hand side of one chat's row: its id, and what it was running.

    The id first because it is what the operator has been looking at on the `chats` bar all
    day, and the harness after it because two chats of one workspace are told apart by
    nothing else. Display text with an open alphabet on the harness half — it is a
    harness's own display name — contained by `overlay.Surface.render` where every other
    title is, and never here (`chats.py`'s rule, and the masked-containment finding behind
    it).
    """
    return f"{c.chat} · {c.harness}" if c.harness else c.chat


def transcript_of(c: Doomed) -> str:
    """The file name a capture of *c* would be written under, or ``""``.

    Through `reopen.transcript_path` so the name is minted in exactly one place, and
    answered as a bare name rather than a path because that is what the manifest carries:
    a path recorded today and read on a plane whose `STATE_DIR` has moved would name a
    directory that is not this plane's.
    """
    p = reopen.transcript_path(c.chat)
    return p.name if p is not None else ""


#: The row that OPENS the confirmation, the row that goes through with it, and one row per
#: chat in between. All three carry a `:`, which is `frame/choose.py`'s trick and the whole
#: of why these cannot collide with an action: `frame/action.py` holds every action id to
#: `component.usable_id` — lower-case letters, digits, underscores and at most one dot —
#: so a provider cannot ship an action called `leave:quit:go` and take the keypress.
#: None of them is ever drawn and none reaches tmux.
OPEN_ID = "leave:{}"
GO_ID = "leave:{}:go"
CHAT_ID = "leave:{}:c{}"

#: The doorway rows' titles. `charter:` and `chat:` because that is the noun each one is
#: about — quit stops the plane, close stops one tab — and the palette is read by an
#: operator scanning left edges.
OPEN_QUIT = "charter: quit — stop every harness on this plane"
OPEN_CLOSE = "chat: close — stop this chat and do not bring it back"


def open_rows(fid: str) -> tuple:
    """The two doorway rows the palette carries, and neither one costs a scan.

    **A doorway and not an action**, for `choose.open_rows`' reason exactly: an `Action`'s
    contract is *fire-and-report*, and opening a confirmation starts nothing — it replaces
    the surface in the pane the operator is already looking at. An action whose ``run``
    merely drew something would pass every test in that contract and describe nothing that
    happens.

    **They are the LAST rows in the catalogue**, which `commands_frame._draw_palette`
    arranges by appending them, and that placement is a guard rather than a taste: the
    palette's cursor starts on the first row that can run, and a destructive row at the top
    would be one `F2 Enter` away. Charter's own harmless rows keep the top of the list.

    No plan is built here. The confirmation is where the per-chat warning is drawn (§4f:
    *at the moment the operator is deciding*), and the operator is not deciding while the
    palette is merely open — a scan of the frame root and three files per chat on every
    `F2` would be `choose.open_rows`' own measured mistake, made again.

    *fid* decides one thing and only one: **`chat: close` needs to know which chat it is
    about, and quit does not.** A palette that could not resolve its own chat — a frame
    launched by a charter that predates `@charter_chat`, so the shared bind's `--chat`
    arrives empty and `$CHARTER_SESSION_ID` is another chat's — has no target to close, and a
    confirmation opened over "every chat on the plane" would be quit wearing close's title.
    So the row is listed with its reason, which is #512's rule (an option you cannot see is
    one you cannot ask about) and is exactly how a pinned `workspace:` doorway already
    behaves.
    """
    from . import overlay
    return (overlay.Row(id=OPEN_ID.format(QUIT), title=OPEN_QUIT),
            overlay.Row(id=OPEN_ID.format(CLOSE), title=OPEN_CLOSE,
                        note="" if fid else NO_CHAT_HERE, refused=not fid))


#: What the close row says on a frame whose own chat charter cannot resolve. Its own sentence
#: rather than `builtin_actions.NO_LAYOUT`'s: that one is about a lost pane record and sends
#: the operator to relaunch, and this is about an id — the same gap `cmd_palette` and
#: `cmd_chat` already answer, said where the row is drawn.
NO_CHAT_HERE = ("charter cannot tell which chat this palette was opened in, so it has no "
                "chat to close — `charter frame-close <chat>` names one")


def verb_of(row) -> str | None:
    """Which confirmation *row* opens, or ``None`` when it opens none.

    Matched against the two ids this module mints rather than by splitting on `:`, so a row
    id that merely contains a colon cannot be read as an instruction — `choose.noun_of`'s
    rule, and the reason it is a function rather than a `startswith`.
    """
    for verb in (QUIT, CLOSE):
        if row.id == OPEN_ID.format(verb):
            return verb
    return None


def goes_through(row, verb: str) -> bool:
    """Whether *row* is *verb*'s confirming row — the one Enter acts on."""
    return row.id == GO_ID.format(verb)


def is_row(row) -> bool:
    """Whether *row* belongs to a confirmation surface at all.

    What tells `commands_frame._draw_palette` that a chosen row is one of these and must
    NOT be handed to `ActionRegistry.invoke` as an action id. Asked of the two ids that do
    something and of the per-chat prefix, rather than of any id containing a colon: the
    palette also draws `frame/choose.py`'s rows, and one module claiming every colon would
    swallow the other's.
    """
    if verb_of(row) is not None:
        return True
    return any(row.id == GO_ID.format(v) or row.id.startswith(CHAT_ID.format(v, ""))
               for v in (QUIT, CLOSE))


def confirm_rows(p: Plan, *, verb: str) -> tuple:
    """The confirmation surface: the row that goes through, then one row per chat.

    **The per-chat rows are `refused=True`, and that is what makes them a warning rather
    than a menu.** `overlay.Row.refused` means "this row cannot run right now" (#732) —
    which is exactly true of them: they are the sentence §4f asks for, and pressing one
    does nothing.

    **The confirming row is FIRST, and an earlier draft had it last.** Under the list it is
    about reads better and was measured to be wrong: `frame/palette.narrow` puts the cursor
    on the first row when nothing has been typed, refused or not, so the surface opened with
    `> alpha.1 · claude-code` selected and Enter bound to nothing at all. *"A palette row
    that visibly does nothing reads as broken and costs the operator a whole `F2` to find
    out"* is `builtin_actions._register_selection`'s own finding, and it applies here more
    than anywhere: the one surface where the operator has just asked a question and is
    waiting for the keypress that answers it.

    So the shape is the shape of every confirmation an operator has ever used: the thing you
    press at the top, what it will do underneath it, and the whole list on screen before the
    first keypress that commits anything. §4f's requirement is that the warning is drawn *at
    the moment of deciding*, and it is — the doorway's Enter draws this surface and nothing
    else.

    A plan with nothing to stop gets one refused row saying so and **no confirming row at
    all**, so there is no keypress that quietly succeeds at nothing.
    """
    from . import overlay
    doomed = stopping(p)
    if not doomed:
        return (overlay.Row(id=CHAT_ID.format(verb, 0), title=NOTHING_OPEN,
                            refused=True),)
    go = overlay.Row(id=GO_ID.format(verb), title=summary(p, verb=verb))
    return (go, *(overlay.Row(id=CHAT_ID.format(verb, i), title=title(c), note=note(c),
                              refused=True, mark=c.active)
                  for i, c in enumerate(doomed)))


def _close_summary(doomed) -> str:
    """The confirming row's title for `chat: close`, and what it says that quit does not.

    Close is the one verb that is deliberately NOT recorded: a chat charter brought back
    after the operator closed it would make closing meaningless, and the accepted cost of
    "a missing exit record means it was open" is exactly this row's existence
    (`state.was_closed`). So the title says *forget*, where quit's says *resume*.
    """
    return f"close {doomed[0].chat} — stop it and forget it; it will not come back"
