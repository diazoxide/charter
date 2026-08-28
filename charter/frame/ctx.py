"""What a component is handed when it draws: geometry, identity, chrome, and its
declared slices.

**Constructed FROM ``needs``** (§4e). A component receives an object holding exactly what
it declared plus what every component is served (:data:`ALWAYS`), and asking for
anything else raises with the name of
the thing to add to ``needs``. That is what makes the idle-cost property — one ``stat``
per panel per tick, which `frame/panel.py` was built around — survive code charter did not
write: today it is verified by a reviewer reading charter's own renderers, and a reviewer
cannot read a provider's.

**Absent, not disabled.** A slice that was not declared is not an attribute holding
``None``, an empty stand-in or a handle that raises when used. It is not there at all. A
present-but-empty attribute is indistinguishable from a slice that happens to be empty,
so a component could ship reading nothing and nobody would find out until an operator's
pane was blank — which is the confidently-wrong output the left sidebar was retired for.

**One snapshot, one timestamp** (§4f). Every slice below comes out of the same scan, so
"everything on screen is from the same moment" is true by construction rather than by two
caches happening to agree. A component never gets a way to fetch a fresher one: refreshing
is the frame's decision, on the frame's clock.

**What this is not.** It is not a sandbox, and nothing here pretends otherwise: a
provider's module is ordinary Python and can import the standard library like any other.
What ``ctx`` rules out is the accident and the shortcut — a filesystem handle, a
subprocess factory or a client sitting on the object because it was convenient once,
which a renderer then reaches for and whose cost lands on every operator's tick. The
declaration is what makes that cost visible in review and in config, rather than
discovered in a profile.
"""

from __future__ import annotations

from dataclasses import dataclass
from types import MappingProxyType
from typing import Any, Mapping

from .. import contain
from . import chrome
from .component import ComponentError, cells, names


def _rows(key: str):
    """A snapshot list served as a tuple — see :data:`SERVES`."""
    return lambda snap: tuple(snap.get(key) or ())


#: How each declared name is cut out of the one snapshot. This mapping IS the vocabulary
#: `component.NEEDS` names — a test asks each against the other, so a slice cannot be
#: served without being declarable, or declared without being served.
#:
#: **A list arrives as a tuple and a mapping as a read-only view.** One snapshot is
#: shared by every component in a repaint, so a component that appended to a list, or
#: cleared it, would be editing what the next one is about to draw. The containment is
#: SHALLOW and this says so plainly: a repo row inside ``repos`` is still a mutable dict,
#: and making it otherwise would cost a deep copy per component per tick — which is the
#: exact budget this module exists to protect. What is bought here is that the SHAPE of
#: the frame's data cannot be rewritten under another component's feet.
#:
#: ``gather`` is the whole scan because that is what today's renderers read — `_bottom`,
#: `_repos` and `_right` each take `gather.read(fid)` entire. It is a slice like the
#: others rather than a back door: a component that declared ``todos`` does not get it.
SERVES = {
    "gather": lambda snap: MappingProxyType(snap),
    "repos": _rows("repos"),
    "todos": _rows("todos"),
}

#: The keys every component gets whatever it declared: how much room it has, which frame
#: it is drawing in, and how to look like it belongs there. Named once so :func:`build`
#: and the tests that assert the attribute set exactly are reading the same list.
#:
#: **``chrome`` is served here rather than through :data:`SERVES`, and that is the whole
#: of why this constant is no longer called ``GEOMETRY``.** Every name in `SERVES` is a
#: slice cut out of the ONE SNAPSHOT — that is what the mapping's values are, functions of
#: the snapshot — and the recipes are not in it. Serving them as a `SERVES` entry would
#: mean a callable that takes the snapshot and ignores it, which is precisely the shape
#: :class:`Ctx`'s own docstring records as the defect that let an action reach the plane's
#: whole vault inventory off its ctx's class. So it is served unconditionally, with the
#: geometry, and `SERVES` keeps meaning exactly one thing.
#:
#: Unconditional and not declarable, which `ctx`'s "absent, not disabled" doctrine would
#: otherwise argue against: the doctrine is there to keep the IDLE COST visible — a slice
#: nobody declared is a `gather.read` nobody pays for — and these are constant strings
#: resolved without reading anything. There is no cost for a declaration to make visible,
#: and a component forced to declare `chrome` before it could match charter's own weight
#: would be a declaration that bought nobody anything.
ALWAYS = ("width", "height", "fid", "chrome")


@dataclass(frozen=True)
class Contract:
    """One ctx class's vocabulary: what it serves, and what it calls its holder.

    A value rather than four class attributes, because it is looked up as a unit and
    because a second contract must differ from the first in all four or in none.
    """

    #: name → how that name is cut out of the one snapshot.
    serves: Mapping[str, Any]
    #: The names served whatever was declared.
    always: tuple[str, ...]
    #: What a refusal calls the thing holding this ctx.
    noun: str
    #: The field it declares what it is handed in.
    declared: str


#: class → its :class:`Contract`. **Module state, and that is the whole point** — see
#: :class:`Ctx`. Nothing reachable from a ctx object reaches this dict: a method's
#: ``__globals__`` does, but that is this module, and reaching a module is ``import``,
#: which this contract has never claimed to prevent.
_CONTRACTS: dict[type, Contract] = {}


def declare(cls: type, contract: Contract) -> Contract:
    """Register *cls*'s vocabulary — the only way a ctx class gets one."""
    _CONTRACTS[cls] = contract
    return contract


def contract_of(obj) -> Contract:
    """The contract *obj*'s class was declared with, or its nearest base's.

    The walk up ``__mro__`` is what lets `frame.action.ActionCtx` inherit `Ctx`'s
    behaviour while declaring its own vocabulary, and a class nobody declared is a
    charter bug named at the moment it would otherwise answer with somebody else's words.
    """
    for cls in type(obj).__mro__:
        got = _CONTRACTS.get(cls)
        if got is not None:
            return got
    raise TypeError(
        f"{type(obj).__name__} is a ctx class nobody declared a contract for — "
        f"call frame.ctx.declare() beside the class")


class Ctx:
    """One repaint's worth of what one component may read.

    No public methods, deliberately. Everything reachable by name on this object is a
    field that was declared or one of :data:`ALWAYS`, which is what lets a test assert the
    attribute set *exactly* — and that assertion is the point: a future field is a
    widening of what a stranger's code may reach, and it should cost a test change and
    the conversation that goes with it.

    **A second contract changes the vocabulary, not the semantics.** An action is handed
    an object with exactly these — absent rather than disabled, read-only, an exact
    attribute set — over a different vocabulary (`frame.action.ActionCtx`), and writing
    that twice would be two answers to "what may a stranger's code reach".

    **That vocabulary is not ON this class, and that is a fix rather than a style.** It
    was four underscore-prefixed class attributes, which looked private and were nothing
    of the sort: ``type(ctx)._serves["vault"]`` is a callable that reads the vault
    registry and IGNORES the snapshot handed to it, so an action that declared nothing at
    all reached the plane's whole vault inventory — the names, and every key name in each
    — straight off its own ctx's class. Every test that asserts the attribute set exactly
    filtered names starting with ``_`` and saw nothing wrong. So the table lives in
    :data:`_CONTRACTS`, keyed by class, and `dir(ctx)`, `vars(ctx)` and every class in
    ``type(ctx).__mro__`` now carry no callable of charter's at all.
    """

    def __init__(self, fields: Mapping[str, Any]) -> None:
        # Straight into ``__dict__``, because ``__setattr__`` below refuses everything.
        self.__dict__.update(fields)

    def __getattr__(self, name: str) -> Any:
        """Only reached when the attribute is genuinely absent — and it says why.

        Two different mistakes, told apart, because they need different fixes: a slice
        charter serves that this component did not ask for, and a name nothing serves at
        all. Answering both with a bare ``AttributeError`` would leave a provider author
        guessing which of the two they had made.
        """
        c = contract_of(self)
        shown = contain.one_line(repr(name))
        if name in c.serves:
            raise AttributeError(
                f"this {c.noun} did not declare {name}: add it to the "
                f"{c.noun}'s {c.declared} to be handed it")
        raise AttributeError(
            f"a {c.noun} ctx has no {shown} — charter serves "
            f"{', '.join(c.always)} and the declared {c.declared} "
            f"({', '.join(c.serves)})")

    def __setattr__(self, name: str, value: Any) -> None:
        """A ctx is what this repaint was handed, not a place to keep state.

        A composite's parts share a pane and draw one after another; a writable ctx would
        be a channel between them that nothing declared and nothing bounds.
        """
        raise AttributeError(
            f"a {contract_of(self).noun} ctx is read-only; "
            f"{contain.one_line(repr(name))} cannot be set")

    def __delattr__(self, name: str) -> None:
        raise AttributeError(
            f"a {contract_of(self).noun} ctx is read-only; "
            f"{contain.one_line(repr(name))} cannot be removed")


declare(Ctx, Contract(serves=SERVES, always=ALWAYS, noun="component",
                      declared="needs"))


def build(needs, *, width: int, height: int, fid: str, snapshot: Mapping) -> Ctx:
    """The object *needs* asked for, cut from *snapshot* at *width* x *height*.

    Every argument is checked before anything is served, and each refusal is a
    `ComponentError` — the one type the whole contract raises, so a caller that draws a
    failed component's message in its own pane rather than losing the session (§4b) has
    one thing to catch.

    A key the snapshot does not carry is served as its empty value rather than refused:
    `gather.read` answers ``{}`` for a frame whose cache has not been written yet, and a
    panel at that moment must draw something. That is a missing *reading*, which is the
    frame's normal cold state — not a missing *declaration*, which is the mistake the
    attribute error above exists for.
    """
    served = names("needs", "this component", needs, tuple(SERVES))
    cells("a pane's width", width, floor=0)
    cells("a pane's height", height, floor=0)
    if not isinstance(fid, str):
        raise ComponentError(
            f"a frame id must be text, not {contain.one_line(repr(fid))}")
    if not isinstance(snapshot, Mapping):
        raise ComponentError(
            f"a snapshot must be one mapping of plane state, not "
            f"{contain.one_line(repr(snapshot))}")
    fields = {"width": width, "height": height, "fid": fid,
              # Resolved per pane rather than carried, because `colour_ok` is a question
              # about THIS process's stdout and `NO_COLOR` is a question about the
              # operator's environment right now — a mapping built once at import would
              # answer both from whatever was true when charter was first imported.
              "chrome": chrome.recipes()}
    fields.update({name: SERVES[name](snapshot) for name in served})
    return Ctx(fields)
