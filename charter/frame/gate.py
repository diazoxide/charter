"""The exit gate: one menu out of charter, wherever the frame draws — #1115.

**Leaving was four gestures and no answer to "which terminal?"** Closing the terminal
detached it; `F2 → detach` named a chat id as a tmux session and either failed or — on
every chat carrying a panel — detached *every* client of the workspace (#1097); `F2 →
charter: quit` stopped the plane; `chat: close` stopped one tab. None of them was called
*close charter*, and the one that came closest could not say whose terminal it was
closing. This module is the single place that question is asked and answered.

**Two rows, and the first is the harmless one.**

* *Close charter (keep chats running)* detaches the terminal that asked, and nothing else.
  Every chat keeps running and `charter` in the project reattaches — the same as closing
  the terminal, which is what an operator reaching for a close button means.
* *Close charter and stop all chats…* is a DOORWAY onto `leave.confirm_rows`, the same
  warning `F2 → charter: quit` opens, listing every chat with what it gets back.

**The cursor is pinned to the first row by id, refused or not** (:class:`Gate`), which is
the one place this surface departs from `palette.aim`. `aim` opens on the first row that
CAN run — the right rule everywhere else — and on a two-row menu whose first row is refused
because charter could not name the presser, that rule opens the cursor on *stop all chats*.
A surface that answers a mis-aimed Enter by stopping every harness on the plane is not a
gate, so the id wins over the rule here and the refused row says why it cannot run.

**This module never draws inside the operator's own tmux** (decision 7). Charter binds no
key there and draws no button, so there is no gesture that opens this surface — and a
`charter frame-palette --gate` typed by hand answers :data:`NOT_HERE` rather than drawing a
menu whose first row `builtin_actions._detachable` already refuses.

**The presser is the argument, not something this module goes looking for.** It arrives
expanded by tmux — from `#{client_name}` on a key bind, or from :data:`PRESSER_OPTION`,
which the click bind writes on the panel pane with `set-option -F` — and every route that
cannot produce one hands over `""`, where the detach row is listed refused with its reason.
Charter never asks tmux for its most recently active client: that answers *a* client, which
on a two-client frame is a coin toss over whose terminal to close.
"""

from __future__ import annotations

import re
from dataclasses import dataclass

from . import overlay, palette

#: The key this menu opens on, everywhere charter binds keys. It joins
#: `overlay.HATCH_KEY` and `layout.BAR_ROWS_KEY` in `instance.component_arrangement`'s
#: `bound` set, so no component and no `[frame] hotkey` can take it: a `bind -n` is
#: server-wide, tmux has no notion of a conflict, and the later line silently replaces the
#: earlier one.
#:
#: **`F10` and not a letter**, for `BAR_ROWS_KEY`'s reason — the bind is written before the
#: harness ever sees the key, so charter may only take keys a harness does not want, and
#: the function row above `F12` is the band charter already spends.
GATE_KEY = "F10"

#: What tells `charter frame-palette` that this pane is the gate rather than the palette.
#: A flag on that command and not a subcommand of its own, for `tabmenu.TAB_OPTION`'s
#: reason: the gate IS the palette's pane — the same split off the same harness, the same
#: `_close_open_overlays` sweep in front of it, the same hatch armed before it can capture
#: anything — with a different row source.
OPTION = "--gate"

#: What the header says. The noun first, because the palette's own heading is the bare word
#: `charter` and an operator who pressed a key by accident has to be able to read what this
#: surface is about before they read the rows.
LABEL = "charter · close"

#: The two rows, by id. **Not action ids**, and the colon is what makes that structural:
#: `frame/action.py` holds every action id to `component.usable_id`, so a provider cannot
#: ship an action called `gate:detach` and take this keypress. `frame/leave.py`,
#: `frame/choose.py` and `frame/tabmenu.py` use the same trick for the same reason. Neither
#: is ever drawn and neither reaches tmux.
DETACH_ID = "gate:detach"
STOP_ID = "gate:stop"

#: The rows' words, and they are the words `F2` uses too (`builtin_actions._register_detach`
#: and `leave.OPEN_QUIT` both read them from here). One spelling per row rather than one per
#: surface: an operator who has learned what *Close charter* does by pressing `F10` has
#: learned what the `F2` row does, and two sentences about one act drift the first time
#: either is edited.
#:
#: **"(keep chats running)" is in the title rather than in a note**, because the note column
#: is the one `overlay._title_width` truncates first and this clause is the whole difference
#: between the two rows.
DETACH_TITLE = "Close charter (keep chats running)"
#: The ellipsis says a confirmation follows, which is the convention every doorway in the
#: frame already keeps (`leave.OPEN_CLOSE` is the exception and is being brought into line
#: nowhere: that row is about one chat and this is about the plane).
STOP_TITLE = "Close charter and stop all chats…"

#: The pane option the `MouseDown1Pane` bind writes the clicking client into, before it
#: forwards the click. On the PANEL rather than on the window or the session, for
#: `commands_frame._PANEL_OPTION`'s reason: the handler that reads it is the panel's own
#: process, it reads its own `$TMUX_PANE`, and a window-scoped value would be whichever
#: client clicked any panel of the window last.
PRESSER_OPTION = "@charter_presser"

#: The shape of a tmux client name, which is the pty the client is attached on.
#:
#: **Held because the value reaches a `detach-client -t` argv**, and it arrives from a tmux
#: format expanded on a server charter shares with every frame on the machine. The
#: alphabet is a device path's: `/dev/` and then the characters a tty name is made of. What
#: a value outside it costs is the detach — the row is listed refused with
#: `builtin_actions.NO_PRESSER_TO_DETACH` — which is the safe direction, because the
#: alternative to refusing is aiming a detach at something charter cannot read.
#:
#: The bound is on the whole match rather than on a component, so the eighteen characters
#: `#{client_name}` — what the option would hold if the click bind ever lost its `-F` — are
#: refused rather than carried onto an argv.
CLIENT_RE = re.compile(r"/dev/[A-Za-z0-9._/-]{1,64}")

#: What a `--gate` invocation inside the operator's own tmux says instead of drawing.
#:
#: **Both clauses, because the operator needs both**: the rows exist and are one keypress
#: away on a surface they can still reach, and the thing this menu's first row would have
#: done is something their own tmux already does better than charter can. CONTEXT.md's
#: prose rule — say the rule worked and name the fix in the same breath.
NOT_HERE = ("the close menu is not drawn inside a tmux you already have — F2's rows carry "
            "it, and your own prefix key detaches")


def wanted(args) -> bool:
    """Whether this invocation is the gate.

    A bare flag, so there is nothing to contain and nothing to refuse — unlike
    `tabmenu.wanted`, whose value names a chat and is held to `chats.ID_RE` before it can
    reach a state path. `getattr` rather than `args.gate`, for that function's reason: a
    `Namespace` built by a test or by a charter that predates the flag has no such
    attribute, and an absent flag is *not the gate* rather than an error.
    """
    return bool(getattr(args, "gate", False))


def presser_of(args) -> str:
    """The client that pressed the key, or ``""`` for *charter cannot tell*.

    The `client` positional `cli.py` has accepted and ignored since #729, read now. It
    arrives as `#{client_name}` expanded by tmux in the context of whoever fired the bind
    (Step 0's G1: the file held the pressing client's name and never the other client's, on
    3.7c and at the floor), travels through this process's argv and — for the pointer route
    — through a `split-window --` argument, neither of which is shell-interpreted.

    **`""` is an ordinary answer and never an error.** A bind installed by a charter that
    predates this change carries no client at all, and a panel whose `@charter_presser`
    could not be read hands on nothing. Both reach :func:`catalogue` as *no presser*, where
    the detach row is listed refused with the sentence that says what to press instead —
    #512's rule, and the one outcome that cannot detach the wrong terminal.
    """
    client = (getattr(args, "client", None) or "").strip()
    return client if CLIENT_RE.fullmatch(client) else ""


def forward(args) -> tuple[str, ...]:
    """What `_open_palette` splices into the argv of the pane it carves.

    `tabmenu.forward`'s shape, and its empty tuple is load-bearing for the same reason one
    noun over: the pane is a SECOND process, and every value it acts on has to make the
    trip. The presser was expanded by tmux in the `run-shell` child that is running this
    function; the process inside the split is the one that builds the rows.

    **The presser rides for `F2` as well as for `F10`**, because both surfaces carry *Close
    charter* and both need to know whose terminal it is. Only the flag is conditional.

    A presser charter cannot read contributes nothing rather than an empty positional — an
    argv holding `""` would hand the pane a client by a different route, and it would stop
    the `F2` argv of a charter with no presser being byte-identical to what it was.
    """
    client = presser_of(args)
    return ((client,) if client else ()) + ((OPTION,) if wanted(args) else ())


def catalogue(fid: str, *, client: str) -> tuple:
    """The two rows. Detach first, stop second, and no third.

    **The order is the guard `leave.open_rows` states**, arrived at from the other
    direction: there the destructive row goes last among many, and here there are two, so
    last means second. What makes it hold on a two-row surface is :class:`Gate`'s cursor —
    with `palette.aim` alone, a refused first row hands the cursor to this one.

    **The detach row is listed refused rather than dropped** when charter cannot name the
    presser (#512). Dropping it would leave one row on the surface, which is *stop all
    chats* under the cursor with nothing above it — the trap the order exists to avoid,
    reached by removing a row instead of by moving one.

    *fid* is not read yet and is in the signature anyway, for `builtin_actions.build`'s
    reason: every caller says which chat it is drawing for, and the rows act on that chat
    at the moment they are chosen (:func:`chose`). Nothing here reads the plane — the
    warning does, one keypress later, which is §4f's *at the moment of deciding* and
    `leave.open_rows`' rule that a menu merely being open costs no scan.
    """
    from . import builtin_actions
    return (
        overlay.Row(id=DETACH_ID, title=DETACH_TITLE, refused=not client,
                    note="" if client else builtin_actions.NO_PRESSER_TO_DETACH),
        overlay.Row(id=STOP_ID, title=STOP_TITLE),
    )


@dataclass
class Gate(palette.Palette):
    """`palette.Palette` with one rule changed: where the cursor opens.

    **`palette.aim` is right everywhere else and wrong here**, and the difference is worth
    stating rather than overriding quietly. `aim` answers *the first row that can run*,
    which is #931's fix for a surface that opened on a refused row and spent the operator's
    Enter on nothing. This surface has two rows, the first of which can be refused —
    charter could not name the presser — and the second of which stops every harness on the
    plane. So the rule that protects every other surface would, here, open a menu with the
    cursor on the one irreversible thing in it.

    **Pinned by ID, not by index.** An index would be a second statement of the order
    :func:`catalogue` already makes, and it would follow the wrong row the moment a third
    was added.

    **And only while the row is on screen.** After a query the cursor is `aim`'s again,
    because an operator who has typed toward the stop row has asked for it — the pin is
    about where the surface OPENS, which is the whole of what was at stake.
    """

    def _refilter(self) -> None:
        super()._refilter()
        for i, row in enumerate(self.rows):
            if row.id == DETACH_ID:
                self._sel = i
                return


def opens(row, fid: str, *, live):
    """The surface *row* opens, or ``None`` when it opens none.

    `tabmenu.opens`' job, and the same two-line shape: a doorway is told apart by its id,
    and what comes back replaces the surface in the pane the operator is already looking at
    (`palette.own_the_tty`'s *then*) rather than starting a second one.

    **It is `leave`'s own confirmation, not a second one.** `F2 → charter: quit` opens
    exactly this, over exactly this plan, and the keypress that commits is the row
    `leave.confirm_rows` mints — so the two routes cannot come to disagree about what
    stopping the plane costs or about which row does it. Ended tabs are listed with what
    they get back because `leave.plan` lists them (decision 12), not because this asked.

    *live* is `commands_frame._live_chats`' tri-state, carried through rather than
    collapsed: ``None`` is *the server would not answer*, which is not *no chats*, and
    `leave.plan` is what knows the difference.

    *focus* is the chat's own workspace, which is what a QUIT records as the plane's focus
    (`commands_frame._record_the_plane`) — the gate's stop row is that quit, so it is asked
    here rather than left empty the way `tabmenu.confirm_rows` leaves it. `chat: close`
    writes no manifest; this does.
    """
    from . import leave, state
    if row.id != STOP_ID:
        return None
    return palette.Palette(
        catalogue=leave.confirm_rows(
            leave.plan(live=live, focus=state.own_workspace(fid) or ""),
            verb=leave.QUIT),
        label=leave.QUIT, mouse=True)


def chose(row, fid: str, *, client: str) -> bool:
    """Act on the row Enter landed on. Answers whether anything was started.

    **The stop DOORWAY is not here, and its absence is the confirmation** — `tabmenu.chose`'s
    rule word for word. `chose` starts work; a doorway starts none, it replaces the surface
    (:func:`opens`). A version that quit the plane on :data:`STOP_ID` would put the most
    destructive thing charter's frame can do one keypress from a key an operator may well
    have pressed by accident.

    **A refused row starts nothing**, which is not tidiness: `overlay.Surface` lets Enter
    land on any row and it is the caller that knows a refusal. Here that is the detach row
    with no presser, and running it would be charter detaching *something* to answer a
    question it could not answer.

    **The detach's own sentence is said here**, unlike an action's, because this surface has
    no registry behind it to report through: `builtin_actions._detach` answers what
    happened — it detached, or it could not prove the presser — and the pane this menu is
    drawn in is about to be killed, so the frame's own attention row is the only screen
    left (`commands_frame._say_on_screen`).
    """
    from .. import commands_frame
    from . import builtin_actions, leave
    if row.refused:
        return False
    if row.id == DETACH_ID:
        commands_frame._say_on_screen(fid, builtin_actions._detach(fid, client))
        return True
    if leave.goes_through(row, leave.QUIT):
        # `_start_leaving`, so `F10 → stop all chats` and `F2 → charter: quit` are one
        # teardown started one way: `frame-quit --chat <fid>`, detached, with the presser's
        # own chat named because this pane is about to stop existing.
        commands_frame._start_leaving(fid, leave.QUIT)
        return True
    return False


def act(row, fid: str, *, client: str) -> None:
    """Act on the row Enter landed on, and say the reason when it could not run.

    `tabmenu.act`'s shape and its argument: one function because the three halves are one
    decision, and because the surface a refusal would otherwise be drawn on is the pane
    :func:`draw` is about to kill.

    **A ``None`` row is a cancel and is answered here rather than at the call site.**
    `own_the_tty` answers ``None`` for Escape, for `overlay.HATCH_KEY` and for the pane's
    writer going away — the commonest way this surface ends, because the row under the
    cursor is deliberately the harmless one and leaving without pressing it is the ordinary
    outcome of an `F10` somebody did not mean. Keeping it here rather than in :func:`draw`
    is what makes it a line a test can turn red: `draw` needs a tty.

    **A row with nothing to say says nothing**, and the guard is not tidiness: the notice is
    a WRITE, so an empty one would blank whatever the attention row was already carrying.
    The rows that reach here having started nothing are the refused detach row, which
    carries its reason, and the warning's own per-chat rows, which carry `leave.note`, and
    its *nothing left to stop* row, which carries none.
    """
    from .. import commands_frame
    if row is None or chose(row, fid, client=client):
        return
    if row.note:
        commands_frame._say_on_screen(fid, row.note)


def draw(args) -> int:
    """Be the gate: draw the two rows, take a choice, act on it, hand the pane back.

    `tabmenu.draw`'s shape, and deliberately its shape rather than a branch inside
    `commands_frame._draw_palette`: every line of that function is about the frame's own
    catalogue, its registry and its pickers, and this surface has two rows and no registry
    at all.

    **Inside the operator's own tmux it draws nothing** (decision 7). No key is bound and no
    button is drawn there, so nothing produces this surface by a gesture — a `charter
    frame-palette --gate` typed by hand is the only way in, and what it gets is
    :data:`NOT_HERE` on the attention row. Drawing the menu there would be a first row
    `builtin_actions._detachable` refuses on every plane and a second that quits one, which
    is a surface whose only working row is the destructive one.

    **The pane is handed back from a ``finally``**, one layer up from `overlay.Surface.run`'s
    own, and the notice branch is inside it — so the operator gets their harness back
    whether the menu drew, refused or raised.

    **Always 0**, for `cmd_palette`'s reason: a non-zero return from a `run-shell` child is
    printed INTO THE HARNESS PANE and drops it into copy-mode, which is charter drawing in
    the one rectangle ADR 0018 says it never draws.
    """
    import os

    from .. import commands_frame
    from . import tabmenu, tmuxctl
    fid, socket, harness, overlay_pane = tabmenu.handback(os.environ)
    client = presser_of(args)
    try:
        if tmuxctl.is_operator_socket(socket):
            commands_frame._say_on_screen(fid, NOT_HERE)
            return 0
        surface = Gate(catalogue=catalogue(fid, client=client), label=LABEL, mouse=True)

        def _then(row):
            # **On the keypress, never on the open.** `catalogue` reads nothing; the plane
            # is scanned here, by the Enter that asked for the warning — one `list-windows`
            # per server, the same round trip `commands_frame._picker` makes for the same
            # doorway.
            #
            # **A drawer, and through the same call** (#921/#927). `_as_a_drawer` passes
            # `None` straight back, so the detach row is untouched and only the stop
            # doorway gives the window up — and the size rule stays in the one place that
            # holds it rather than being restated here.
            return commands_frame._as_a_drawer(
                opens(row, fid,
                      live=commands_frame._plane_live(
                          commands_frame._plane_servers())[0]),
                socket=socket, pane=overlay_pane)

        act(palette.own_the_tty(surface, then=_then), fid, client=client)
    finally:
        commands_frame._close_palette(socket, harness=harness,
                                      overlay_pane=overlay_pane)
    return 0
