"""What a component is: identity, edge, size policy, declared cost, accepted events.

A slot is a string in a list whose POSITION is secretly geometry. A component is the
thing that replaces it — a named part of the frame that knows its own edge, how big it
wants to be, what it reads and which events it accepts (spec §4). Charter's own panels
consume this seam first, so the extension model has no private back door for them to
drift through (§4b's sequencing note).

**Everything is checked here, at construction, and nowhere else.** A provider meets this
module the moment it builds its component, which is before charter has placed it, before
a pane exists and before anything is on screen. That is the only point at which a
refusal names a fixable thing rather than appearing inside somebody's frame — the same
argument §4g makes for refusing a mismatched :data:`API_VERSION` at load rather than
hoping at render time.

**An id is held to a closed alphabet because it reaches tmux.** `instance._HOTKEY_RE`'s
docstring records what a committed `[frame] hotkey` did: a newline ended the ``bind``
line, ``source-file`` returned 0, and a second tmux command ran at launch with no
keypress. A component id arrives from the same class of place — a committed
`charter.toml` names it, an installed provider declares it — and travels to a menu row
and a pane title. So it is a containment boundary rather than a naming convention, and
:data:`_ID_RE` is deliberately narrower than "whatever Python can hold": an id this
refuses that a provider wanted costs them a rename, and an id this accepted that tmux
parses as a command costs the operator the machine.

A title is the other half of that pair and gets the other treatment. Its alphabet is
open — it is display text, and a provider may legitimately want spaces, punctuation and
non-ASCII in it — so it is *contained* rather than refused, by the same `contain.one_line`
every committed display string already goes through, before it can reach a row it might
otherwise forge a second one of.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Callable

from .. import contain

#: The contract's version, as a single integer, refused at load rather than negotiated
#: (§4g). Charter bumps it when the shape below changes; a provider declaring a different
#: one does not load, and the refusal names the provider, the version it speaks and the
#: version charter speaks. There is no shim band and no best-effort mode: loading-and-
#: hoping means the failure lands at render time inside somebody's frame, where the
#: person seeing it cannot act on it. It also makes the cost of a change real to charter
#: — a bump breaks every provider at once, which is the friction that stops the contract
#: churning.
API_VERSION = 1

#: The sides of the harness pane a component may attach to. A closed set, checked here,
#: because the edge decides which tmux split direction the component costs (`layout`'s
#: `_COLUMN_SLOTS`) and an unknown edge has no answer to that question — it would either
#: pick one silently or drop the component out of the frame without saying so.
EDGES = ("top", "bottom", "left", "right")

#: The event kinds a component may declare, closed by §4f. ``focus`` and ``blur`` are the
#: pair that decision counts as one kind; they are two names here because a component
#: receives one or the other, never "a focus/blur".
#:
#: **``drag`` is absent deliberately and must stay absent.** It is stateful, the hardest
#: to get right across terminals, and the most likely to fight tmux's own selection —
#: and the asymmetry is the whole argument: adding it in a later API version costs
#: nothing, while removing it after a provider has shipped against it costs everything.
#:
#: **Declaring one of these is a declaration of what you HANDLE, never a promise that it
#: FIRES — and for three of the six, charter cannot make that promise.** Written here
#: because this constant is where a provider author meets the question;
#: `docs/frame.md`'s "Mouse is off by default" says the operator's half of the same fact.
#:
#: * ``click`` and ``scroll`` — **charter does not control whether pointer events reach
#:   your pane** (§4i). tmux asks the outer terminal to report the mouse from the ACTIVE
#:   pane's own request alone: with `[frame] mouse` off and the harness pane active,
#:   nothing has asked the terminal to report, so a click on your panel produces no bytes
#:   at all and there is nothing for tmux to route. It is not that charter drops the
#:   event — the event never happens. The harness (Claude Code, or whatever the operator
#:   ran) is the program that decides, and charter does not own it. `[frame] mouse = true`
#:   is the only setting that makes reporting unconditional, and it costs the operator
#:   their terminal's own text-selection (see `instance.FRAME_FIELDS`). **So a component
#:   whose only route to a piece of state is a click has no route to it on most planes.**
#:   Give every pointer affordance a ``key`` as well.
#: * ``focus``/``blur`` — needs tmux's ``focus-events``, which ships OFF.
#:   `commands_frame.conf_text` turns it on for every frame charter launches, so on
#:   charter's own server these fire; inside an operator's existing tmux charter sources
#:   no config at all, and they do not. They also require the operator's terminal
#:   emulator to report focus in the first place, which not all do.
#:
#: The rule that follows, and the one thing this constant is asking of you: **degrade to
#: "never fires", never to "fires wrongly".** A component that treats a missing ``blur``
#: as "still focused" is correct; one that infers focus from elapsed time is not.
EVENT_KINDS = ("key", "click", "scroll", "focus", "blur", "resize")

#: The slices of the plane snapshot a component may declare in ``needs`` — and therefore
#: the complete set of names `ctx.build` can answer with. One snapshot, one timestamp
#: (§4f): everything on screen is from the same moment by construction rather than by
#: luck, so these are slices of a single scan and not a cache per source.
#:
#: **Closed, so that a typo is a refusal rather than a blank pane.** ``ctx`` is built FROM
#: ``needs``; a component declaring ``"gathr"`` would be handed an object with no such
#: attribute and would raise on its first repaint, inside the frame, where the message is
#: a pane rather than a fixable line. Checked here instead, against the one list `ctx`
#: serves from, for the reason `contain.py`'s docstring gives for one implementation over
#: four near-misses: two lists for one concept drift, and the drift is the defect.
#:
#: **These are the slices `gather.scan` actually carries, and no others.** §4f's extended
#: gather adds personas and changes to the same one snapshot, and each joins this tuple
#: *when it can be served* — not before. A name accepted here that `ctx` answered with an
#: empty tuple would be worse than a refusal: a component would declare it, draw nothing,
#: pass its own tests against an empty fixture, and be indistinguishable from a plane
#: that genuinely has no personas. `ctx.SERVES` is asked against this tuple by a test, so
#: the two cannot drift apart in either direction.
NEEDS = ("gather", "repos", "todos")

#: What a refusal tells a provider author to write instead. One sentence, held in one
#: place, because it is repeated into every id refusal and a second spelling of it would
#: eventually describe a different pattern from the one :data:`_ID_RE` enforces.
ID_HINT = ("lower-case letters, digits and underscores, starting with a letter, "
           "optionally namespaced by distribution as `dist.name`")

#: The alphabet an id may be spelled in, and the reason it is this narrow is
#: :data:`ID_HINT`'s call site rather than taste — see this module's docstring.
#:
#: Nothing in ``[a-z0-9_.]`` can end a tmux ``bind`` line, open a quote, start a second
#: command, introduce a tmux format (``#{...}``), traverse a path (``../``) or carry an
#: escape sequence. Case is excluded as well, so two ids cannot differ only by a shift
#: key — a distinction a menu row shows and a reader does not reliably see.
#:
#: The length bound is per segment and is not decoration: an id is repeated into a pane
#: title and a menu row, both of which are width-budgeted surfaces.
#:
#: **Matched with ``fullmatch``, and the first version of this module used ``match`` and
#: was wrong.** Python's ``$`` matches at the end of the string *or just before a
#: trailing newline*, so ``_ID_RE.match("personas\\n")`` succeeds against a pattern that
#: was written to exclude newlines entirely — an id ending in exactly the character this
#: whole constant exists to keep away from tmux config text. `frame_of` already reaches
#: for `fullmatch` on `instance._HOTKEY_RE`, whose pattern carries the same ``^…$``, and
#: that is why the hotkey guard does not have this hole; it is spelled the same way here
#: rather than fixed a second way, so the two guards keep one answer between them.
_ID_RE = re.compile(r"^[a-z][a-z0-9_]{0,31}(?:\.[a-z][a-z0-9_]{0,31})?$")


class ComponentError(ValueError):
    """A component that cannot be constructed, placed or composed — and why.

    One type for the whole contract, so a caller that wants to degrade rather than
    propagate — a config boundary reading `[[frame.component]]`, a discovery pass over
    installed providers — has one thing to catch. It subclasses ``ValueError`` because
    every instance of it is a value that failed a check, and because a caller that
    catches ``ValueError`` around a construction was already right.
    """


def cells(name: str, value: Any, *, allow_none: bool = False, floor: int = 1) -> None:
    """Refuse *value* unless it is a whole number of terminal cells, at least *floor*.

    ``bool`` is excluded explicitly. ``isinstance(True, int)`` is ``True`` in Python, so
    ``Fixed(True)`` would sail through a plain int check and mean ``Fixed(1)`` — a
    fixture VALUE carrying a meaning nobody wrote, which is a defect this suite has now
    caught in four separate shapes.

    *floor* is 1 for a size policy, because a panel nobody can see is a panel nobody
    asked for, and 0 for a measured pane in `ctx`, because tmux can genuinely leave a
    pane with no room at all and a frame draws nothing there rather than refusing. One
    function with a floor rather than two nearly-identical checks: the ``bool`` trap
    above is exactly the kind of thing a second copy is written without.
    """
    if allow_none and value is None:
        return
    if isinstance(value, bool) or not isinstance(value, int) or value < floor:
        raise ComponentError(
            f"{name} must be a whole number of cells, at least {floor}, not "
            f"{contain.one_line(repr(value))}")


@dataclass(frozen=True)
class Fixed:
    """Exactly *n* cells — rows on a horizontal edge, columns on a vertical one."""

    n: int

    def __post_init__(self) -> None:
        cells("a fixed size", self.n)


@dataclass(frozen=True)
class Content:
    """As tall as its own content, never more than *cap* if one is given.

    The policy `layout.repos_rows` already implements for the repo table, named so the
    registry can hold it rather than each caller remembering which slot is the variable
    one.
    """

    cap: int | None = None

    def __post_init__(self) -> None:
        cells("a content cap", self.cap, allow_none=True)


@dataclass(frozen=True)
class Fill:
    """Whatever the parent has left.

    **Exactly one child of a composite may say this**, and more than one is a
    registration-time error rather than a runtime tie-break (§4e) — the registry is where
    that is enforced, because it is the only thing that sees a composite's children
    together. Ambiguity here produces a layout that shifts with the data, which reads as
    a bug every single time.
    """


#: The three policies, as one tuple, so the check below cannot fall out of step with the
#: classes above by being written as three ``isinstance`` arms somebody adds a fourth to.
SIZES = (Fixed, Content, Fill)


def usable_id(value: Any) -> bool:
    """Whether *value* is an id charter will let travel to tmux — see :data:`_ID_RE`.

    Asked rather than re-spelled, because a second copy of this pattern is a second
    answer to "what may reach a `bind` line", and the two would drift the way ``match``
    and ``fullmatch`` already did inside this one module. `frame.action` holds an action
    id to exactly this alphabet for exactly this reason: it reaches a palette row and a
    key binding from the same class of place a component id does.
    """
    return isinstance(value, str) and _ID_RE.fullmatch(value) is not None


def names(name: str, owner: str, value: Any, allowed: tuple[str, ...], *,
          error: type = ComponentError) -> tuple[str, ...]:
    """*value* as a tuple of names drawn from *allowed*, or a refusal naming both.

    A bare string is refused rather than accepted, and that is the point of doing this in
    one helper: ``needs = "gather"`` iterates as six one-character needs, so a permissive
    reading would refuse it six times over for the wrong reason, or — worse, with a
    single-character name in play — accept part of it.

    *error* is which contract is being checked. A component's ``needs`` and an action's
    ``touches`` are the same question asked of two closed vocabularies, and each contract
    raises its own one type so that a caller degrading rather than propagating still has
    one thing to catch.
    """
    if isinstance(value, (str, bytes)) or not isinstance(value, (list, tuple)):
        raise error(
            f"{owner}: {name} must be a tuple of names, not "
            f"{contain.one_line(repr(value))}")
    out = tuple(value)
    unknown = [v for v in out if v not in allowed]
    if unknown:
        raise error(
            f"{owner}: unknown {name} "
            f"{', '.join(contain.one_line(repr(u)) for u in unknown)} — "
            f"charter serves {', '.join(allowed)}")
    return out


@dataclass(frozen=True, kw_only=True)
class Component:
    """One named part of the frame, and everything charter needs to know to place it.

    Keyword-only on purpose. Field order is not part of the contract and must not become
    part of it by accident: ``children`` joins this dataclass for composites, and a
    provider that had spelled its component positionally would break on that addition
    having done nothing wrong.
    """

    id: str
    title: str
    edge: str
    size: Fixed | Content | Fill
    render: Callable[[Any], list[str]]
    needs: tuple[str, ...] = ()
    events: tuple[str, ...] = ()
    #: The leaf ids this component draws inside its own pane, in the order it draws
    #: them, or ``()`` for a leaf. Charter never splits a pane (§4d), so this is how N
    #: things share one — and it is ids rather than components because the rules that
    #: govern it (one level, exactly one `Fill()`, no child claimed twice) need to see
    #: every component together, which only the registry does. Their SHAPE is checked
    #: here; what they refer to is checked at registration.
    children: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        # The id first, because every message below names the component it is about, and
        # a component whose id could forge a line would forge those messages too.
        if not usable_id(self.id):
            raise ComponentError(
                f"{contain.one_line(repr(self.id))} is not a usable component id — "
                f"write {ID_HINT}")
        if not isinstance(self.title, str):
            raise ComponentError(
                f"component {self.id}: title must be text, not "
                f"{contain.one_line(repr(self.title))}")
        if self.edge not in EDGES:
            raise ComponentError(
                f"component {self.id}: {contain.one_line(repr(self.edge))} is not an "
                f"edge — one of {', '.join(EDGES)}")
        if not isinstance(self.size, SIZES):
            raise ComponentError(
                f"component {self.id}: {contain.one_line(repr(self.size))} is not a size "
                f"policy — Fixed(n), Content(cap=None) or Fill()")
        if not callable(self.render):
            raise ComponentError(
                f"component {self.id}: render must be callable, not "
                f"{contain.one_line(repr(self.render))}")
        if isinstance(self.children, (str, bytes)) or \
                not isinstance(self.children, (list, tuple)):
            raise ComponentError(
                f"component {self.id}: children must be a tuple of component ids, not "
                f"{contain.one_line(repr(self.children))}")
        for child in self.children:
            if not usable_id(child):
                raise ComponentError(
                    f"component {self.id}: {contain.one_line(repr(child))} is not a "
                    f"usable component id — write {ID_HINT}")
        object.__setattr__(self, "title", contain.one_line(self.title))
        object.__setattr__(
            self, "needs", names("needs", f"component {self.id}", self.needs, NEEDS))
        object.__setattr__(
            self, "events",
            names("events", f"component {self.id}", self.events, EVENT_KINDS))
        object.__setattr__(self, "children", tuple(self.children))
