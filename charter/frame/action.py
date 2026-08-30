"""What an action is, and what one is handed when it runs.

A component *draws*; an action *does* (§4d). That one word is the whole of why this
module exists beside `component.py` rather than inside it, and why the contract here is
narrower in the two places where real damage would live.

**Fire-and-report, never blocking** (§4g). :attr:`Action.run` starts work and returns; it
is not where the work happens. `frame.actions.ActionRegistry.invoke` never waits for it,
progress surfaces through `inflight`, and the palette closes. A blocking action in a TUI
is indistinguishable from a hang, and the operator's only recourse would be the escape
hatch — for something working correctly.

**An action that cannot run right now says why** — it is not omitted. The session lock
refuses a workspace switch today, and a palette that silently drops the row is worse than
one that explains: the operator cannot ask about an option they cannot see. That is #512's
"no repos" drawn over a plane that had them, one surface along. So availability is a pair
— :attr:`Action.available` and :attr:`Action.reason_unavailable` — and charter asks the
second one only when the first said no, which is the only arrangement in which the two
cannot contradict each other on screen.

**What an action may reach is narrower than what a component may read.** ``needs`` exists
so a renderer's COST is declarable; ``touches`` exists so an action's REACH is. The
vocabulary is closed, and today it holds no route to a vault value, a forge token or a
shell. ``vault`` is served as an INVENTORY — which vaults this plane has and what they are
keyed by, never what is in them — because an action offering to spend a credential needs
the names to offer, and nothing more to offer them. Spending arrives with the first action
that spends, and it will go through `charter secret exec`, which already resolves the value
inside charter's own process, strips every other vault's identity from the child, and
redacts what comes back. Narrow from day one (§4d): widening costs a line, and un-widening
after a provider has shipped against it costs everything.

**This is containment, not a sandbox**, on exactly the terms `ctx.py` states for
components: a provider's module is ordinary Python and can import whatever it likes. What
the declaration rules out is the accident and the shortcut — the handle that was
convenient once, sitting on the object where a stranger's code reaches for it.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Mapping

from .. import contain
from . import ctx as _ctx
from .component import ID_HINT, names, usable_id

#: The action contract's version, as a single integer, refused at load rather than
#: negotiated (§4g) — the same mechanism `component.API_VERSION` is, on its own number.
#:
#: **Its own number, and a provider declares it under its own name**
#: (``ACTION_API_VERSION``; see `frame.actions.ActionProviders`). One distribution may
#: legitimately supply a CI panel and a "rerun failed job" action out of one module — §4d's
#: own example — and one module attribute cannot mean two contracts whose integers move
#: independently. Sharing it would mean a component-contract change refusing every action
#: on the machine for a contract it did not touch, which is a break with no fix a provider
#: author can apply.
API_VERSION = 1

#: What an action may declare it reaches. Closed, and it is exactly :data:`SERVES` — a
#: name accepted here and served empty would be worse than a refusal, for the reason
#: `component.NEEDS` gives at length: the action would declare it, do nothing, pass its own
#: tests against an empty fixture, and be indistinguishable from a plane that genuinely has
#: nothing. ``forge`` and ``shell`` are absent because charter serves neither yet; each
#: joins the day it can be served, and not before.
TOUCHES = ("gather", "repos", "todos", "vault")


class ActionError(ValueError):
    """An action that cannot be constructed, registered or started — and why.

    One type for the whole contract, so a caller that degrades rather than propagates —
    the palette, which turns every one of these into a row that says why — has one thing
    to catch. Separate from `ComponentError` because the two contracts are separate: a
    caller catching one must not silently swallow the other's failures.
    """


def _always(ctx) -> bool:
    """The default availability: yes.

    A default that answered "no" would need a reason, and a default reason is the
    unfalsifiable row this contract exists to prevent.
    """
    return True


def _silent(ctx) -> str:
    """The default reason: none, because the default action is available.

    Never shown: charter asks this only after :attr:`Action.available` said no, and an
    action that says no has to say why. `frame.actions` finishes the sentence when a
    provider does not.
    """
    return ""


@dataclass(frozen=True, kw_only=True)
class Action:
    """One thing the command surface can do, and everything charter needs to offer it.

    Keyword-only, for `Component`'s reason: field order is not part of the contract and
    must not become part of it by accident, because a provider that spelled its action
    positionally would break the day a field is added, having done nothing wrong.

    Callables rather than methods on a base class, so that an action is a value: a
    built-in is one expression, a provider's is one expression, and neither inherits
    anything charter could later change underneath it.
    """

    #: Held to the component alphabet, by the same function, because it reaches the same
    #: places: a palette row, a pane title, and — once Task 5 lands per-action keys — a
    #: tmux ``bind`` line, which is where `[frame] hotkey`'s newline achieved code
    #: execution at launch.
    id: str
    #: Display text. An open alphabet, contained rather than refused — see `component`'s
    #: docstring for why the pair gets opposite treatments.
    title: str
    #: ``run(ctx)`` — starts the work and RETURNS. Whatever it answers, if anything, is
    #: one line about what it started, shown by the palette; ``None`` is "nothing to say".
    run: Callable[[Any], Any]
    #: ``available(ctx) -> bool`` — may this run right now, against this plane, this
    #: moment. Asked every time the surface is drawn, never cached: the session lock it
    #: most often reports is held and released by other processes.
    available: Callable[[Any], bool] = _always
    #: ``reason_unavailable(ctx) -> str`` — asked ONLY when the line above said no, which
    #: is what keeps the pair from disagreeing where an operator can see it.
    reason_unavailable: Callable[[Any], str] = _silent
    #: What this action reaches, drawn from :data:`TOUCHES`. The ctx is built from it, so
    #: an action gets what it declared and not one field more.
    touches: tuple[str, ...] = ()
    #: Whether this row is the state the frame is already IN — `density: full` on a frame
    #: at `full`, `chrome: off` on a frame with no surface. Drawn as
    #: `frame/overlay.ROW_MARK` in a column every row reserves.
    #:
    #: **A flag and not two characters in the title**, which is #749: `_register_density`
    #: and `_register_chrome` used to compose `"* "` into the title themselves, and every
    #: other action — every doorway, every provider's — composed nothing, so the palette
    #: had two left edges. A bool cannot be spelled a third way by the next row source.
    #:
    #: Static rather than a callable, unlike :attr:`available`: the mark is resolved when
    #: the registry is BUILT (`builtin_actions.build` is handed the density and chrome in
    #: effect), because it describes the moment the palette opened rather than a plane
    #: other processes are moving underneath it.
    mark: bool = False
    #: Whether running this leaves the palette OPEN, with the query and the cursor where
    #: the operator left them, so the next Enter runs it again — #746.
    #:
    #: **For an action whose natural use is repeated, and for nothing else.** `repo:
    #: select the next row` moves a selection by one; with `[frame] mouse = false`
    #: shipped as the default it is the only route the repo table has, and it cost a full
    #: palette open, filter and close per row — three rows measured at three ~3-second
    #: pane cycles. Everything else the palette runs is a thing you do once.
    #:
    #: **What it does NOT relax is fire-and-report.** A repeatable action still starts its
    #: work and returns; what changes is only whether the surface that ran it is torn
    #: down. `commands_frame._draw_palette` still joins on the same grace and still says a
    #: refusal on the operator's own screen.
    repeat: bool = False

    def __post_init__(self) -> None:
        # The id first: every message below names the action it is about, and an action
        # whose id could forge a line would forge those messages too.
        if not usable_id(self.id):
            raise ActionError(
                f"{contain.one_line(repr(self.id))} is not a usable action id — "
                f"write {ID_HINT}")
        if not isinstance(self.title, str):
            raise ActionError(
                f"action {self.id}: title must be text, not "
                f"{contain.one_line(repr(self.title))}")
        for field in ("run", "available", "reason_unavailable"):
            if not callable(getattr(self, field)):
                raise ActionError(
                    f"action {self.id}: {field} must be callable, not "
                    f"{contain.one_line(repr(getattr(self, field)))}")
        object.__setattr__(self, "title", contain.one_line(self.title))
        object.__setattr__(
            self, "touches",
            names("touches", f"action {self.id}", self.touches, TOUCHES,
                  error=ActionError))


class VaultInventory:
    """Which vaults this plane holds and what they are keyed BY — never what is in them.

    **The names are resolved once, when the ctx is built, and the values never are.** An
    action that declared ``vault`` is offering to do something with a credential — copy
    one to a file, run a command under one — and to offer that it needs the names to put
    on a row. It does not need, and does not get, a way to read one.

    :meth:`keys` builds a provider and drops it, deliberately. A cached provider is how a
    value would arrive here without anybody writing a method for it: the object would
    simply be holding the file it had read, and everything reachable from a ctx is
    reachable by the action holding it.

    What this is not: a claim that an action *cannot* read a secret. An action's module is
    ordinary Python and `charter.secrets` is an import away — the same honesty `ctx.py`
    states for components. What is claimed, and tested by walking every route out of a
    real ctx over a real vault, is that **charter hands one over nowhere**: not as a
    value, not as a provider, not in a closure, not behind a method. An action that reads
    a vault does it in the open, in code somebody can review, rather than through a
    convenience charter left on the object.
    """

    def __init__(self, names: tuple[str, ...]) -> None:
        self._names = names

    def names(self) -> tuple[str, ...]:
        """Every vault registered on this plane, alphabetically."""
        return self._names

    def keys(self, vault: str) -> tuple[str, ...]:
        """The key NAMES in *vault* — `VaultProvider.keys` is "never the values".

        A vault this plane does not have is a refusal naming it, not an empty tuple: an
        empty answer is indistinguishable from a vault that holds nothing, and a typo
        would then read as an empty vault (#512, one surface along).
        """
        if vault not in self._names:
            raise ActionError(
                f"no vault named {contain.one_line(repr(vault))} on this plane — "
                f"this action was handed {', '.join(self._names) or 'no vaults'}")
        from ..secrets import registry as vaultreg
        return tuple(vaultreg.provider_for(vault).keys())


def _vault(snap) -> VaultInventory:
    """The vault inventory, read once per ctx.

    Deliberately not a slice of the snapshot: vault registration is not plane state the
    frame gathers on its refresh clock, and putting it there would mean every repaint
    reading it for components that have no business with it. It is filesystem work, which
    is exactly why an action has to DECLARE it — the same argument ``needs`` makes for a
    renderer's cost, applied to an action's reach.
    """
    from ..secrets import registry as vaultreg
    return VaultInventory(tuple(sorted(vaultreg.vaults())))


#: How each declared name is served. The three plane slices are `ctx.SERVES`' own
#: functions, referenced rather than re-written: "what is in ``repos``" must have one
#: answer, and a second copy would be a second answer the day either moved.
SERVES = {
    "gather": _ctx.SERVES["gather"],
    "repos": _ctx.SERVES["repos"],
    "todos": _ctx.SERVES["todos"],
    "vault": _vault,
}

#: What an action is handed whatever it declared. No width and no height: an action does
#: not draw, and a rectangle it was handed would be a rectangle it could be tempted to
#: write into.
IDENTITY = ("fid",)


class ActionCtx(_ctx.Ctx):
    """What an action may reach: `Ctx`'s semantics over this contract's vocabulary.

    Absent rather than disabled, read-only, and an exactly-assertable attribute set — all
    inherited, because they are the same property, and two implementations of "what may a
    stranger's code reach" is the shape #547 is about.

    **Its vocabulary is declared beside it, not on it.** :data:`SERVES` holds `_vault`,
    which reads the vault registry and ignores the snapshot it is handed; as a class
    attribute — even an underscore-prefixed one — it was a live route from the ctx of an
    action that declared NOTHING to this plane's whole vault inventory, past every test
    that asserts the attribute set exactly. `frame.ctx.declare` keeps the table off the
    class; `NoRouteFromAnActionToAVaultValue` walks the class as well as the object.
    """


_ctx.declare(ActionCtx, _ctx.Contract(serves=SERVES, always=IDENTITY, noun="action",
                                      declared="touches"))


def build(touches, *, fid: str, snapshot: Mapping) -> ActionCtx:
    """The object *touches* asked for, cut from *snapshot*.

    Built by the registry from the action's OWN declaration, never by whatever is invoking
    it — so a caller that did not read the declaration cannot hand an action more than it
    asked for, which is the half of "absent, not disabled" a caller could otherwise undo.
    """
    served = names("touches", "this action", touches, tuple(SERVES), error=ActionError)
    if not isinstance(fid, str):
        raise ActionError(
            f"a frame id must be text, not {contain.one_line(repr(fid))}")
    if not isinstance(snapshot, Mapping):
        raise ActionError(
            f"a snapshot must be one mapping of plane state, not "
            f"{contain.one_line(repr(snapshot))}")
    fields: dict[str, Any] = {"fid": fid}
    fields.update({name: SERVES[name](snapshot) for name in served})
    return ActionCtx(fields)
