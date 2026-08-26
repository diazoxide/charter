"""What a component is handed when it draws: geometry, identity, and its declared slices.

**Constructed FROM ``needs``** (§4e). A component receives an object holding exactly what
it declared plus the fixed geometry, and asking for anything else raises with the name of
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

from types import MappingProxyType
from typing import Any, Mapping

from .. import contain
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

#: The keys every component gets whatever it declared: how much room it has, and which
#: frame it is drawing in. Named once so :func:`build` and the tests that assert the
#: attribute set exactly are reading the same list.
GEOMETRY = ("width", "height", "fid")


class Ctx:
    """One repaint's worth of what one component may read.

    No public methods, deliberately. Everything reachable by name on this object is a
    field that was declared or a piece of geometry, which is what lets a test assert the
    attribute set *exactly* — and that assertion is the point: a future field is a
    widening of what a stranger's code may reach, and it should cost a test change and
    the conversation that goes with it.

    **The four class attributes below are what a second contract changes.** An action is
    handed an object with exactly these semantics — absent rather than disabled, read-only,
    an exact attribute set — over a different vocabulary (`frame.action.ActionCtx`), and
    writing that twice would be two answers to "what may a stranger's code reach". Each is
    underscore-prefixed because the exactness assertion reads ``dir()``, and a public class
    attribute would BE an attribute a component can reach.
    """

    #: name → how that name is cut out of the one snapshot.
    _serves = SERVES
    #: The names served whatever was declared.
    _geometry = GEOMETRY
    #: What a refusal calls the thing holding this ctx.
    _noun = "component"
    #: The field it declares what it is handed in.
    _declared = "needs"

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
        shown = contain.one_line(repr(name))
        if name in self._serves:
            raise AttributeError(
                f"this {self._noun} did not declare {name}: add it to the "
                f"{self._noun}'s {self._declared} to be handed it")
        raise AttributeError(
            f"a {self._noun} ctx has no {shown} — charter serves "
            f"{', '.join(self._geometry)} and the declared {self._declared} "
            f"({', '.join(self._serves)})")

    def __setattr__(self, name: str, value: Any) -> None:
        """A ctx is what this repaint was handed, not a place to keep state.

        A composite's parts share a pane and draw one after another; a writable ctx would
        be a channel between them that nothing declared and nothing bounds.
        """
        raise AttributeError(
            f"a {self._noun} ctx is read-only; {contain.one_line(repr(name))} cannot "
            f"be set")

    def __delattr__(self, name: str) -> None:
        raise AttributeError(
            f"a {self._noun} ctx is read-only; {contain.one_line(repr(name))} cannot be "
            f"removed")


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
    fields = {"width": width, "height": height, "fid": fid}
    fields.update({name: SERVES[name](snapshot) for name in served})
    return Ctx(fields)
