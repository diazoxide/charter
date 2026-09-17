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
