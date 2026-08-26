"""The actions this frame can offer, and starting one without waiting for it.

`registry.py` answers "what is on screen"; this answers "what can be done", and it is the
same object one contract over: the same entry-point discovery, the same four refusals, the
same posture that a stranger's code costs its own row and never the session. What differs
is data — see :class:`ActionProviders`, which sets eight class attributes and inherits
every refusal rather than repeating one of them. #547 is what a second implementation of
one question costs, and it was only found because the copy happened to call the original.

**Two properties here are not negotiable.**

*Fire-and-report* (§4g). :meth:`ActionRegistry.invoke` starts the work and answers an
:class:`Invocation` — a receipt, not a result. It never joins. The action runs on a worker
thread so that a badly-written one costs its own row rather than the frame's input loop: a
blocking action in a TUI is indistinguishable from a hang, and the operator's only recourse
would be the escape hatch, for something working correctly. Progress surfaces through
`inflight`, which is the frame's existing spinner and not a second clock.

*An unavailable action carries why.* Every registered action is offered, always, and one
that cannot run right now is offered with the reason on it (:class:`Offer`). Charter
finishes the sentence when a provider does not — an unavailable row with an empty reason is
the same defect as a missing row, arriving by a shorter path.

**And this is where a failed provider finally has somewhere to speak.** `registry`'s
`STANDIN_EDGE` note records that in Phase 1 an arrangement charter could not honour had no
surface to say so on, so it was refused whole. An action has one: the row itself. A missing
distribution, a version charter does not speak, two distributions claiming one id — each is
an offer that is permanently unavailable and says exactly which, and the rest of the
palette is offered.
"""

from __future__ import annotations

import dataclasses
import threading
from types import MappingProxyType
from typing import Any, Mapping

from .. import contain, inflight
from . import action, registry
from .action import Action, ActionError
from .registry import _because

#: The entry point group an installed distribution declares its actions in (§4d):
#:
#: ``[project.entry-points."charter.actions"]`` / ``acme.deploy = "acme_charter:deploy"``
#:
#: **Separate from `registry.PROVIDER_GROUP`, not one group with a kind field.** A
#: component draws and an action does; the contracts have different shapes, different
#: version integers and different blast radii, and a single group would mean charter
#: importing a provider's module to find out which of the two it had. Discovery must be
#: answerable from metadata alone — that is the whole of why an installed-but-unused
#: provider costs a frame nothing.
ACTION_GROUP = "charter.actions"

#: How much of a reason charter will carry to a row.
#:
#: Larger than `contain.DISPLAY_LIMIT`, which sizes a refusal SENTENCE, because the
#: reasons that matter most here are the load failures below — and each of those ends by
#: naming what to do about it. Clipping at a sentence would take the tail, and the tail is
#: the half the operator can act on. The WIDTH budget is applied where the row is drawn,
#: by the surface that knows how wide it is; this is a bound on how much a provider's
#: string may grow, which is a different question and needs its own number.
REASON_LIMIT = 1024


class ActionProviders(registry.Providers):
    """The action providers installed on this machine, listed without importing one.

    Eight attributes and no methods: everything below is the component loader's, and the
    contract-specific words are data so that the two cannot drift into giving different
    answers. What that buys is concrete — the id-collision refusal, the pre-build version
    check and the "entry point name IS the id" rule hold for actions because they hold at
    all, not because somebody remembered to write them twice.
    """

    group = ACTION_GROUP
    #: **Its own attribute name**, not `API_VERSION`. One module may supply a CI panel and
    #: a "rerun failed job" action — §4d's own example of why the group is separate — and
    #: one name cannot carry two contracts whose integers move independently. A module
    #: declaring only `API_VERSION` has said nothing about this contract, and is refused
    #: for saying nothing rather than loaded on the other contract's word.
    version_attr = "ACTION_API_VERSION"
    speaks = action.API_VERSION
    kind = Action
    contract = "action"
    noun = "frame action"
    verb = "offering"
    degrades = "Its row says so; the rest of the palette is offered"
    too_late = "when you press its key"
    error = ActionError


@dataclasses.dataclass(frozen=True)
class Offer:
    """One row of the command surface: what it is, and whether it can run right now.

    A value rather than a live view of the action, because a row is drawn from a moment.
    Availability is asked once per listing and carried here, so the palette cannot show a
    row as available and refuse it a line later for a reason it never printed.
    """

    id: str
    title: str
    available: bool
    #: Empty exactly when :attr:`available`. Never empty when it is not — charter writes
    #: a sentence when the action would not.
    reason: str


class Invocation:
    """The receipt :meth:`ActionRegistry.invoke` answers: what was started, and how it went.

    **A receipt, not a result.** :attr:`started` and :attr:`reason` are true the instant
    invoke returns; :attr:`note`, :attr:`error` and :attr:`ok` are what the work has said
    SO FAR, and while :attr:`running` they mean "nothing yet" rather than "nothing". A
    caller that renders this renders the present state of the work, which is the whole
    point of the surface being non-blocking.
    """

    def __init__(self, aid: str, *, started: bool, reason: str = "") -> None:
        self.id = aid
        #: Whether the work was started. False for an action that refused to be run —
        #: the reason is on :attr:`reason`, and it is the one the listing would have shown.
        self.started = started
        self.reason = reason
        self._thread: threading.Thread | None = None
        self._note = ""
        self._error = ""

    @property
    def running(self) -> bool:
        """Whether the action is still inside ``run``."""
        return self._thread is not None and self._thread.is_alive()

    @property
    def note(self) -> str:
        """The one line the action said about what it started, contained. ``""`` so far."""
        return self._note

    @property
    def error(self) -> str:
        """What the action raised, as one line naming the type. ``""`` for none so far."""
        return self._error

    @property
    def ok(self) -> bool:
        """Started, finished, and raised nothing. False while it is still running."""
        return self.started and not self.running and not self._error

    def join(self, timeout: float | None = None) -> bool:
        """Wait for the work to finish. **The frame never calls this.**

        It exists for the two callers that are not the frame: a test, which must not tear
        down a fixture the work is still reading, and a shutdown that wants to know
        whether anything is still going. Anything DRAWING this asks :attr:`running` — a
        surface that waited here would be the blocking action this whole contract refuses,
        wearing the caller's clothes.
        """
        if self._thread is None:
            return True
        self._thread.join(timeout)
        return not self._thread.is_alive()

    def _begin(self, a: Action, ctx) -> None:
        """Put the work on a thread of its own and return.

        Here rather than in :meth:`ActionRegistry.invoke` so that the thread and the
        fields it writes are owned by one object: a caller that reached in to start it
        could also reach in to join it, and the whole contract is that nobody joins.
        """
        self._thread = threading.Thread(
            target=self._work, args=(a, ctx), name=f"charter-action-{self.id}",
            daemon=True)
        self._thread.start()

    def _work(self, a: Action, ctx) -> None:
        """The worker thread's whole body: run it, record what it said, clear the record.

        Every exception is recorded rather than raised, `KeyboardInterrupt` included —
        which is where this deliberately differs from `Registry.draw`, and the difference
        is the thread. There, re-raising hands the interrupt back to the operator's own
        main loop; here there is nobody to hand it to, and the default thread excepthook
        would print a traceback over the frame charter is drawing. So it lands on
        :attr:`error`, where the surface can say it.
        """
        try:
            note = a.run(ctx)
            if note is not None:
                self._note = contain.one_line(str(note), limit=REASON_LIMIT)
        except BaseException as exc:
            self._error = _because(exc)
        finally:
            inflight.finish(self.id, kind=inflight.ACTION)


class ActionRegistry:
    """The actions this frame knows about, in the order they were registered.

    An instance rather than module state, for `Registry`'s reason: a test, a config
    boundary, a future second frame each build their own and cannot be affected by what
    another one registered.

    Registration order is the offer order. It is not geometry — an action has no edge —
    but it is still charter's to choose rather than the machine's: `Providers.ids` sorts
    because discovery order is ``sys.path`` order, and a palette that reordered itself
    when a virtualenv changed would be reporting the wrong thing.
    """

    def __init__(self) -> None:
        self._by_id: dict[str, Action] = {}
        #: id → the position it was registered in. A separate mapping rather than "the
        #: position in ``self._by_id``", for `Registry._order`'s reason: dicts preserve
        #: insertion order, so the two agree today and would disagree silently the first
        #: time anything rebuilt or re-keyed that dict.
        self._order: dict[str, int] = {}
        self._next = 1
        #: id → why it is a standin rather than the action that was asked for, kept after
        #: the row is drawn so `doctor` can say it in full at a width a row does not have.
        self._failures: dict[str, str] = {}
        #: One scan of the installed providers per frame, and it has not happened yet.
        self.providers = ActionProviders()

    # -- registering ------------------------------------------------------- #

    def register(self, a: Action) -> Action:
        """Add *a* at the end of the offer order, and answer it.

        Checked before anything is stored, so a refusal leaves the registry exactly as it
        was: a half-registered action is a row that cannot be run.
        """
        if not isinstance(a, Action):
            raise ActionError(
                f"{contain.one_line(repr(a))} is not an action — build one with "
                f"frame.action.Action(...)")
        if a.id in self._by_id:
            was = self._by_id[a.id]
            raise ActionError(
                f"two actions claim the id {a.id}: {was.title} is registered and "
                f"{a.title} wants the same id. Neither is offered by charter picking "
                f"one — rename one of them")
        self._by_id[a.id] = a
        self._order[a.id] = self._next
        self._next += 1
        return a

    def add(self, aid: str) -> Action:
        """Offer *aid* on this frame, importing the provider that supplies it if need be.

        **This never propagates a provider's failure** — a committed config naming
        `acme.deploy` on a machine without it, a provider that raises on import, a version
        charter does not speak, two distributions claiming one id: each is a ROW saying so,
        and the rest of the palette is offered. A committed file must never be able to make
        charter unusable for somebody who has not installed a third-party package.

        Already registered — a built-in, or a config naming one twice — answers what is
        registered rather than refusing: this asks for *aid* to be offered, where
        :meth:`register` asks for an action to be added.

        The one thing it raises for is an id that could never name an action at all: there
        is nothing to stand in for and nothing to write on the row, and that refusal
        belongs to whatever read the value, beside the rest of its validation.
        """
        if isinstance(aid, str) and aid in self._by_id:
            return self._by_id[aid]
        try:
            loaded = self.providers.load(aid)
        except ActionError as exc:
            # An id that could never name an action has nothing to stand in for: the
            # standin below would be refused by `Action` for the same reason, one frame
            # deeper, where the caller can no longer see which value was unusable.
            if not action.usable_id(aid):
                raise
            return self._stand_in(aid, str(exc))
        # Outside the `except`, so that a refusal from `register` — the id-collision
        # rule — is the caller's to see rather than a standin drawn over it.
        return self.register(loaded)

    def _stand_in(self, aid: str, reason: str) -> Action:
        """A row that says why *aid* is not the action that was asked for.

        An action, registered like any other, rather than a hole in the palette. Its
        ``run`` refuses on principle and is unreachable in practice —
        :meth:`invoke` never runs an unavailable action — because a standin that could be
        started by a caller taking a different route would be the one thing worse than a
        missing row: a row that does something nobody can name.
        """
        def refuse(ctx):
            raise ActionError(reason)

        a = self.register(Action(
            id=aid, title=f"{aid} — unavailable", run=refuse,
            available=lambda ctx: False, reason_unavailable=lambda ctx: reason))
        self._failures[aid] = reason
        return a

    @property
    def failures(self) -> Mapping[str, str]:
        """id → why it is a standin, for everything this registry could not load."""
        return MappingProxyType(dict(self._failures))

    # -- asking ------------------------------------------------------------ #

    def get(self, aid: str) -> Action:
        """The action registered as *aid*.

        Raises `ActionError` rather than `KeyError`: the id may have come from a committed
        file naming a provider this machine has not installed, and a caller degrading that
        way has one type to catch for the whole contract.
        """
        try:
            return self._by_id[aid]
        except (KeyError, TypeError):
            raise ActionError(
                f"no action is registered as {contain.one_line(repr(aid))}") from None

    def all(self) -> tuple[Action, ...]:
        """Every registered action, in the order it was registered."""
        return tuple(sorted(self._by_id.values(), key=lambda a: self._order[a.id]))

    def offers(self, *, fid: str, snapshot: Mapping) -> tuple[Offer, ...]:
        """Every action, as rows, with availability resolved against this moment.

        **Every** action: an unavailable one is a row with its reason, never an omission.
        The listing is what the operator can see, so what they cannot do has to be visible
        in it or they cannot find out why.
        """
        out = []
        for a in self.all():
            available, reason, _ = self._check(a, fid=fid, snapshot=snapshot)
            out.append(Offer(id=a.id, title=a.title, available=available, reason=reason))
        return tuple(out)

    def invoke(self, aid: str, *, fid: str, snapshot: Mapping) -> Invocation:
        """Start *aid* and answer the receipt. **Returns without waiting for it** (§4g).

        Availability is re-asked here rather than trusted from the listing: the plane
        moves — the session lock is held and released by other processes — and a row drawn
        a second ago is not a promise. An action that cannot run is not run, and the
        receipt carries the same reason the row would have.

        The record goes into `inflight` BEFORE the thread starts, so the frame's spinner
        cannot miss a fast action: a record written from inside the worker could land after
        the work was already over.
        """
        a = self.get(aid)
        available, reason, ctx = self._check(a, fid=fid, snapshot=snapshot)
        if not available:
            return Invocation(aid, started=False, reason=reason)
        inv = Invocation(aid, started=True)
        inflight.start(aid, kind=inflight.ACTION)
        inv._begin(a, ctx)
        return inv

    # -- availability, in one place -------------------------------------- #

    def _check(self, a: Action, *, fid: str,
               snapshot: Mapping) -> tuple[bool, str, Any]:
        """``(available, reason, ctx)`` for *a* against this moment.

        One implementation, asked by both :meth:`offers` and :meth:`invoke`, so the row
        and the keypress cannot disagree about whether something can run — and one ctx,
        built once, so an action that declared `vault` costs one registry read rather than
        one per surface that asks about it.

        A provider's `available` that RAISES answers no, with what it raised: a stranger's
        code costs its own row, exactly as a renderer that raises costs its own pane. The
        alternative is an exception out of the listing, which costs the whole palette.

        `KeyboardInterrupt` still travels, as it does through `Registry.draw`: this runs on
        the frame's own thread, and an interrupt swallowed here is a palette that cannot be
        stopped while it is being listed.
        """
        try:
            ctx = action.build(a.touches, fid=fid, snapshot=snapshot)
            available = bool(a.available(ctx))
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            return False, self._reason(a, _because(exc)), None
        if available:
            return True, "", ctx
        try:
            said = a.reason_unavailable(ctx)
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            said = _because(exc)
        return False, self._reason(a, said), ctx

    @staticmethod
    def _reason(a: Action, said: Any) -> str:
        """*said* as one row's worth of why, and never nothing.

        Contained BEFORE anything measures or draws it (#472): a reason is a provider's
        string, or a plane value quoted into one, and a newline in it writes a second row
        that looks exactly as much like charter's own as the first. `[frame] hotkey` is
        what a committed value reaching a config line cost once, and a palette row is the
        same class of place.

        An empty answer is finished by charter rather than passed through. "Unavailable,
        no reason" is the row this contract exists to prevent, and a provider returning
        ``""`` produces it just as surely as omitting the row would.
        """
        text = contain.one_line(str(said or ""), limit=REASON_LIMIT).strip()
        return text or (
            f"{a.id} is unavailable and did not say why — an action that refuses has to "
            f"name what would make it available")
