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

**This task changed no output, and that is the point.** Each component wraps the renderer
`frame/slots.py` already had, unchanged; the six declarations below are a statement of
what those renderers already do, not a new arrangement. The before/after render at 200x50
and 80x24 is byte-identical.

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

**One of them takes EVENTS, and it is the first thing anywhere that does.** #607 built the
whole path — the decoder, the dispatcher, `DELIVERED` — and shipped it with no consumer:
every one of these six declared `events = ()`, so a release went by in which a provider
could declare `scroll`, pass validation, and be the only thing on the machine receiving
anything. `repos` declares `scroll` and `click` and :func:`_repos_events` receives them,
which makes charter's own panel the worked example rather than the exception — the same
sequencing §4b asks for and the same reason charter's panels went through the component
seam first. What that handler does and does not do is its own docstring; the short of it is
that a click SELECTS and never chooses, because a pointer event can arrive unpaired.

**Registration order is split order** (`registry.Registry`), so the four placed components
are registered in the order charter splits their panes off the harness: identity,
attention, repos, sidebar. The two parts of the sidebar are registered between them,
because the registry refuses a composite whose parts it has not seen — they take split
numbers of their own and are never placed, which `Registry.on_edge` is what enforces.
"""

from __future__ import annotations

from .component import Component, Content, Fill, Fixed
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


#: Which mouse button selects a row. One, and it is named rather than "whatever came":
#: middle-click is paste on every terminal an operator has ever used and right-click opens
#: their emulator's own menu, so acting on either would be charter taking a gesture that
#: already means something else. `overlay._SGR_BUTTONS` is where the three get their names,
#: and the ones §4f named no kind for never arrive here at all.
_SELECT_BUTTON = "left"


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
        if not ev.pressed or ev.name != _SELECT_BUTTON:
            return False
        name = _slots.VIEWPORT.repo_at(ev.row)
        if name is None or name == state.selection(fid):
            return False
        state.record_selection(fid, name)
        state.bump(fid)
        return True

    return on_event


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
    reg.register(Component(
        id="identity", title="identity", edge="top", size=Fixed(1),
        needs=(), render=_panel("top")))
    reg.register(Component(
        id="attention", title="attention", edge="bottom", size=Fixed(1),
        needs=(), render=_panel("bottom")))
    # `Content()` with no cap, and `layout.repos_rows` is the cap: the table's height is
    # what the plane's repos need, bounded by what the harness may not be charged
    # (`layout.HARNESS_MIN_ROWS`), which is a fact about the WINDOW rather than about the
    # component. A cap here would be a second, weaker copy of that arithmetic.
    #
    # **The one component that declares events, and it is the first thing in charter that
    # ever has** (#607 built the path; nothing consumed it). `scroll` and `click` are the
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
        children=("personas", "todos", "changes")))
    return reg
