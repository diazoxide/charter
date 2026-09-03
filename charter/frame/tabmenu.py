"""The tab menu: right-click a chat tab, and act on THAT tab — #846.

**This is `frame/palette.py` with a different row source, and that is the whole design.**
Not `display-menu`, and not a fourth surface: :func:`draw` builds an `overlay.Surface`
through `palette.Palette` and hands it `palette.own_the_tty`, so everything the overlay
already decided holds here unchanged — full-pane rather than `display-popup`, modal, one
key (`overlay.HATCH_KEY`) that always leaves, a scrolling window, containment before width
arithmetic, and a doorway that replaces the surface in the pane the operator is already
looking at.

**`display-menu` was measured and lost, twice.** `frame/menu.py` was deleted rather than
deprecated when the palette arrived (`overlay.py`'s module docstring, `palette.py`'s), and
nothing about a right-click makes it a better answer than it was:

* at `tmuxctl.FLOOR` (3.2) it has **no styling flags at all** — `-s`, `-S`, `-H` and `-b`
  each answer rc 1 — so a menu drawn there cannot mark a row, dim a refused one or say why
  one is refused, which is #512's rule broken by the surface rather than by charter;
* every item needs a **key as well as a command**, which is the digit shortcut that ran out
  at nine coming back;
* it is **per-client**, so the two-client frame `commands_frame._say_on_screen` already
  reasons about gets one menu and one operator wondering why nothing happened;
* it is **totally modal at the server**: every panel process on the window stops being
  drawn while it is up, which is the frame going blind for as long as the operator reads;
* and it has a hard `client_height − 2` item cap past which it **draws nothing and exits
  0** — a surface that silently succeeds at nothing, which is exactly the failure this
  repository spends its guards on.

---

**What a right-click resolves to is a TAB, and the resolution is free.** `slots._Tabs`
already maps every cell of the strip to the name drawn in it; :meth:`slots._Tabs.tab_at`
is that map read without `switch_to`'s one subtraction — the tab you are ON answers itself
here, because closing the chat you are in is the ordinary case of closing a chat, where
switching to the chat you are in is 41 tmux calls to arrive where you already were.

**Exactly two rows have a tab to sit on**, which is a catalogue of the whole palette by
scope rather than a choice: `chat: previous transcript` and `chat: close` are about one
chat; detach, both next/previous pairs, the densities, the chromes, the todo row and the
regather are about the FRAME; the pickers and `charter: quit` are about the PLANE. There is
no `stop` distinct from close and no rename anywhere in the frame. So this module removes
nothing from `F2` — the palette keeps every row it had, and this is a second, faster route
to two of them, which matters because **a right-click menu is invisible until you try it**.

**Close goes LAST and stays confirmed.** `frame/leave.open_rows` puts the destructive rows
at the bottom of the palette so that a destructive row is never one `F2 Enter` away, and a
gesture that reaches close in one pointer press makes that reasoning stronger rather than
weaker. The doorway starts nothing: it opens `leave.confirm_rows` over
`leave.plan(only=<tab>)` — the same plan, the same warning and the same teardown as
`F2 → chat: close`, with one target — and the keypress that commits is the row that
module mints, so the two routes cannot come to disagree about which one that is.

**And if the terminal never sends button 2, nothing happens.** The proof that tmux routes
it to a reporting pane was made by injecting SGR bytes into a real client over a real pty,
which bypasses the terminal EMULATOR; whether a given emulator forwards button 2 to a
mouse-reporting application or serves its own context menu is emulator-dependent and
configurable (iTerm2 3.6.11 ships `"Button,1,1,," -> kContextMenuPointerAction` beside a
profile with `Mouse Reporting` on, and the precedence between them is not determinable
from the plist). So every path here degrades to *never fires* and none to *fires wrongly*
or to a refusal: an absent or unspellable `--tab` is not an error, it is
:func:`wanted` answering `""` and the ordinary palette opening instead.
"""

from __future__ import annotations

import os

from .. import util
from . import chats, leave, overlay, palette

#: The option that carries which tab a menu is about, from the panel that decoded the
#: press, through the `frame-palette` process the panel spawns, to the process inside the
#: pane it splits.
#:
#: **On `frame-palette` rather than a command of its own**, because a tab menu IS the
#: palette: the same pane is carved off the same harness by the same argv, the same
#: `_close_open_overlays` sweep runs in front of it so a right-click while `F2` is up
#: replaces it rather than leaving an invisible pane holding a live process (#739), and
#: the same hatch is armed before the surface can capture anything. A second subcommand
#: would be a second copy of all of that, drifting.
TAB_OPTION = "--tab"

#: The two rows, by id. **Not action ids**, and the colon is what makes that structural:
#: `frame/action.py` holds every action id to `component.usable_id` — lower-case letters,
#: digits, underscores, at most one dot — so a provider cannot ship an action called
#: `tab:close` and take this keypress. `frame/choose.py` and `frame/leave.py` use the same
#: trick for the same reason. Neither is ever drawn.
TRANSCRIPT_ID = "tab:transcript"
CLOSE_ID = "tab:close"


def label(target: str) -> str:
    """What the menu's header says before the operator types.

    The chat's own id, because that is what they right-clicked and because this surface
    may be about a tab the frame is NOT on — `palette.HEADING`'s bare `charter` would
    leave a `chat: close` row over an unnamed target, which is the palette's row wearing a
    menu's clothes.
    """
    return f"chat {target}"


def transcript_title(target: str) -> str:
    """The transcript row's title. Named, for :func:`label`'s reason."""
    return f"chat: previous transcript — {target}"


def close_title(target: str) -> str:
    """The close doorway's title.

    `leave.OPEN_CLOSE` says *this chat*, which is true of a palette opened IN the chat it
    is about and is exactly what cannot be said here: the tab under the pointer is very
    often not the tab the frame is on. So the id is spelled out, and the rest of the
    sentence is `leave.OPEN_CLOSE`'s promise unchanged — the operator is being told the
    same thing by both routes.
    """
    return f"chat: close {target} — stop it and do not bring it back"


def wanted(args) -> str:
    """Which tab this invocation is about, or ``""`` for *this is not a tab menu*.

    **An unspellable value is not an error, it is no menu**, and that is the whole
    degradation promise of this feature stated at its entry point. This value arrives
    from a panel process that read it out of a click map, travels through an argv and a
    tmux `split-window`, and ends up in `state.frame_dir` and a `charter frame-close`
    positional — so it is held to `chats.ID_RE`, the alphabet a chat id reaches tmux
    under, exactly as `commands_frame.cmd_close` holds its own positional. What a value
    outside it produces is the ordinary palette, which is what `F2` produces, which is
    never wrong and never says anything an operator did not ask for.

    ``fullmatch`` alone, with no truthiness test in front of it: `chats.ID_RE` is `+`, so
    it already refuses the empty string, and a second guard nothing could turn red is the
    line this repository deletes (`commands_frame._pressers_chat` says so about the
    identical shape).

    **And no `strip`, unlike `_pressers_chat` and `cmd_close`, which read the same kind of
    value on a different trip.** Those two take `#{@charter_chat}` expanded by tmux INTO A
    SHELL-QUOTED `run-shell` string, where padding is a thing a format can produce. This
    one cannot be padded and the deletion sweep is what asked the question. Measured: the
    only producer is `slots._Tabs.tab_at`, whose names come off `chats.of_workspace`,
    which reaches them through `chats.is_chat` — *"the id is held to `ID_RE` here, because
    this is where a name off `os.scandir` enters charter's vocabulary"*. A directory
    literally named ``api.2 `` beside `api.1` and `api.2`, with this workspace recorded on
    it, is excluded from the roster, is drawn on no bar and is answered by no cell. From
    there the value travels in an argv list (`util.self_relaunch_argv`, `builtin_actions
    ._spawn`'s `Popen`) and as separate `split-window --` arguments, neither of which is
    shell-interpreted.

    So a `strip` here would be a repair for damage that cannot arrive — and worse than
    idle: if a padded name ever did reach it, stripping would silently retarget the menu
    at a DIFFERENT chat, which is #838's defect exactly. Refusing is the answer that
    cannot act on the wrong one.
    """
    tab = getattr(args, "tab", None) or ""
    return tab if chats.ID_RE.fullmatch(tab) else ""


def forward(args) -> tuple[str, ...]:
    """:data:`TAB_OPTION` and its value for a tab menu, and nothing at all for `F2`.

    **The empty tuple is load-bearing.** `commands_frame._open_palette` splices this into
    the argv the overlay pane runs, so a version that emitted `("--tab", "")` would make
    every `F2` a tab menu about no chat — and :func:`wanted` would answer `""` for it one
    process later, which is a surface that draws the ordinary palette after paying for an
    option it could not use. Emitting nothing keeps the `F2` argv byte-identical to what
    it was before this module existed.
    """
    tab = wanted(args)
    return (TAB_OPTION, tab) if tab else ()


def catalogue(target: str) -> tuple[overlay.Row, ...]:
    """The menu's rows: the transcript, then close.

    **Two rows, and the order is the guard.** `frame/leave.open_rows` puts the destructive
    row last because a palette's cursor starts on the first row that can run, so a
    destructive row at the top is one Enter from a surface that has only just appeared.
    This menu appears under the pointer rather than under a keypress, so it arrives with
    even less warning — the placement is inherited rather than re-argued, and the row is
    a DOORWAY either way (:func:`chose`).

    **A tab with no capture is listed with its reason and not dropped** — #512's rule,
    and `builtin_actions.NO_TRANSCRIPT`'s own sentence rather than a second one: a chat
    that has never been quit has no transcript, which is the ordinary state of a chat
    running normally, and an operator cannot ask about an option they cannot see.

    No plan is built here and nothing is scanned. `frame/leave.open_rows` makes the same
    promise for the same reason: the operator is not deciding while a menu is merely open,
    and the warning belongs at the moment they are (§4f).
    """
    from . import builtin_actions
    has = builtin_actions._has_transcript(target)
    return (
        overlay.Row(id=TRANSCRIPT_ID, title=transcript_title(target),
                    note="" if has else builtin_actions.NO_TRANSCRIPT, refused=not has),
        overlay.Row(id=CLOSE_ID, title=close_title(target)),
    )


def confirm_rows(target: str, *, live) -> tuple[overlay.Row, ...]:
    """What the close doorway opens: `leave`'s own warning, narrowed to *target*.

    **The same plan, the same rows and the same confirming id as `F2 → chat: close`.**
    `leave.plan(only=…)` is the seam that already existed for exactly this, and a second
    enumeration for this route would be a second answer to *what does stopping this cost*
    — two sentences about one chat that drift the first time either is edited.

    *live* is `commands_frame._live_chats`' tri-state, carried through rather than
    collapsed: ``None`` is *the server would not answer*, which is not the same as *no
    chats*, and `leave.plan` is what knows the difference.

    ``focus=""``, and it is not a fallback. `Plan.focus` is read by the manifest a QUIT
    writes (`commands_frame._record_the_plane`); a confirmation draws rows and writes no
    manifest, and close is the one verb that is deliberately never recorded at all
    (`leave._close_summary`). A value here would be one nothing on this path can observe,
    which is the shape the deletion sweep reports — `commands_frame._picker` already
    removed the `or ""` beside its own for the same finding.
    """
    return leave.confirm_rows(leave.plan(live=live, focus="", only=target),
                              verb=leave.CLOSE)


def chose(row, target: str, *, fid: str) -> bool:
    """Act on the row Enter landed on. Answers whether anything was started.

    **The close DOORWAY is not here, and its absence is the confirmation.** `chose` starts
    work; a doorway starts none — it replaces the surface (`palette.own_the_tty`'s *then*,
    :func:`draw`) — so a row this function does not recognise answers ``False`` and the
    caller says its note. A version that closed the chat on :data:`CLOSE_ID` would put the
    one irreversible thing charter's frame can do exactly one keypress from a pointer
    gesture, which is what `frame/leave.py`'s row ordering exists to prevent and what a
    right-click makes more reachable rather than less.

    **A refused row starts nothing**, which is not merely tidy: `overlay.Surface` lets
    Enter land on any row and it is the caller that knows a refusal (the palette says the
    note on the operator's own screen). Handing `charter frame-transcript` a chat with no
    capture would open an empty pager for an operator who was told there was nothing to
    open.

    **`builtin_actions._spawn`, never a bare `Popen`.** The menu closes the instant a row
    has been chosen — `commands_frame._close_palette` is one chained tmux command — and
    `kill-pane` hands SIGHUP to this process's group, so work started in-process dies with
    the pane it was started from. That is measured at `builtin_actions._spawn`, and it is
    also why `$CHARTER_SESSION_ID` is STATED for the child rather than inherited.

    *target* is the tab, and *fid* is the chat this menu was opened over. **They are two
    different questions and close needs both**: `charter frame-close <target>` says which
    chat to stop, and `--chat <fid>` says where the keypress came from, which is what
    puts `cmd_close`'s sentence on the screen the operator is actually looking at rather
    than on the one that is about to stop existing.
    """
    from .builtin_actions import _spawn
    if row.refused:
        return False
    if row.id == TRANSCRIPT_ID:
        _spawn(util.self_relaunch_argv("frame-transcript", "--chat", target), fid=fid)
        return True
    if leave.goes_through(row, leave.CLOSE):
        _spawn(util.self_relaunch_argv("frame-close", target, "--chat", fid), fid=fid)
        return True
    return False


def opens(row, target: str, *, live) -> "palette.Palette | None":
    """The surface *row* opens, or ``None`` when it opens none.

    `commands_frame._picker`'s job on this menu, and the same two-line shape: a doorway is
    told apart from everything else by its id, and what comes back replaces the surface in
    the pane the operator is already looking at (`palette.own_the_tty`'s *then*) rather
    than starting a second one — which would race this pane's own teardown.

    **The plan is built HERE and not when the menu opened**, which is §4f's *at the moment
    the operator is deciding*: the rows describe the plane as it is under the keypress
    rather than as it was when the pointer landed, and a right-click that never reaches
    the doorway pays for no scan of the frame root at all — `catalogue` is one `is_file`.

    One test of the id and no test of `refused`: neither row this menu draws can be a
    refused doorway — the close row is never refused, because the tab is a name the panel
    resolved off a strip it had just painted, where `leave.open_rows`' own doorway can be
    (a palette that cannot tell which chat it was opened in has no chat to close). A guard
    for a state that cannot arrive is the line the deletion sweep reports.
    """
    if row.id != CLOSE_ID:
        return None
    return palette.Palette(catalogue=confirm_rows(target, live=live), label=leave.CLOSE,
                           mouse=True)


def act(row, target: str, *, fid: str) -> None:
    """Act on the row Enter landed on, and say the reason when it could not run.

    **One function because the three halves are one decision**, and because the surface a
    refusal would otherwise be drawn on is the pane :func:`draw` is about to kill. The
    frame's own attention row is a different pane and a different process, so the sentence
    survives this one (`commands_frame._say_on_screen`).

    **A ``None`` row is a cancel and is answered here rather than at the call site.**
    `overlay.Surface.run` answers ``None`` for Escape, for `overlay.HATCH_KEY`, and for
    the pane's writer going away — the one answer that can never become a wedge — and it
    is the commonest way this surface ends, because the row under the cursor when Escape
    is pressed is one keypress from a confirmation that stops a harness. Nothing was
    chosen, so there is nothing to start and nothing to say. Keeping it here rather than
    in :func:`draw` is what makes it a line a test can turn red: `draw` needs a tty.

    **A row with nothing to say says nothing**, and the guard is not tidiness: the notice
    is a WRITE, so an empty one would blank whatever the attention row was already
    carrying. The rows that reach here having started nothing are the transcript row on a
    chat with no capture (which carries `builtin_actions.NO_TRANSCRIPT`), the warning's own
    per-chat rows (which carry `leave.note`) and its *nothing left to stop* row, which
    carries none — so both branches are reachable and neither is a restatement of the
    other.
    """
    from .. import commands_frame
    if row is None or chose(row, target, fid=fid):
        return
    if row.note:
        commands_frame._say_on_screen(fid, row.note)


def handback(env) -> tuple[str, str, str, str]:
    """Who this pane is, and the three ids that give the harness back.

    ``(fid, socket, harness, overlay_pane)``: which frame this pane belongs to, which
    tmux it is on, which pane the keyboard goes back to, and which pane to kill —
    `commands_frame._close_palette`'s three arguments, resolved in one place so that
    :func:`draw` can put the whole of them in a ``finally`` that cannot itself raise.

    **Every one of the four falls back, and not one fallback is decorative.**

    * `$CHARTER_SESSION_ID` is absent whenever the pane was split with no ``-e`` payload,
      which is every tmux below `tmuxctl.PANE_ENV_FLOOR` — `commands_frame
      ._relayout_pane_env` answers ``None`` there and `overlay.open_argv` sets nothing, so
      the pane inherits whatever the shared server happens to hold. ``""`` is what every
      charter reader already treats as absent.
    * The socket falls back to charter's own for `builtin_actions._server`'s reason: a
      frame with no recorded server is one launched by a charter that predates
      `state.record_server`, and charter's own socket is where it will be — never
      "nowhere", which would aim the teardown at no server at all.
    * `state.harness_pane` answers ``None`` for a frame charter has lost the record of,
      and ``None`` is not ``""``: it would be formatted into a tmux target as the four
      characters `None`, naming a pane that cannot exist. `frame/overlay.py` measured what
      an EMPTY target costs instead — `kill-pane -t ""` kills the pane the command is
      running against — which is why `overlay.hatch_command` emits no `kill-pane` at all
      rather than one with nothing in it.
    * `$TMUX_PANE` is absent in any process tmux did not start, which is every test of
      this function and would be a hand-typed `charter frame-palette --pane`.

    *env* is passed rather than read, so all four are reachable from a test — the reason
    `commands_frame._relayout_pane_env` takes *fid* as an argument instead of reading it
    back out of a variable one tmux server shares between every frame on the machine.
    """
    from .. import commands_frame
    from . import state
    fid = env.get("CHARTER_SESSION_ID", "")
    return (fid,
            state.frame_server(fid) or commands_frame.SOCKET,
            state.harness_pane(fid) or "",
            env.get("TMUX_PANE", ""))


def draw(args) -> int:
    """Be the tab menu: draw the rows, take a choice, act on it, hand the pane back.

    `commands_frame._draw_palette`'s shape, and deliberately its shape rather than a
    branch inside it: everything below is about ONE chat that is not necessarily this
    frame's, where every line of that function is about the frame itself.

    **The close is a ``finally``**, one layer up from `overlay.Surface.run`'s own: a menu
    that raised must still give the operator their harness back, and whatever went wrong
    is one traceback into a pane that is about to stop existing — the one place charter
    may print one.

    **The frame is read out of the environment and the tab off the argv**, which is the
    split this whole feature turns on. `commands_frame._relayout_pane_env` told this pane
    which frame it belongs to, because one tmux server is shared by every frame on the
    machine; the tab travels on the argv because it is not the frame — it is a name the
    panel resolved out of a click map, and the pane the menu is drawn in was carved off
    THIS frame's harness so that a menu about another tab still appears where the operator
    is looking. :func:`handback` is the whole of the first half, resolved before the
    ``try`` so the ``finally`` below has nothing left to compute and nothing left to
    raise.

    **Always 0**, for `cmd_palette`'s reason: a non-zero return from a `run-shell` child
    is printed INTO THE HARNESS PANE and drops it into copy-mode, which is charter drawing
    in the one rectangle ADR 0018 says it never draws.
    """
    from .. import commands_frame
    fid, socket, harness, overlay_pane = handback(os.environ)
    target = wanted(args)
    try:
        surface = palette.Palette(catalogue=catalogue(target), label=label(target),
                                  mouse=True)

        def _then(row):
            # **This runs on a keypress, never on the open**, which is what makes reading
            # the plane here affordable: `leave.open_rows`' rule is that a menu merely
            # being up must cost no scan, and it does not — `catalogue` reads one
            # `is_file`. One `list-windows` per server on the Enter that was pressed is
            # the same round trip `commands_frame._picker` makes for the identical
            # doorway, and it is what makes the warning describe the plane as it is under
            # the keypress rather than as it was when the pointer landed.
            return opens(row, target,
                         live=commands_frame._plane_live(
                             commands_frame._plane_servers())[0])

        # `act` takes the cancel too (`own_the_tty` answers `None` for Escape, for the
        # hatch, and for a pane whose writer is gone), so there is no branch here that a
        # test would need a tty to reach.
        act(palette.own_the_tty(surface, then=_then), target, fid=fid)
    finally:
        commands_frame._close_palette(socket, harness=harness,
                                      overlay_pane=overlay_pane)
    return 0
