"""Which harnesses does charter know, and which one is it running inside?

The same shape as ``charter/forge/registry.py``, for the same reason it records there:
iterating :data:`KINDS` means a harness added to it is covered everywhere the day it is
registered, never a hardcoded literal in `init` or `doctor` that someone has to remember
to update. Adding Codex should be adding a class.
"""

from __future__ import annotations

import os

from .base import Deficit, Harness
from .claude_code import NAME as CLAUDE_CODE, ClaudeCodeHarness
from .codex import NAME as CODEX, CodexHarness
from .opencode import NAME as OPENCODE, OpenCodeHarness

#: Every harness charter knows, by the name it puts in ``$CHARTER_HARNESS``.
KINDS: dict[str, type] = {
    CLAUDE_CODE: ClaudeCodeHarness,
    OPENCODE: OpenCodeHarness,
    CODEX: CodexHarness,
}


def all() -> list[Harness]:
    """One instance of every registered harness, in registration order."""
    return [cls() for cls in KINDS.values()]


def get(name: str | None) -> Harness | None:
    """The registered harness called *name*, or ``None`` for one charter has never met."""
    cls = KINDS.get(name or "")
    return cls() if cls else None


def current() -> str | None:
    """The harness charter is running inside, or ``None`` when nothing says.

    ``$CHARTER_HARNESS`` first, and returned **verbatim** even when unregistered: the
    harness is the authority on its own identity, and a name charter has not met yet is
    information — swallowing it would render as "no harness", which is the failure shape
    this repo keeps paying for. Native evidence (:meth:`Harness.detect`) is the fallback,
    for sessions that started before the plane was wired.
    """
    named = os.environ.get("CHARTER_HARNESS")
    if named:
        return named
    for h in all():
        if h.detect():
            return h.name
    return None


def inherited_paths() -> tuple[str, ...]:
    """Every registered harness's :attr:`Harness.inherited_paths`, once each.

    The plane-root paths a checkout with a git root of its own cuts a session off from —
    what `workspace._inherited_files` mirrors into a guest checkout. Asked of the registry
    rather than listed in `workspace.py`, so a harness registered in :data:`KINDS` has its
    in-repo surface carried into a clone the day it declares one (#868).

    Deduplicated with ``dict.fromkeys`` rather than a set, and both halves of that are
    deliberate. Two harnesses naming the same path must not mirror it twice — the second
    pass would be a second key for one file — and the order has to be the registration
    order, not whatever a hash gives: an answer that comes out of a set is right about half
    the time by luck, which is a guarantee nothing can pin.
    """
    return tuple(dict.fromkeys(p for h in all() for p in h.inherited_paths))


def deficits(name: str | None) -> tuple[Deficit, ...]:
    """What *name* cannot carry.

    Empty for an unregistered harness, which is not a claim that it is complete — it is
    charter having nothing to say. `doctor` distinguishes the two, because "no known
    ceilings" and "no knowledge of this harness" read identically otherwise.
    """
    h = get(name)
    return h.deficits if h else ()
