"""Charter's own panels, expressed as components — the same seam a provider gets.

Four strings in a list is what the frame used to be, and their POSITION was the geometry
(`instance.FRAME_SLOTS`). This module is where those four names become components that
declare their own edge, their own size policy and what they read, so that everything
downstream asks the registry a question instead of remembering an answer.

**Charter's own panels go through the public seam first** (§4b's sequencing note). There
is no private table of edges beside the one a provider's component will be placed from:
`layout` derives every one of its per-slot facts — which splits cost columns, which
strips are a fixed height, which one takes what its content needs — from
:func:`build`'s registry, and nothing derives them from a list position any more.

**Phase 1's own task changed no output, and that was its point.** Each of the components
below wraps a renderer `frame/slots.py` already had, unchanged; those declarations are a
statement of what the renderers already do, not a new arrangement, and the before/after
render at 200x50 and 80x24 was byte-identical.

**Phase 5's two bars are the first entries here that are not that**, and they changed no
output either — for a different reason. `chats` and `workspaces` are registered and NOT
placed (see :func:`build`), so nothing draws them until a plane writes a
`[[frame.component]]` table naming one. :func:`places` is the question that makes that
route work at all. A plane that has placed one gets a clickable bar; every other plane's
frame is byte-identical to what it was, because there is no such pane on it.

**The slot names survive as SHORTHAND, because they are committed** (:data:`SLOT_OF`).
`[frame] slots = ["top", "bottom", "repos", "right"]` sits in charter.toml on every plane
that has one, `charter panel top --session …` is the argv `layout.panel_command` emits
into a tmux pane, and both are compatibility surfaces. A component id is the frame's
currency and a slot name is one of four aliases for a built-in id, resolved by
:func:`component_id` — one table, rather than a rename rippling through tmux argv, config
files and the renderer registry at once.

**Which is why a name that is NOT one of those four is a component id, not a typo.** That
is the whole of Phase 2's Task 1: `charter panel acme.metrics` reaches a component an
installed provider supplies (:func:`supplies`), because every step between a committed
`[[frame.component]]` table and a painted pane now resolves a NAME rather than looking one
up in a table of four. Phase 1 could place a provider and never draw it for exactly the
opposite reason.

**What each component declares in ``needs`` is what its RENDERER actually reads, which is
not the same as what the slice names suggest.**

* `identity` and `attention` declare nothing. They look like they should read the plane —
  `_bottom` prints a todo count — but `statusline._todo_count` is a directory glob
  (`todos.count_open`) and `_alerts` reads charter.toml and the persona roster. Neither
  goes near `gather`, and a declaration saying otherwise would make the frame's cost
  budget describe a cost that is not there.
* `personas` declares nothing either, and the coordinator's note on this is worth keeping:
  its renderer reads `statusline._persona_chip_cells()` directly. That is a fact about the
  renderer, not about the contract. Declaring a `personas` slice `ctx` cannot serve would
  hand a component an empty tuple it could draw nothing from and pass its own tests
  against — a convincing empty, which is worse than a refusal and is exactly the defect
  #512 fixed. `component.NEEDS` names the three slices `gather.scan` genuinely carries;
  personas joins it when it can be served, not before.
* `repos`, `todos` and `sidebar` declare ``gather`` — the whole scan, not the narrower
  ``repos``/``todos`` slices — and each for a reason the narrow slice cannot cover.
  `_repos` reads ``worktrees`` and ``current_repo`` alongside the rows, and it tells
  ``gather.cached(fid) is None`` ("nothing has scanned yet") apart from an empty scan
  ("this workspace has no clones"), a distinction the `repos` slice flattens to ``()`` for
  both. `_todo_rows` reads ``todo_count``, the UNCLIPPED total, which is how the
  `…(+N more)` line can say how many are hidden; a component built on the `todos` slice
  alone would report zero hidden todos with no way to know it was wrong.

**Three of them take EVENTS, and `repos` was the first thing anywhere that did.** #607
built the whole path — the decoder, the dispatcher, `DELIVERED` — and shipped it with no
consumer: every one of these components declared `events = ()`, so a release went by in
which a provider could declare `scroll`, pass validation, and be the only thing on the
machine receiving anything. `repos` declares `scroll` and `click` and :func:`_repos_events`
receives them, which makes charter's own panel the worked example rather than the exception
— the same sequencing §4b asks for and the same reason charter's panels went through the
component seam first. What that handler does and does not do is its own docstring; the
short of it is that a click SELECTS and never chooses, because a pointer event can arrive
unpaired.

**The two bars are the second and third, and a click on one of them SWITCHES.** They
shipped with the same empty declaration and the same consequence one surface over: an
operator placed both, clicked a tab, and nothing happened, because a bar with no
`on_event` is a caption that happens to list names. :func:`_bar_events` is what receives
them now. The difference from the table is argued at that function rather than here, and
it turns on the two things a bar does not have: an intermediate "selected" state to
confirm, and a keyboard to confirm it with (`key` is a kind `events.DELIVERED` does not
carry, because the harness owns the keyboard).

**Registration order is split order** (`registry.Registry`), so the four placed components
are registered in the order charter splits their panes off the harness: identity,
attention, repos, sidebar. The two parts of the sidebar are registered between them,
because the registry refuses a composite whose parts it has not seen — they take split
numbers of their own and are never placed, which `Registry.on_edge` is what enforces. The
two bars are registered LAST, after everything charter places, so adding them moved no
existing component's split number.
"""

from __future__ import annotations

from .component import EDGES, Component, Content, Fill, Fixed
from .registry import Registry

#: Component id → the `[frame] slots` name it is spelled with in a committed charter.toml
#: and in `charter panel <slot>`'s argv. The one table between the two vocabularies; see
#: this module's docstring for why both exist.
#:
#: Only PLACED components are here. `personas` and `todos` share the sidebar's pane and
#: were never slots, so there is nothing for them to be spelled as.
#: ``changes`` is absent for the same reason ``personas`` and ``todos`` are: this table is
#: PLACED components, and charter does not place it. A plane that wants it writes a
#: `[[frame.component]]` table, and travels under its own id — which is the path a
#: provider's component already takes, and `instance.FRAME_SLOTS` says why it is the right
#: one here too.
SLOT_OF = {
    "identity": "top",
    "attention": "bottom",
    "repos": "repos",
    "sidebar": "right",
}

#: The reverse, for the direction the config boundary reads in: a committed slot name to
#: the component that draws it. Derived rather than written out, so the two cannot
#: disagree about a name — the shape `instance.FRAME_DEFAULTS` already uses.
COMPONENT_OF = {slot: cid for cid, slot in SLOT_OF.items()}


def component_id(name):
    """The component *name* names: a committed slot name resolved, anything else itself.

    **The one direction the two vocabularies are read in**, and the reason `[frame] slots`
    could be called shorthand rather than a second system. `top` is `identity`'s committed
    spelling; `acme.metrics` is a provider's id and has no committed spelling, so it is
    already what it resolves to.

    Unambiguous by inspection and not by luck: no slot name is another component's id
    (`test_component_registry` asks it of the tables rather than of this sentence), so a
    name resolves one way whichever vocabulary it was written in.

    Anything that is not text comes back as it went in. This is asked of values that came
    out of a committed file, and the refusal belongs to whatever validates the value, with
    the rest of its message — not to a lookup that would raise `TypeError` half a frame
    away from the line that was wrong.
    """
    return COMPONENT_OF.get(name, name) if isinstance(name, str) else name


def places(cid, reg: Registry | None = None) -> bool:
    """Whether charter's OWN registry puts *cid* on an edge — a component a plane may
    place, whether or not it has a committed slot-name spelling.

    **The question `SLOT_OF` was standing in for, asked directly.** That table is the
    shorthand between two vocabularies — a committed `[frame] slots` name and a component
    id — and two separate places had come to read "is it in `SLOT_OF`" as "is it one of
    charter's own placeable components" (`instance.component_tables` and
    `slots.drawable`). Those were the same set for as long as every
    component charter placed had a slot name, and Phase 5's two bars are the first that do
    not: they have no committed spelling, because there is no `[frame] slots` word for a
    thing that did not exist when that list was frozen, and adding one would put them on
    every operator's frame (`build`'s own comment measures what that costs).

    Without this the bars would be registered and unplaceable — a component charter can
    draw that no configuration can ask for, which is dead code wearing a feature's name.
    With it they are exactly as placeable as a provider's component, through the one form
    that can place one: a `[[frame.component]]` table.

    `Registry.on_edge` is what answers, so a composite's PARTS are excluded for free —
    `personas`, `todos` and `changes` are drawn inside the sidebar's pane, and a part that
    could be placed as well would be drawn twice.

    *reg* is a registry the caller already has. `instance.component_tables` builds one to
    resolve the arrangement and then asks this once per table, so without it a committed
    `[[frame.component]]` list would rebuild the registry per row — on the path of every
    charter command, `charter --version` included, since `config.derive` resolves `FRAME`
    at import. ``None`` builds one, which is what the callers that have none do
    (`slots.drawable`, and every test). It is a parameter and not a module-level cache for
    `supplies`' reason: a registry kept from the first ask answers for the ``sys.path``
    charter had then, which is wrong for a long-lived process and wrong in the direction
    that is hard to see.
    """
    # **No `isinstance` refusal, and the deletion sweep is why.** This is asked of a value
    # read out of a committed `charter.toml`, so a TOML array or table can reach it — and
    # `Registry.on_edge` answers with components whose `id` is always a string, so
    # `c.id == cid` is already False for every one of them. A guard in front of that is a
    # line no input can make observable, which the sweep found as a survivor. The property
    # it was protecting is structural rather than guarded, and
    # `ABarIsPlaceableByConfig.test_places_refuses_anything_that_is_not_a_name` keeps
    # asking for it.
    if reg is None:
        reg = build()
    return any(c.id == cid for edge in EDGES for c in reg.on_edge(edge))


def supplies(cid) -> bool:
    """Whether an installed distribution supplies the component *cid* — without importing.

    Entry point METADATA only, which is what makes this askable from the places that ask
    it: `frame/panel.py` before it draws, `frame/slots.py:unimplemented` before a pane is
    split, and `commands_frame._arm_panel_respawn` before a name reaches tmux config text.
    None of them may import a stranger's module to find out whether one exists, and the
    respawn hook in particular must not: it runs while charter is arming a pane, on a name
    that has been read back off disk.

    A fresh :class:`registry.Providers` per call rather than one cached at module scope.
    The scan is ~0.2 ms and `importlib.metadata` caches underneath it, so the cache would
    buy nothing measurable — and it would answer for the ``sys.path`` charter had the
    first time anything asked, which is wrong for a long-lived process, wrong for a test
    that installs a distribution, and wrong in the direction that is hard to see: a stale
    "no such provider" is a pane that never appears.
    """
    return Registry().providers.supplies(cid)


def _panel(slot: str):
    """The whole-pane renderer for *slot*, adapted to the component contract.

    ``slots.SLOTS`` is read at CALL time rather than captured here, and the tests that
    replace an entry of it (`mock.patch.dict(slots.SLOTS)`) are why: a renderer captured
    at registration would go on drawing after the table it came from had been changed,
    which is the same stale-copy failure `panel_command`'s docstring records for the
    respawn argv.

    ``split("\\n")`` and never ``splitlines()``. A panel writes the renderer's string out
    as it is, so the adaptation has to round-trip: ``"\\n".join(s.split("\\n")) == s`` for
    every string there is, while ``splitlines()`` turns a renderer that answered one empty
    line into a component that answered no lines at all.

    **The wrapped renderer measures its own pane and ignores ``ctx.width``.** That is
    today's truth and this says so rather than implying otherwise: `slots._width` asks the
    pane's own tty because a panel process inherits the LAUNCHING shell's ``$COLUMNS``
    (`frame/slots.py`'s module docstring measures a 22-column pane reporting 200). The
    geometry on `ctx` is what a component written against the contract reads; moving
    charter's own renderers onto it is a later task, and doing it here would have changed
    output, which this one may not.
    """
    def render(ctx) -> list[str]:
        from . import slots
        return slots.SLOTS[slot](ctx.fid).split("\n")

    render.__name__ = f"render_{slot}"
    render.__qualname__ = render.__name__
    return render


#: Which mouse button charter acts on. One, and it is named rather than "whatever came":
#: middle-click is paste on every terminal an operator has ever used and right-click opens
#: their emulator's own menu, so acting on either would be charter taking a gesture that
#: already means something else. `overlay._SGR_BUTTONS` is where the three get their names,
#: and the ones §4f named no kind for never arrive here at all.
#:
#: **One constant for the table and for both bars**, because "which button did the operator
#: mean" is one question — a second answer to it would be a frame where the same gesture
#: works on one pane and does nothing on the next, which is the thing an operator reports
#: as a bug and is right to.
_ACT_BUTTON = "left"


def _repos_events(fid: str):
    """The `repos` table's handler: the wheel scrolls it, a click selects a row.

    **Charter's own first consumer of the event path #607 built**, and the six built-ins
    declared nothing until this one. What it demonstrates is meant to be the shape a
    provider copies, so the two rules it keeps are the two that are easy to get wrong:

    **A click only ever SELECTS.** Nothing here is irreversible, nothing here starts work,
    and that is not caution — it follows from what `frame/overlay.py` measured. A `click`
    release may arrive with no matching press (a drag begun on a pane border delivers
    exactly one release), so a pointer event is a thing that can arrive unpaired, and §4i's
    rule is that the irreversible half is never driven by one. Choosing is a keypress's job.
    A component that wanted to *do* something to the selected repo would put that behind
    `Enter` in a palette row, where it has a name and a confirmation.

    **The PRESS is acted on and the release is dropped**, which is `component.EVENT_KINDS`'
    "act on one of them, never wait for the pair" answered in the direction that matches
    what the operator did: the press is where they pointed. A drag that BEGAN elsewhere and
    happens to release over this pane delivers only a release and selects nothing, which is
    right — they never pointed here.

    **Neither branch reads the plane, and that is the contract rather than thrift.** A
    handler is handed no ctx (§4f), so it cannot know how many repos there are or how tall
    this pane is; both are `slots._repos`' to compute, and it hands them to
    `slots.VIEWPORT` on every paint. So a wheel notch on a pane that already shows every
    repo — which is the ordinary plane, because the pane is sized to its content — moves
    nothing and answers falsy, and the frame does not repaint. The one plane read here is
    `state.selection`, and it is read to answer *has anything changed*, not to draw.

    **The click bumps the frame's version, and that is what makes the third surface work.**
    The selected row's detail is drawn on the ATTENTION row, which is a different pane and a
    different process (`slots._selected_detail`). Returning truthy repaints this panel and
    only this panel; `state.bump` is how every other cross-panel fact in this frame travels,
    and the poll it wakes is `panel._tick`'s existing one. Re-selecting the row that is
    already selected is not news for either pane, so it does neither — which is also what
    keeps a double-click from bumping the frame twice.

    *fid* is closed over rather than resolved: this process was told which frame it is
    drawing (`charter panel repos --session <fid>`), and a handler that read
    `$CHARTER_SESSION_ID` back out of its own environment would be answering from a variable
    one tmux server shares between every frame on the machine — the trap
    `state.record_identity` measured.
    """
    def on_event(ev):
        from . import overlay, state
        from . import slots as _slots
        if ev.kind == overlay.SCROLL:
            return _slots.VIEWPORT.move(
                _slots.SCROLL_ROWS if ev.name == "down" else -_slots.SCROLL_ROWS)
        if not ev.pressed or ev.name != _ACT_BUTTON:
            return False
        name = _slots.VIEWPORT.repo_at(ev.row)
        if name is None or name == state.selection(fid):
            return False
        state.record_selection(fid, name)
        state.bump(fid)
        return True

    return on_event


#: What a click on the `chats` bar starts, and what a click on `workspaces` starts — the
#: arguments in front of the name, which goes LAST on both commands (`cli._wire`).
#:
#: **The existing front door for each noun, and neither is a shortcut past it.** A chat
#: switch is `commands_frame.cmd_chat` — `chats.check`'s five refusals, `_pane_place`'s
#: cross-session reading (#684), `select-window`, and the two re-layouts — and a workspace
#: switch is `cmd_switch` over `switch.to_workspace`'s three plus `_switch_client`'s own
#: (§4b): which plane's session that name has, whether anybody is attached to move, and
#: whether the client really moved. Every one of those refusals reaches the operator
#: through `_say_on_screen`, which writes to the frame's own attention row; a panel process
#: has no surface of its own, and a click that silently did nothing is exactly the report
#: this feature answers.
#:
#: Spelled as the argv rather than called in-process for a second reason: what these start
#: is slow and is not this loop's to wait on. A chat switch is 41 tmux invocations and
#: ~360 ms measured; a workspace switch is a `switch-client` plus a re-layout at each end,
#: which is the same order of cost. `panel._watch` is a paint loop, and a handler that
#: blocked it would freeze the pane it was clicked on — and this pane is one of the panels
#: the switch it starts is about to tear down.
_CHAT_SWITCH = ("frame-chat",)
_WORKSPACE_SWITCH = ("frame-switch", "--workspace")


def _bar_events(fid: str, command: tuple[str, ...]):
    """A tab bar's handler: a left click on a tab switches this frame to it.

    **A click here SWITCHES, where a click on the repo table only ever SELECTS, and that
    difference is a decision rather than a drift.** §4i's rule is that the irreversible
    half of an interaction is never driven by a pointer event, because one can arrive
    unpaired — a drag begun on a pane border delivers exactly one release. Three things
    put a tab bar on the other side of that rule:

    * **The unpaired event is the RELEASE, and this acts on the PRESS.** That is
      :func:`_repos_events`' own reading of §4i, kept here word for word: a press is
      delivered because the pointer was over this pane when the button went down, which
      is the operator pointing at this tab. A drag that began elsewhere and happens to
      release over the bar delivers only a release and switches nothing, which is right —
      they never pointed here.
    * **A switch is reversible by the same gesture that made it.** The tab you left is
      still on the bar, one click away; nothing is created, nothing is destroyed, and no
      work is started. That holds for a workspace tab exactly as it does for a chat tab
      (§4b): the switch moves a tmux client, the workspace it left keeps every harness it
      had, and clicking the tab you came from puts you back. That is the property §4i is
      actually about, and it holds.
    * **There is no chooser for a select-then-confirm bar to be confirmed with.** On the
      table, selecting is a real intermediate state (a highlighted row, a detail on the
      attention strip) and `Enter` in the palette is the chooser. A bar has no such
      state — the tab you are on IS the selection, drawn with `slots._BAR_MARK` — and
      `key` is in `component.EVENT_KINDS` but deliberately NOT in `events.DELIVERED`,
      because the harness owns the keyboard and tmux routes typing to the active pane. So
      a bar that "selected" would draw a second mark nothing on the machine could act on:
      a feature that cannot be finished, which is the dead code `places` refuses to ship
      under a feature's name.

    **A click on the tab you are already on does nothing**, and that rule lives in
    `slots._Tabs.switch_to` rather than here — see it for why, and for the other thing
    that answers nothing: every cell this bar drew no tab into.

    **A click on the `+14` opens the palette, and that is a second gesture on one row
    rather than a widening of the first.** The overflow counts stand for names that are
    NOT on the row, so there is nothing there to switch to and there never will be — that
    much is unchanged. What changed is the conclusion drawn from it. Answering nothing was
    right while the alternative was picking one of the names it stands for; it is not
    right when the operator can see a field that says *nine more* and press it, which is
    what the operator this change is for did.

    So the count hands off to the palette, which is §3.6 arriving where it was always
    heading: *the bar is a readout, never the mechanism*, and the palette reaches every
    chat and every workspace in two keystrokes at every width — including the widths where
    this bar can draw no name at all. That last part is why the narrow rung's `2/3` is a
    count here too: it is the same field at the width where the strip reaches nothing.

    **It is :func:`_strip_events`' gesture, not this one's**, and the difference is worth
    keeping straight because both live in this handler now. A tab click completes on the
    pointer: it switches, and the tab you left is one click away. A count click cannot —
    it names no chat — so it can only mean *let me see the rest*, and seeing needs a
    chooser. `key` is in `component.EVENT_KINDS` and deliberately not in
    `events.DELIVERED`, so charter cannot grow a pointer-driven chooser inside a one-row
    pane; the palette IS the chooser and it makes itself the active pane
    (`overlay.modal_argvs`), so the keyboard reaches the list it just drew.

    **The palette it opens is the top-level one, not the picker for this bar's noun**, and
    that is an honest cost rather than a design. `charter frame-palette` has no way to say
    "open in the `workspaces` picker": the option would live in `commands_frame.cmd_palette`
    and `cli.py`, and what the operator gets meanwhile is the doorway row for their noun
    with the name they are on already in it, one keystroke from the list. That is one
    keystroke more than it should be and infinitely fewer than a field that does nothing.

    **`switch_to` is asked first and `more_at` second, and the two cannot both answer.**
    A count is not in the column map and a tab is not in the overflow set — `slots._bar`
    builds them from one walk of one composition — so the order is a reading order rather
    than a precedence. It is written this way round because the tab is the common case.

    **It answers falsy even when it started a switch**, which is the one place this reads
    differently from the table's handler. Truthy means *repaint me* (§4f), and nothing
    this process can see has changed: the switch happens in another process and ends in a
    `state.bump` of its own — `switch.to_persona`'s last line, `_apply_arrangement`'s —
    which is the version this panel's poll is already watching. Answering truthy would
    buy one immediate repaint of a byte-identical row, which is the cost
    `slots._Viewport.move` refuses in as many words.

    **No `ev.kind` test**, unlike the table's handler: `chats` and `workspaces` declare
    `click` and nothing else, and `events.Dispatcher._deliver` drops a kind the component
    did not declare before it ever reaches here. A branch on a kind that cannot arrive is
    a line no input could turn red.

    **`scroll` is not declared, and that is not an omission.** A bar is one row with
    nothing to scroll; a handler for it could only ever answer False, and the wheel is not
    a gesture to hang a switch on. `events.Dispatcher.open` charges the same
    `overlay.MOUSE_ON` for one pointer kind as for two, so declaring it would cost nothing
    and mean nothing — which is precisely what makes it wrong to declare.

    *command* is which of the two switches this bar starts (:data:`_CHAT_SWITCH`,
    :data:`_WORKSPACE_SWITCH`). One handler and two lines of data rather than two
    handlers, because everything above is true of both bars and a second copy would be
    two places for it to stop being true.

    *fid* is closed over for :func:`_repos_events`' reason and is handed to
    `builtin_actions._spawn` as the child's own `$CHARTER_SESSION_ID`: this process was
    TOLD which frame it draws (`charter panel chats --session <fid>`), and one tmux server
    is shared by every frame on the machine, so a child left to read that variable out of
    an inherited environment could act on another operator's frame.

    **No `--chat` on the chat switch, unlike the palette's row.** `_pressers_chat` prefers
    that option because a palette's `bind` text is shared by every frame on the socket and
    `#{@charter_chat}` is the only thing that can tell two chats of one session apart. A
    panel has no such ambiguity and `_spawn` states the id outright, so the option would be
    a second spelling of a value the environment already carries — provably equal, which
    the deletion sweep reports and this repository deletes.
    """
    def on_event(ev):
        from .. import util
        from . import slots as _slots
        from .builtin_actions import _spawn
        if not ev.pressed or ev.name != _ACT_BUTTON:
            return False
        name = _slots.TABS.switch_to(ev.col)
        if name is None:
            if _slots.TABS.more_at(ev.col):
                # The same argv `_strip_events` spawns for a door, for its reasons — one
                # answer to "how does a frame surface open the palette", shared rather than
                # copied. Falsy afterwards for this handler's own reason: the palette's
                # pane is carved off the harness, so nothing in THIS rectangle changed.
                _spawn(util.self_relaunch_argv(*_PALETTE), fid=fid)
            return False
        # `builtin_actions._spawn` and not a `Popen` of this module's own — one answer to
        # "how does a frame surface start work that must outlive it", shared with every
        # palette row and with `commands_frame._start_chat_switch`, which starts this
        # exact argv for the exact same switch. Its `start_new_session=True` and its three
        # `DEVNULL` streams are its own docstring's argument and are not re-argued here:
        # what a second copy would buy is a second place for them to stop agreeing.
        _spawn(util.self_relaunch_argv(*command, name), fid=fid)
        return False

    return on_event


#: What a click on the `personas` column starts — the same front door
#: :data:`_WORKSPACE_SWITCH` opens for the other noun, one option over
#: (`commands_frame.cmd_switch` takes both). Not `choose.switch_to` called in process, for
#: :data:`_CHAT_SWITCH`'s two reasons: every refusal `switch.to_persona` can raise reaches
#: the operator through `_say_on_screen`, which is a `display-message` on the frame's own
#: client and a surface no panel process has; and a switch re-gathers the plane, which is
#: not a paint loop's work to sit through.
_PERSONA_SWITCH = ("frame-switch", "--persona")

#: What a click on a door opens: `F2`'s own action, spelled the way `commands_frame
#: .conf_text` binds it minus the client name.
#:
#: **The client is not passed, and that is a real difference from the bind.** `conf_text`
#: expands `#{client_name}` because tmux knows which client pressed the key; a panel
#: process is not a `run-shell` child of a keypress and has no presser to name. What the
#: client buys is which screen `cmd_palette`'s refusals are drawn on, and without it
#: `_say_on_screen` falls back to `-t <session>` — the most recently attached client,
#: measured in that function. On the one-client frame this is drawn for they are the same
#: screen; on a two-client one a refusal can land on the other operator's status line,
#: which is the honest cost of a doorway that is a rectangle rather than a keystroke.
#:
#: **And no `--chat` either, for `_bar_events`' reason**: `builtin_actions._spawn` sets
#: `$CHARTER_SESSION_ID` to this panel's own *fid*, which is the chat this pane was
#: launched for, and `commands_frame._pressers_chat` falls back to exactly that. The
#: option exists because one `bind` text is shared by every frame on the socket; a panel
#: was TOLD which frame it draws and has no such ambiguity to resolve.
_PALETTE = ("frame-palette",)


def _strip_events(fid: str):
    """The identity and attention strips' handler: a click on a door opens the palette.

    **The complaint this answers is that `F2 palette` is the only affordance charter
    advertises on screen and the one thing on the row that could not be operated the way
    it is drawn** (#751). A key name beside a noun is a button everywhere else an operator
    has seen one. It is a button here now, and so are the workspace chip and the persona
    head beside it — the other two nouns on those rows that the palette already opens by
    name.

    **All three go to the same place, and that is the design rather than a shortcut.**
    §4i's rule is that the irreversible half of an interaction never rides on a pointer
    event, because one can arrive unpaired. `_bar_events` gets out from under it by acting
    on the PRESS and switching to a tab that is still one click away — a gesture that
    completes on the pointer. A strip has no such gesture available: `⬢ alpha` names the
    workspace you are ON and `◆ steward` the persona you ARE, so a click on either can
    only mean *let me pick another one*, and picking needs a chooser. `key` is in
    `component.EVENT_KINDS` and deliberately NOT in `events.DELIVERED` — the harness owns
    the keyboard — so charter cannot grow a second, pointer-driven chooser inside a one-row
    pane. The palette IS the chooser, it is the surface both nouns are reachable through
    already, and opening it is a complete gesture: the overlay makes itself the active
    pane (`overlay.modal_argvs`), so the keyboard reaches the list it just drew.

    **The PRESS is acted on and the release is dropped**, kept word for word from
    :func:`_repos_events` and :func:`_bar_events`: a drag begun on a pane border delivers
    exactly one release (`frame/overlay.py` measured it), and a drag that began elsewhere
    and happens to end over this row never pointed here.

    **It answers falsy even when it opened the palette**, which is :func:`_bar_events`'
    reading and not the table's: nothing this process draws has changed, and the pane the
    palette carves comes off the harness rather than out of this rectangle. Truthy would
    buy one repaint of a byte-identical row.

    **No `ev.kind` test and no `scroll`**, for :func:`_bar_events`' reasons — these two
    components declare `click` and nothing else, `events.Dispatcher._deliver` drops
    anything they did not declare before it reaches here, and a one-row strip has nothing
    a wheel could move.

    *fid* is closed over rather than read back out of `$CHARTER_SESSION_ID`: one tmux
    server is shared by every frame on the machine, and this process was told which frame
    it draws.
    """
    def on_event(ev):
        from .. import util
        from . import slots as _slots
        from .builtin_actions import _spawn
        if not ev.pressed or ev.name != _ACT_BUTTON:
            return False
        if not _slots.DOORS.opens_palette(ev.col):
            return False
        _spawn(util.self_relaunch_argv(*_PALETTE), fid=fid)
        return False

    return on_event


def _persona_events(fid: str):
    """The sidebar's handler: a persona's NAME switches to it, its BADGES say what they mean.

    **Two meanings on one row, resolved by which CELL the pointer landed in** — which is
    not a second gesture bolted on, but the layout `slots._persona_rows` already draws
    being read back. A row is a name cell and a badge cell, and they are asking different
    questions: the name is *be this persona*, the badges are *what is that glyph*.
    `slots._Chips.hit` owns the resolution and this branches on what it answers.

    **The complaint is that the sidebar draws the roster as an inverted-row list with a
    cursor-looking marker on the active one — the visual vocabulary of something you pick
    from — and handles no events at all** (#742). It picks from now on.

    **A click SWITCHES, exactly as it does on the two tab bars, and the three reasons
    :func:`_bar_events` states hold here unchanged**: this acts on the PRESS, so the one
    event that can arrive unpaired (a release from a drag begun elsewhere) switches
    nothing; a persona switch is reversible by the identical gesture, because the row you
    left is still in the column one click away and nothing is created, destroyed or
    started; and there is no chooser a select-then-confirm column could be confirmed with,
    since `key` is in `component.EVENT_KINDS` and deliberately not in `events.DELIVERED`.
    A column that merely *selected* would draw a second mark beside `▸` that nothing on
    the machine could ever act on.

    That is where this deliberately departs from #742's own suggestion, which asked for
    "selecting, not switching — the same 'a click only ever selects' rule the repo table
    follows". The table's rule is not "a click never switches", it is `frame/overlay.py`'s
    "the irreversible half is never driven by an event that can arrive unpaired", and what
    makes selection the right answer THERE is that the table has a real intermediate state
    (a highlighted row, a detail on the attention strip) and `Enter` in the palette to
    choose from it. A persona column has neither: the row in reverse video IS the
    selection, and a second one would be dead code wearing a feature's name.

    **The rows that answer nothing are `slots._Chips`' to refuse, not this handler's** —
    the `▪ personas 6` heading, the `…(+N more)` row, the `no personas` sentence, the
    blank rows between the sidebar's sections, every todo and change row below them, and
    the persona you are already being. See that class; the last of those is
    `_Tabs.switch_to`'s rule and is also what stops a double-click switching twice.

    **The badge half deliberately does NOT refuse the persona you are being.** *"What does
    the flag on my own row mean"* is the commonest form of the question #753 is about, and
    answering it is not the no-op that re-adopting yourself would be. That asymmetry is
    `_Chips.hit`'s and is argued there.

    **Falsy even when it started a switch**, for :func:`_bar_events`' reason:
    `switch.to_persona` ends in a `state.bump` of its own, which is the version this
    panel's poll is already watching, so truthy would buy one repaint of a row that has
    not changed yet.

    **It is declared on the `sidebar` COMPOSITE and nowhere else, because
    `frame/registry.py` refuses a part that declares events** — a part is never placed
    (`on_edge`), and `panel._dispatcher` asks `wanted()` of the component that WAS placed,
    so a declaration on `personas` would pass every check, build a handler and receive
    nothing, ever. That refusal says the composite "is also the only thing that knows
    which of its parts a pointer would have been over"; here it does not have to work that
    out, because `slots.persona_section` is the one pass that composes those rows and they
    are the first thing the pane draws, so `slots.CHIPS` needs no base and no offset.

    The cost is stated at the `personas` registration: a plane that places that component
    on its own gets the column drawn and inert.
    """
    def on_event(ev):
        from .. import util
        from . import slots as _slots
        from . import state
        from .builtin_actions import _spawn
        if not ev.pressed or ev.name != _ACT_BUTTON:
            return False
        hit = _slots.CHIPS.hit(ev.row, ev.col)
        if hit is None:
            return False
        if hit.explain:
            # **#729's dwell, used rather than a second surface of its own** — which is
            # the whole reason this half of #753 is three lines. `state.say` writes the
            # frame's own notice file and bumps the version, so the ATTENTION pane — a
            # different process — draws the legend on its next poll and drops it when the
            # dwell expires. Nothing here draws, and nothing here waits.
            #
            # `REFUSAL_SECONDS` and not the default four: that constant is the longer
            # dwell for the outcome that has to be READ rather than glanced at, and a
            # legend is exactly that — seven glyphs and what each one stands for, against
            # a `workspace → gamma` the frame has already confirmed by repainting.
            state.say(fid, _slots.BADGE_LEGEND, seconds=state.REFUSAL_SECONDS)
            return False
        _spawn(util.self_relaunch_argv(*_PERSONA_SWITCH, hit.name), fid=fid)
        return False

    return on_event


def _chats(ctx) -> list[str]:
    """The chat bar — `slots.chats_bar` at this pane's OWN width.

    `ctx.width`, unlike the four wrapped whole-pane renderers (:func:`_panel`), which
    measure their own tty and ignore it. A bar written for the contract can read the
    geometry the contract carries, and this is the first charter component that does.
    """
    from . import slots
    return slots.chats_bar(ctx.fid, ctx.width)


def _workspaces(ctx) -> list[str]:
    """The workspace bar — `slots.workspaces_bar` at this pane's own width."""
    from . import slots
    return slots.workspaces_bar(ctx.fid, ctx.width)


def _personas(ctx) -> list[str]:
    """The sidebar's persona rows — `slots.persona_section` at this pane's size."""
    from . import slots
    return slots.persona_section(ctx.width, ctx.height,
                                 terse=slots.verbosity(ctx.fid) == "terse")


def _todos(ctx) -> list[str]:
    """The sidebar's todo rows, spending the rows the pane gave this part."""
    from . import slots
    return slots.todo_section(ctx.fid, ctx.width, ctx.height,
                              terse=slots.verbosity(ctx.fid) == "terse")


def _changes(ctx) -> list[str]:
    """The sidebar's cross-repo change rows — nothing at all when there are none.

    No `terse`: the section is already one heading and at most three rows, and a density
    that made it shorter would be making a list that is usually empty shorter still.
    """
    from . import slots
    return slots.changes_section(ctx.fid, ctx.width, ctx.height)


def build(fid: str = "") -> Registry:
    """A registry holding charter's six built-in components, in split order.

    A fresh one per call, deliberately — `registry.Registry`'s own docstring argues it:
    module-level mutable state shared behind per-caller objects is isolation that is a
    fiction. `layout` asks once at import for the edges and sizes it derives; a config
    boundary resolving one plane's `[[frame.component]]` tables asks for its own.

    Cheap enough to mean it: six frozen dataclasses and no I/O. The renderers are reached
    lazily from inside each ``render``, so building a registry imports nothing.

    **`fid` is which frame the components will be DRIVEN in, and it is optional because
    most callers never drive them.** `Component.on_event` is handed one event and nothing
    else (§4f), so a handler that has to write down a selection for this frame can only
    have the id by closing over it, and this is where the closing happens. `layout` builds a
    registry at import to read edges and sizes off it, and `instance` builds one to resolve
    a plane's arrangement; neither dispatches an event through what it built, so neither has
    an id to give and neither needs one. `panel._run` — the one caller that DOES build a
    dispatcher — passes it.

    There is deliberately no refusal for the empty default and no guard downstream of it: a
    handler built without an id is one nothing calls, so a check for it would be a line no
    test could turn red, which is the second-weaker-answer shape #568 deleted.
    """
    from . import slots

    reg = Registry()
    # **The two one-row strips declare `click` and nothing else** (#751). `scroll` is not
    # declared for the reason `chats` and `workspaces` do not declare it: a one-row pane
    # has nothing a wheel could move, so its handler could only ever answer False, and
    # `events.Dispatcher.open` charges the same `overlay.MOUSE_ON` for one pointer kind as
    # for two — declaring it would cost nothing and mean nothing, which is exactly what
    # makes it wrong to declare.
    reg.register(Component(
        id="identity", title="identity", edge="top", size=Fixed(1),
        needs=(), render=_panel("top"),
        events=("click",), on_event=_strip_events(fid)))
    reg.register(Component(
        id="attention", title="attention", edge="bottom", size=Fixed(1),
        needs=(), render=_panel("bottom"),
        events=("click",), on_event=_strip_events(fid)))
    # `Content()` with no cap, and `layout.repos_rows` is the cap: the table's height is
    # what the plane's repos need, bounded by what the harness may not be charged
    # (`layout.HARNESS_MIN_ROWS`), which is a fact about the WINDOW rather than about the
    # component. A cap here would be a second, weaker copy of that arithmetic.
    #
    # **The first component in charter that ever declared events** (#607 built the path;
    # nothing consumed it), and still the only one that declares `scroll` — the other five
    # are one-row strips and a column with nothing under it. `scroll` and `click` are the
    # two `frame/events.py` can carry to a pane the pointer is over without moving the
    # keyboard, and they are declared TOGETHER because `events.Dispatcher.open` asks the
    # terminal for one request that serves both — a component declaring only one of them
    # would still pay `overlay.MOUSE_ON`'s whole price.
    #
    # **Declaring them is not a promise they fire**, which is the rule `EVENT_KINDS`
    # states and this is the first place charter itself is bound by it: with `[frame]
    # mouse` off — the shipped default — the harness decides whether the terminal reports
    # at all, so on most planes this handler is never called and the table is exactly what
    # it always was. That is why the selection has a keyboard route as well
    # (`builtin_actions._register_selection`), and why the pane is still readable with no
    # selection at all.
    reg.register(Component(
        id="repos", title="repos", edge="bottom", size=Content(),
        needs=("gather",), render=_panel("repos"),
        events=("scroll", "click"), on_event=_repos_events(fid)))
    # The two parts of the sidebar, registered before the composite that draws them.
    # `personas` is the `Fill()`: the pane is the persona column everywhere else charter
    # names it, so it takes what is left and the todos are capped
    # (`slots._MAX_TODO_LINES`) — `slots._right`'s docstring argues that ordering, and
    # this is the same decision said in the registry's vocabulary rather than a second
    # one.
    # **`personas` declares no events, and it is not an oversight — `registry.Registry`
    # refuses one that does.** It is a PART of the `sidebar` composite below, a part is
    # never placed on the frame (`on_edge`), and `panel._dispatcher` asks `wanted()` of the
    # component that was placed — so a declaration here would pass every check, build a
    # handler and receive nothing, ever. That refusal is #607's own defect one level down
    # and it is deliberate; the composite is what declares, and `slots.CHIPS` is what
    # tells it which of its parts a click was over.
    #
    # The honest cost: a plane that places `personas` on its own through
    # `[[frame.component]]`, instead of the `sidebar` that ships, gets the column drawn
    # and inert. Lifting that means letting a composite delegate a kind to one part, which
    # is a change to the registry's contract and not this one's.
    reg.register(Component(
        id="personas", title="personas", edge="right", size=Fill(),
        needs=(), render=_personas))
    reg.register(Component(
        id="todos", title="todos", edge="right", size=Content(cap=slots._MAX_TODO_LINES),
        needs=("gather",), render=_todos))
    # **A PART of the sidebar and not a pane of its own, and that was measured twice.**
    #
    # First: the frame's sizing supports exactly ONE variable-height pane, by
    # construction. `layout.slot_sizes` answers every member of `VARIABLE_ROW_SLOTS` with
    # `layout.repos_rows`, and `commands_frame._reassert_sizes` leaves that set unasserted
    # so tmux's `resize-pane -y` — which moves exactly one boundary — has one remainder to
    # give the rows to. Registered as a placed `Content()` this was handed the REPO
    # TABLE's height: six rows, for six repos, on a plane with one change.
    #
    # Second, and decisive: a placed component has to be in `instance.FRAME_SLOTS`, and
    # `FRAME_SLOTS`, `FRAME_DEFAULTS["slots"]` and `FRAME_DENSITY["full"]` are pinned to
    # agree — so placing it puts a pane on EVERY operator's frame, saying "no changes in
    # <ws>", for a feature most planes never use. `repos` saying "no clones" is a plane
    # that is broken or new; a plane with no cross-repo change is the ordinary, permanent
    # state.
    #
    # As a section it costs those planes NOTHING: `slots.changes_section` returns no rows
    # at all when there are none, exactly as the todos do, and `_right` only spends a
    # blank row on a section that has something in it.
    #
    # It declares BOTH slices, and both are real reads rather than one being decoration:
    # `changes` is the rows, and `gather` is `gathered_at` — the snapshot's single
    # timestamp, whose AGE the heading draws because a refresh is an action and not a tick
    # (§4g). A surface that showed a state without its age would be indistinguishable from
    # a live one.
    reg.register(Component(
        id="changes", title="changes", edge="right",
        size=Content(cap=slots._MAX_CHANGE_LINES),
        needs=("gather", "changes"), render=_changes))
    # The composite declares the UNION of what its parts read, and `changes` joining the
    # sidebar is what put `changes` on this line. `_panel("right")` draws all three
    # sections through `slots._right`, so a composite declaring less than its parts read
    # would be a cost the frame's budget does not describe — which is the one thing the
    # declaration is for.
    reg.register(Component(
        id="sidebar", title="sidebar", edge="right", size=Fixed(22),
        needs=("gather", "changes"), render=_panel("right"),
        children=("personas", "todos", "changes"),
        events=("click",), on_event=_persona_events(fid)))
    # **Phase 5's two bars, and they are REGISTERED but not PLACED** — the same shape
    # `changes` takes above, and for a stronger version of its argument.
    #
    # `changes` is not in `SLOT_OF` because a plane with no cross-repo change is the
    # ordinary, permanent state and a pane saying so on every frame is a pane earning
    # nothing. A plane with ONE chat is that state too, and the numbers are worse rather
    # than better: measured on this tree, a chat switch is 41 tmux invocations and ~360 ms
    # (3.7c) / ~395 ms (3.2) with four panels, and each placed pane is ~7 of those
    # invocations on the way out and back — so a bar placed by default makes every switch
    # slower for every operator, permanently, to draw a name most of them already know
    # from `top`. The palette reaches every chat in two keystrokes at every width (§3.6:
    # the bar is a readout, not the mechanism), so nothing is unreachable without them.
    #
    # A plane that wants a bar writes a `[[frame.component]]` table — which is also the
    # only way any component gets a toggle key, built-in or not, and a key on the chat bar
    # is exactly what an operator who wants it occasionally should have.
    #
    # Both declare nothing in `needs`: neither renderer goes near `gather`.
    # `slots.chats_bar` reads `.charter/frame/` and `slots.workspaces_bar` reads
    # `workspaces/`, and a declaration naming a slice `ctx` carries would make the frame's
    # cost budget describe a cost that is not there — `identity`'s own reasoning, above.
    #
    # **Both declare `click`, and that is what makes them tab bars rather than captions.**
    # They shipped registered `needs=()` with no `events`/`on_event` at all, so a click
    # reached the pane, was decoded, and landed on no consumer — the report this closes was
    # "I clicked a tab and nothing happened", which is what a readout with no handler looks
    # like from the outside. `_bar_events` is what receives them and why a click here
    # SWITCHES where a click on the table only selects.
    #
    # `click` alone rather than `("scroll", "click")` like `repos`: a bar is one row and
    # there is nothing to scroll it to, so a `scroll` declaration could only reach a
    # handler that always answered False. `repos` declares both because it moves a
    # viewport; the shared reason that constant's comment gives — one `MOUSE_ON` serves
    # both — is why declaring the second one costs nothing, never a reason to declare one
    # that does nothing.
    reg.register(Component(
        id="chats", title="chats", edge="top", size=Fixed(1),
        needs=(), render=_chats,
        events=("click",), on_event=_bar_events(fid, _CHAT_SWITCH)))
    reg.register(Component(
        id="workspaces", title="workspaces", edge="top", size=Fixed(1),
        needs=(), render=_workspaces,
        events=("click",), on_event=_bar_events(fid, _WORKSPACE_SWITCH)))
    return reg
