"""What is on screen, in what order, and which pane owns which component.

One object answers the two questions the frame has always had to answer twice: *which
components are placed on this edge* and *in what order were they placed*. Today the
answer is a list of four strings in `instance.FRAME_SLOTS` whose POSITION is the geometry
— which is why three separate mechanisms grew to change what is on screen (`[frame]
slots`, `[frame] density`, `cmd_density`'s live re-layout), each with its own idea of what
a slot is.

**Registration order IS split order, and it is stored rather than derived.**
`layout.panel_argvs` splits each panel off the harness pane in order, so what a component
is registered AFTER decides how much room is left for it. Measured against tmux 3.7c in a
200x50 window (#386): ``["top", "bottom", "right"]`` gives a **200-column** bottom row,
while ``["top", "right", "bottom"]`` gives **177** — the same three panels, the same
edges, inset beside the side panel instead of running the full width. The spec's
200-versus-**154** is the same measurement of the arrangement charter shipped before #488
retired `left`, where two 22-column sidebars and their borders came off the row instead of
one. The number moved when a panel was retired; the property did not, and the property is
what this module stores.

So :meth:`Registry.split_order` answers with the number the registry assigned at
registration time. Nothing here derives an order from a list position, a sorted mapping or
an edge grouping — a filtered view (:meth:`Registry.on_edge`) is a filter over that one
order and never a renumbering of what is left.

**Composition is one level, and it is arbitrated here.** A component owns a pane and
charter never draws splits inside one (§4d); a composite is how N things share a pane. The
rules — one level only, exactly one child taking what is left, no child claimed by two
parents — need to see every component together, and the registry is the only thing that
does. All three are refused at registration, naming both offenders, because a tie broken
at layout time produces a frame that shifts with the data (§4e) and cannot be reproduced
from the configuration that caused it.
"""

from __future__ import annotations

from .. import contain
from .component import EDGES, Component, ComponentError, Fill


class Registry:
    """The components this frame knows about, in the order they were registered.

    An instance rather than module state, so a caller — a test, a config boundary
    resolving one repo's `[[frame.component]]` tables, a future second frame — builds its
    own and cannot be affected by what another one registered. Module-level mutable state
    shared behind per-caller objects is the isolation-that-is-a-fiction shape `PersonaIso`
    exists for, one layer down.
    """

    def __init__(self) -> None:
        self._by_id: dict[str, Component] = {}
        #: id → the split it is, assigned once and never recomputed. A separate mapping
        #: rather than "the position in ``self._by_id``": dicts preserve insertion order,
        #: so the two agree today and would silently disagree the first time anything
        #: rebuilt, filtered or re-keyed that dict — and the disagreement would be a
        #: geometry change nothing announced.
        self._order: dict[str, int] = {}
        self._next = 1
        #: child id → the composite that draws it. The reason a child leaves
        #: :meth:`on_edge` without leaving the registry.
        self._parent: dict[str, str] = {}

    # -- registering ------------------------------------------------------- #

    def register(self, c: Component) -> Component:
        """Add *c*, at the end of the split order, and answer it.

        Every cross-component rule is checked here, before anything is stored, so a
        refusal leaves the registry exactly as it was: half a composite is a frame with a
        pane whose contents nothing owns.
        """
        if not isinstance(c, Component):
            raise ComponentError(
                f"{contain.one_line(repr(c))} is not a component — build one with "
                f"frame.component.Component(...)")
        if c.id in self._by_id:
            was = self._by_id[c.id]
            raise ComponentError(
                f"two components claim the id {c.id}: {was.title} is registered and "
                f"{c.title} wants the same id. Neither is placed by charter picking "
                f"one — rename one of them")
        if c.children:
            self._check_children(c)
        self._by_id[c.id] = c
        self._order[c.id] = self._next
        self._next += 1
        for child in c.children:
            self._parent[child] = c.id
        return c

    def _check_children(self, c: Component) -> None:
        """The three composition rules, each raising with both offenders named."""
        fills, seen = [], set()
        for cid in c.children:
            if cid in seen:
                raise ComponentError(
                    f"component {c.id} lists {cid} twice — a pane draws each of its "
                    f"parts once")
            seen.add(cid)
            child = self._by_id.get(cid)
            if child is None:
                raise ComponentError(
                    f"component {c.id} is built from {cid}, which is not registered — "
                    f"register the parts before the composite that draws them")
            if child.children:
                raise ComponentError(
                    f"component {c.id} is built from {cid}, which is itself built from "
                    f"other components. Composition is one level: a component is a leaf "
                    f"or a composite of leaves")
            if cid in self._parent:
                raise ComponentError(
                    f"component {c.id} is built from {cid}, which {self._parent[cid]} "
                    f"already draws — one pane draws it, and two claims have no answer")
            if isinstance(child.size, Fill):
                fills.append(cid)
        if len(fills) != 1:
            named = ", ".join(fills) if fills else "none of them"
            raise ComponentError(
                f"component {c.id} needs exactly one part sized Fill() to take what is "
                f"left of its pane, and {named} " +
                ("do" if len(fills) > 1 else "does") + ". Size the others Fixed(n) or "
                f"Content(cap=…)")

    # -- asking ------------------------------------------------------------ #

    def get(self, cid: str) -> Component:
        """The component registered as *cid*.

        Raises `ComponentError` rather than `KeyError` when nothing is registered under
        that id: the id may have come from a committed `charter.toml` naming a provider
        this machine has not installed, which is a message and not a crash (§4b), and a
        caller degrading that way has one type to catch for the whole contract.
        """
        try:
            return self._by_id[cid]
        except (KeyError, TypeError):
            raise ComponentError(
                f"no component is registered as {contain.one_line(repr(cid))}") from None

    def all(self) -> tuple[Component, ...]:
        """Every registered component, composites and their parts alike, in split order.

        A part is still registered — it is drawn by its parent rather than by the frame,
        which is a placement question and not an existence one. A menu row naming it, and
        the intra-pane focus §4e gives charter, both need it to be here.
        """
        return tuple(sorted(self._by_id.values(), key=lambda c: self._order[c.id]))

    def on_edge(self, edge: str) -> tuple[Component, ...]:
        """The components PLACED on *edge*, in split order.

        A composite's parts are absent: their parent draws them inside its own pane, and
        a part that appeared here too would be drawn twice — once in its own pane and
        once inside its parent's.

        An unrecognised edge is refused rather than answered with ``()``. An empty answer
        is indistinguishable from "nothing is placed there", so a typo would be a missing
        panel that nothing anywhere reports.
        """
        if edge not in EDGES:
            raise ComponentError(
                f"{contain.one_line(repr(edge))} is not an edge — one of "
                f"{', '.join(EDGES)}")
        return tuple(c for c in self.all()
                     if c.edge == edge and c.id not in self._parent)

    def children_of(self, cid: str) -> tuple[Component, ...]:
        """The parts *cid* draws, in the order it declared them — ``()`` for a leaf.

        Declared order, not split order: these share one pane, and the frame never split
        it, so the order is the order the composite's own renderer stacks them in.
        """
        return tuple(self.get(child) for child in self.get(cid).children)

    def split_order(self, cid: str) -> int:
        """Which split *cid* is — the number assigned when it was registered.

        Asked rather than inferred. A caller that needs "which split is this" must not
        have to count positions in a list somebody may have filtered first: `on_edge`
        already returns such a list, and the numbers it carries are these.
        """
        self.get(cid)                       # the refusal that names an unknown id
        return self._order[cid]
