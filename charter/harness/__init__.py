"""Which harness charter is running inside, and what that harness cannot carry.

A *harness* is the agent runtime — Claude Code, opencode — as distinct from a *host*,
which in this codebase is a forge. ADR 0015.

The harness names itself in the environment: opencode's generated plugin sets
``$CHARTER_HARNESS`` through its ``shell.env`` hook, Claude Code sets it statically from
``.claude/settings.json``'s ``env``. Everything else asks here, so "which harness am I?"
is answered once at the edge rather than by sniffing runtime-specific variables in every
function that needs to know.
"""

from __future__ import annotations

from .base import Deficit, Harness
from .claude_code import NAME as CLAUDE_CODE
from .opencode import NAME as OPENCODE
from .registry import KINDS, all, current, deficits, get

__all__ = ["CLAUDE_CODE", "OPENCODE", "Deficit", "Harness", "KINDS",
           "all", "current", "deficits", "get"]
