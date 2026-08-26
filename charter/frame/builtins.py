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

**The slot names survive, as a mapping, because they are committed** (:data:`SLOT_OF`).
`[frame] slots = ["top", "bottom", "repos", "right"]` sits in charter.toml on every plane
that has one, `charter panel top --session …` is the argv `layout.panel_command` emits
into a tmux pane, and both are compatibility surfaces. So a slot name is what the outside
world says and a component id is what charter reasons with, with exactly one table
between them rather than a rename rippling through tmux argv, config files and the
renderer registry at once.

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


def build() -> Registry:
    """A registry holding charter's six built-in components, in split order.

    A fresh one per call, deliberately — `registry.Registry`'s own docstring argues it:
    module-level mutable state shared behind per-caller objects is isolation that is a
    fiction. `layout` asks once at import for the edges and sizes it derives; a config
    boundary resolving one plane's `[[frame.component]]` tables asks for its own.

    Cheap enough to mean it: six frozen dataclasses and no I/O. The renderers are reached
    lazily from inside each ``render``, so building a registry imports nothing.
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
    reg.register(Component(
        id="repos", title="repos", edge="bottom", size=Content(),
        needs=("gather",), render=_panel("repos")))
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
    reg.register(Component(
        id="sidebar", title="sidebar", edge="right", size=Fixed(22),
        needs=("gather",), render=_panel("right"),
        children=("personas", "todos")))
    return reg
