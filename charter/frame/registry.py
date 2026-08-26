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

**And this is the seam a stranger's code arrives through.** A committed `charter.toml`
says which components to place, where and in what order; it never says what code runs
(§4b). The code comes from a distribution the operator installed, found through
:data:`PROVIDER_GROUP` — so a cloned repo whose config names `acme.metrics` on a machine
without it draws a pane saying exactly that, and cannot make anything run. That is the
whole safety argument for binding by NAME rather than by a `command = "…"` string, which
would be executable content in a committed file: `[frame] hotkey` and #453 are what that
costs.

Four rules hold the seam, and each of them is a refusal rather than a best effort:

* **A mismatched API version does not load** (§4g). One integer, named on both sides.
* **Two providers claiming one id load NEITHER** (§4h), because a pane whose origin
  cannot be determined is a debugging problem with no entry point.
* **A provider that raises costs its own pane** — on import, while building, or in
  ``render`` — and the pane says which component failed and why. A blank pane is the
  confidently-wrong output the left sidebar was retired for.
* **Charter contains what a provider returned**, after it returns it. A provider renders
  committed values — repo names, branches, todo text — and its lines reach a terminal, so
  escaping and the width budget are applied here rather than trusted to the code that
  needed containing.
"""

from __future__ import annotations

import dataclasses
import importlib
from importlib import metadata
from types import MappingProxyType
from typing import Any, Mapping

from .. import contain, tui
from .component import (API_VERSION, EDGES, Component, ComponentError, Content,
                        Fill)

#: The entry point group an installed distribution declares its components in (§4b):
#:
#: ``[project.entry-points."charter.components"]`` / ``acme.metrics = "acme_charter:metrics"``
#:
#: `importlib.metadata` is stdlib, so this leaves ``dependencies = []`` untouched, and it
#: is deliberately not a harness plugin group: the frame is harness-agnostic and a
#: provider must work under Claude Code, codex and opencode alike, so it binds to charter.
PROVIDER_GROUP = "charter.components"

#: The edge and size a component charter could not load is stood in for on, when the
#: caller does not say. A caller that read `[[frame.component]]` DOES know — it passes the
#: configured pair, and :func:`_rectangle` applies it to the loaded component and to the
#: standin alike, so a machine missing one provider draws the same rectangles as a machine
#: that has it, with a message in the one pane instead of the frame's geometry shifting
#: under everything else.
#:
#: **No such caller exists yet, and that is deliberate** (§4b property 4, narrowed
#: 2026-08-26). `instance.component_tables` refuses an arrangement naming a component
#: charter cannot place, whole, rather than dropping the one placement — because every
#: step from there to a painted pane still speaks the four committed SLOT NAMES
#: (`layout._derive`, `layout.panel_command`'s `charter panel <slot>` argv,
#: `frame/panel.py:run`), so a placement dropped here would be a panel silently absent
#: with no pane to say why. Phase 2 builds the surface; this is what will be passed
#: across it.
STANDIN_EDGE = "bottom"
#: Capped rather than open: the message is charter's, but the reason inside it quotes a
#: provider's exception text, and an unbounded panel whose height an installed package
#: chooses is a budget the input can grow.
STANDIN_SIZE = Content(cap=6)

#: How many characters of ONE returned line charter will escape, measure and clip.
#:
#: `tui.truncate` bounds a line's VISIBLE WIDTH, and visible width is not a bound on
#: length: a combining mark measures zero cells, so a line of a million of them fits any
#: pane and costs the repaint a million characters. `contain.one_line` bounds the
#: characters, `tui.truncate` bounds the cells, and neither substitutes for the other.
#:
#: Larger than `contain.DISPLAY_LIMIT`, which sizes a refusal SENTENCE. This sizes a pane
#: row, and every invisible in it is replaced by a four-to-six character escape, so a
#: legitimately full 300-column row of them is still well inside this and only a line
#: nobody could read is cut.
LINE_LIMIT = 4096


def _where(ep) -> str:
    """The distribution behind entry point *ep*, named the way a refusal must name it.

    Every refusal below names the PROVIDER and not only the id, because the id came out
    of a committed file the operator may not have written and the distribution is the
    thing they can install, uninstall or file a bug against.
    """
    dist = getattr(ep, "dist", None)
    name = getattr(dist, "name", None) or "an unnamed distribution"
    version = getattr(dist, "version", None)
    return contain.one_line(f"{name} {version}" if version else str(name))


def _because(exc: BaseException) -> str:
    """A raised thing as one line of a refusal: its type, and what it said.

    The type is kept because a provider's message is frequently empty — ``KeyError('x')``
    stringifies to ``'x'`` and ``StopIteration`` to nothing at all — and "it failed"
    without a name is the message that sends the reader to a traceback charter does not
    have. No traceback: this reaches a pane a few cells wide, and a provider's file paths
    are not the operator's to read out.
    """
    return f"{type(exc).__name__}: {contain.one_line(str(exc))}"


def _wrap(text: str, width: int) -> list[str]:
    """*text* as lines of at most *width* CELLS, split at spaces where it can be.

    Measured with `tui.width`, never `len`: a refusal quoting a component id with a wide
    character in it would otherwise be laid out a column too long and clipped exactly
    where the thing it names is.

    A word wider than the pane is cut rather than left to be clipped, because the clip
    would take the tail with it — and the tail of a refusal is the half that says what to
    do about it.
    """
    if width <= 0:
        return []
    out: list[str] = []
    line = ""
    for word in text.split(" "):
        while tui.width(word) > width:
            head = tui.truncate(word, width, ellipsis="")
            if not head:                        # one character wider than the pane
                break
            if line:
                out.append(line)
                line = ""
            out.append(head)
            word = word[len(head):]
        if not line:
            line = word
        elif tui.width(line) + 1 + tui.width(word) <= width:
            line = f"{line} {word}"
        else:
            out.append(line)
            line = word
    if line:
        out.append(line)
    return out


def _fit(lines, *, width: int, height: int, escape: bool) -> tuple[str, ...]:
    """*lines* as at most *height* rows of at most *width* cells each.

    ``escape`` is provenance and not a preference — see :meth:`Registry.draw`.

    `contain.one_line` runs BEFORE the width arithmetic, which is the order the rest of
    charter uses for the same reason: an escape sequence is a zero-width instruction to
    the terminal, so measuring first would measure a string that is not what the terminal
    is about to do, and clipping first would cut an escape in half.
    """
    out = []
    for line in lines[:max(height, 0)]:
        if escape:
            line = contain.one_line(line, limit=LINE_LIMIT)
        out.append(tui.truncate(line, width))
    return tuple(out)


def _rectangle(c: Component, *, edge, size) -> Component:
    """*c* in the rectangle the caller asked for, or in its own where the caller did not.

    **The one place either half of a rectangle is resolved**, and that is the whole
    point of it being a function rather than two expressions. `Registry.place` has two
    ways to answer — the component a provider supplied, and the standin drawn when it
    could not be — and a rectangle resolved separately on each is a rectangle that can
    differ between them. It did: the loaded component kept its own `edge` and `size` and
    the configured pair reached only the standin, so one committed `[[frame.component]]`
    table drew a panel on `right` on a machine with the provider installed and on `top`
    on a machine without it, with `on_edge` disagreeing to match.

    ``None`` is the only spelling of "the caller did not say". Not falsiness: ``edge =
    ""`` in a committed file is a value somebody wrote and charter cannot honour, and
    quietly substituting a default for it is the config key that changes nothing this
    phase was written against. `Component`'s own validation is what refuses it, here,
    where the caller can still be told which value was unusable.

    A frozen dataclass replaced rather than mutated, so nothing that already holds *c* —
    a provider's own module-level singleton, most obviously — sees the frame's
    arrangement written back into it.
    """
    if edge is None and size is None:
        return c
    return dataclasses.replace(c, edge=c.edge if edge is None else edge,
                               size=c.size if size is None else size)


class Providers:
    """The component providers installed on this machine, listed without importing one.

    **Listed, then imported one at a time, and never the other way round.** The entry
    point NAME is the component id, so charter answers "is `acme.metrics` installed, and
    by what" from distribution metadata alone; a provider's module is imported at the
    moment its component is actually placed and at no other moment. An installed provider
    the operator has not placed therefore costs a frame nothing — no import, no module,
    no code of theirs run — which is what makes it safe for a machine to carry several.

    That is also why the entry point name and the component's own id must be the same
    word, and why :meth:`load` refuses a provider where they differ. The name is what a
    committed `charter.toml` places and what charter resolves before importing anything;
    if the loaded component could answer to a different id, a config naming one thing
    would draw another — and a provider could take over a built-in's name by declaring an
    entry point that does not mention it.

    One instance per frame, scanning once: entry point discovery walks ``sys.path``, and
    doing it per component would put a directory walk on the repaint path §4's whole
    cost argument exists to keep off it.
    """

    def __init__(self) -> None:
        self._entries: dict[str, tuple] | None = None

    def _scan(self) -> dict[str, tuple]:
        """id → every entry point claiming it. More than one IS the collision (§4h)."""
        if self._entries is None:
            found: dict[str, list] = {}
            for ep in metadata.entry_points(group=PROVIDER_GROUP):
                found.setdefault(ep.name, []).append(ep)
            self._entries = {name: tuple(eps) for name, eps in found.items()}
        return self._entries

    def ids(self) -> tuple[str, ...]:
        """Every component id an installed provider claims, alphabetically.

        Sorted rather than in discovery order, because discovery order is ``sys.path``
        order — a property of the machine, not of the plane — and a menu or a `doctor`
        line that reordered itself when a virtualenv changed would be reporting the
        wrong thing. Split order is the registry's, and it comes from placement.
        """
        return tuple(sorted(self._scan()))

    def supplies(self, cid: str) -> bool:
        """Whether anything installed claims *cid* — asked without importing it."""
        return isinstance(cid, str) and cid in self._scan()

    def load(self, cid: str) -> Component:
        """The component *cid* names, imported now, or a `ComponentError` saying why not.

        Every refusal here is one a caller is expected to DEGRADE rather than propagate —
        `Registry.place` turns each into a pane — so they are written to be read by the
        operator in a few cells, and each one names the distribution.
        """
        shown = contain.one_line(repr(cid))
        if not isinstance(cid, str) or "." not in cid:
            raise ComponentError(
                f"nothing on this frame is named {shown}, and no installed provider can "
                f"be: a provider's id is namespaced by its distribution, like "
                f"acme.metrics. Charter's own components are registered before any "
                f"config is read, so a bare name that is not one of them is a typo")
        eps = self._scan().get(cid, ())
        if not eps:
            raise ComponentError(
                f"no installed provider supplies the component {contain.one_line(cid)} — "
                f"install the distribution that declares it in the {PROVIDER_GROUP} "
                f"entry point group, or stop placing it. The rest of the frame is drawn")
        if len(eps) > 1:
            raise ComponentError(
                f"{len(eps)} installed providers supply the component "
                f"{contain.one_line(cid)}: {', '.join(_where(ep) for ep in eps)}. "
                f"Charter loads {'NEITHER' if len(eps) == 2 else 'NONE of them'} — "
                f"picking one by load order would draw a pane whose origin cannot be "
                f"determined. Uninstall one of them")
        return self._one(eps[0], cid)

    def _one(self, ep, cid: str) -> Component:
        """The single claimant, imported, version-checked, built and identified."""
        module_name = contain.one_line(str(getattr(ep, "module", "") or ""))
        value = contain.one_line(str(getattr(ep, "value", "") or ""))
        try:
            module = importlib.import_module(ep.module)
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            raise ComponentError(
                f"{_where(ep)} supplies {contain.one_line(cid)}, and importing "
                f"{module_name} raised {_because(exc)}. Its pane says so; the rest of "
                f"the frame is drawn") from None

        # Before the entry point's own attribute is even looked up, let alone called: a
        # version charter cannot honour is refused at LOAD, where the message names a
        # fixable thing, rather than hoped through to render time inside somebody's frame
        # (§4g). There is no shim band and no best-effort mode.
        speaks = getattr(module, "API_VERSION", None)
        if isinstance(speaks, bool) or not isinstance(speaks, int):
            raise ComponentError(
                f"{_where(ep)} supplies {contain.one_line(cid)} and {module_name} "
                f"declares API_VERSION = {contain.one_line(repr(speaks))}. Charter "
                f"speaks component API version {API_VERSION}, as one integer, so this "
                f"provider is not loaded")
        if speaks != API_VERSION:
            raise ComponentError(
                f"{_where(ep)} supplies {contain.one_line(cid)} for component API "
                f"version {speaks}, and charter speaks {API_VERSION}. It is not loaded: "
                f"a contract charter cannot honour fails at render time inside your "
                f"frame, where you cannot act on it")

        obj: Any = module
        try:
            for part in [p for p in (getattr(ep, "attr", "") or "").split(".") if p]:
                obj = getattr(obj, part)
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            raise ComponentError(
                f"{_where(ep)} supplies {contain.one_line(cid)} as {value}, and "
                f"{module_name} has no such name: {_because(exc)}") from None

        # A Component or something that answers one. Both, because the spec's own example
        # entry point (`acme_charter.metrics:Component`) reads as an object and a factory
        # is the obvious other spelling — refusing either would refuse a provider author
        # for a choice that changes nothing charter can observe.
        if not isinstance(obj, Component):
            if not callable(obj):
                raise ComponentError(
                    f"{_where(ep)} supplies {contain.one_line(cid)} as {value}, which is "
                    f"{contain.one_line(repr(obj))} — an entry point names a frame "
                    f"component, or something charter can call with no arguments to get "
                    f"one")
            try:
                obj = obj()
            except KeyboardInterrupt:
                raise
            except BaseException as exc:
                raise ComponentError(
                    f"{_where(ep)} supplies {contain.one_line(cid)}, and building it "
                    f"raised {_because(exc)}") from None
            if not isinstance(obj, Component):
                raise ComponentError(
                    f"{_where(ep)} supplies {contain.one_line(cid)}, and calling {value} "
                    f"answered {contain.one_line(repr(obj))} rather than a frame "
                    f"component")
        if obj.id != cid:
            raise ComponentError(
                f"{_where(ep)} declares the entry point {contain.one_line(cid)} and "
                f"answers a component whose id is {contain.one_line(obj.id)}. The entry "
                f"point name is what a committed charter.toml places and what charter "
                f"resolves without importing anything, so the two must be one word")
        return obj


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
        #: The ids whose LINES charter did not write — everything that arrived through a
        #: provider, and every standin, whose text quotes one. See :meth:`draw`.
        self._foreign: set[str] = set()
        #: id → why it is a standin rather than the component that was asked for. Kept
        #: after the pane is drawn, so `doctor` can say it in full at a width a pane does
        #: not have.
        self._failures: dict[str, str] = {}
        #: One scan of the installed providers per frame, and it has not happened yet:
        #: `Providers` lists nothing until something is placed.
        self.providers = Providers()

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

    # -- placing what config named ----------------------------------------- #

    def place(self, cid: str, *, edge: str | None = None,
              size: Any = None) -> Component:
        """Put *cid* on this frame, importing the provider that supplies it if need be.

        **This never propagates a provider's failure**, and that is the point of it
        (§4b): a committed config naming `acme.metrics` on a machine without it, an
        installed provider that raises on import, a version charter does not speak, two
        distributions claiming one id — each of those is a PANE saying so, and the rest
        of the frame is drawn. A committed file must never be able to make charter
        unusable for somebody who has not installed a third-party package.

        *edge* and *size* are the arrangement, **and the arrangement wins** (§4b:
        *arrangement is committed, execution is local*). They are applied to the
        component that LOADED exactly as they are to the standin, through one function
        (:func:`_rectangle`), so the two paths cannot answer differently: the frame a
        machine missing one provider draws is the frame the other machine draws, with a
        message in that one pane. The alternative — a provider's own `edge` overruling
        the committed table — is the same committed file drawing two different frames on
        two machines, and it inverts the principle the extension model rests on. §4b's
        `default_edge`/`default_size` are for a sensible arrangement BEFORE anyone
        configures one; they do not beat one that exists.

        Without them a loaded component keeps its own declaration and a standin takes
        :data:`STANDIN_EDGE` and :data:`STANDIN_SIZE` — "the caller did not say" is
        ``None`` and only ``None``, in both directions.

        Already registered — a built-in, or a provider placed twice by a config listing
        it twice — answers what is registered rather than refusing: this asks for *cid*
        to be on the frame, where `register` asks for a component to be added. A
        component keeps the rectangle it went onto the frame in, because `Component` is
        frozen and a split order already assigned is geometry that has been spent.

        The one thing it does raise for is an id, an edge or a size that could never
        name a component or a rectangle at all: there is nothing to stand in for and
        nowhere to draw the message, and that refusal belongs to whatever read the value,
        beside the rest of its validation. It raises the same way whether or not the
        provider is installed, which is the same symmetry the paragraph above is about.
        """
        if isinstance(cid, str) and cid in self._by_id:
            return self._by_id[cid]
        try:
            c = self.register(
                _rectangle(self.providers.load(cid), edge=edge, size=size))
        except ComponentError as exc:
            return self._stand_in(cid, str(exc), edge=edge, size=size)
        # AFTER the registration that could still have refused it, so a component that
        # was not placed is not recorded as one whose lines charter contains.
        self._foreign.add(c.id)
        return c

    def _stand_in(self, cid: str, reason: str, *, edge, size) -> Component:
        """A pane that says why *cid* is not the component that was asked for.

        A component, registered like any other, rather than a hole in the frame or a
        skipped split. A missing panel that nothing anywhere explains is the
        confidently-wrong output #512 was about: the operator sees a frame that looks
        deliberate and has no way to find out it is not.

        Built at charter's own defaults and then put through :func:`_rectangle`, the one
        the loaded component goes through — rather than resolving the pair a second time
        here. Two spellings of "the caller did not say" is how the two paths came to
        disagree in the first place.
        """
        c = self.register(_rectangle(Component(
            id=cid, title=f"{cid} — not drawn", edge=STANDIN_EDGE,
            size=STANDIN_SIZE, needs=(), events=(),
            render=lambda ctx: _wrap(reason, ctx.width)), edge=edge, size=size))
        self._failures[cid] = reason
        self._foreign.add(cid)
        return c

    @property
    def failures(self) -> Mapping[str, str]:
        """id → why it is a standin, for everything this registry could not load."""
        return MappingProxyType(dict(self._failures))

    # -- drawing ----------------------------------------------------------- #

    def draw(self, cid: str, ctx) -> tuple[str, ...]:
        """The rows *cid* drew — bounded by its own rectangle, and never raising.

        **A component that raises costs its own pane and never the session** (§4b). Every
        way a renderer can fail lands here: an exception, a return that is not lines, a
        line that is not text. The pane then NAMES the component and what it raised,
        because a pane that goes blank when a component breaks is indistinguishable from
        one whose plane has nothing to show — and this project has shipped that defect
        once already (#512).

        `KeyboardInterrupt` is the one thing that still travels: the operator's own
        interrupt is theirs, and swallowing it would mean a frame that cannot be stopped
        during a repaint. This is containment, not a sandbox, and nothing here pretends
        otherwise — a provider's module is ordinary Python.

        **Escaping is decided by who wrote the line, not by the caller.** A provider's
        rows are escaped: they carry committed plane values — repo names, branch names,
        todo text — and they reach a terminal, where an escape sequence is an instruction
        and not a character, so a row of one could move the cursor out of its own
        rectangle and draw over the pane beside it. Charter's own renderers already
        contain their committed values at the point they interpolate them and then add
        charter's own colour markup on top, so escaping their output here would corrupt
        charter's markup while protecting nothing. The width and the row count are
        applied to both: the rectangle is the frame's, whoever drew inside it.
        """
        c = self.get(cid)
        try:
            lines = c.render(ctx)
            if isinstance(lines, (str, bytes)) or not isinstance(lines, (list, tuple)):
                raise ComponentError(
                    f"render must answer a list of lines, not "
                    f"{contain.one_line(repr(lines))}")
            for line in lines:
                if not isinstance(line, str):
                    raise ComponentError(
                        f"render answered {contain.one_line(repr(line))}, which is not a "
                        f"line of text")
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            return _fit(_wrap(f"{contain.one_line(cid)} failed to draw — "
                              f"{_because(exc)}", ctx.width),
                        width=ctx.width, height=ctx.height, escape=True)
        return _fit(lines, width=ctx.width, height=ctx.height,
                    escape=cid in self._foreign)

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
