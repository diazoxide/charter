"""Who "this session" is — one answer, for everything that keys state by it.

There were three implementations of this, and they disagreed about the case that matters:
what absence means. `workspace._session_id` returned ``None``; `persona._session_id`
returned the string ``"nosession"``. That sentinel is a *shared key*, so every invocation
without a session id wrote into one bucket — and `persona.gc_ephemeral` compared it
against the id of the session it was told to preserve, so when the GC itself ran without
one, ``nosession`` looked like the live session and was skipped. It accumulated forever,
which is the opposite of what "ephemeral" promises.

Absence is now representable: :func:`current` returns ``None`` and callers decide. The
sentinel survives only as :data:`NO_SESSION`, for the one place that genuinely needs a
directory name, and the GC no longer protects it.
"""

from __future__ import annotations

import os
import re

#: Bucket name for state written with no session to attribute it to. A shared key by
#: construction, so it is prunable like any other stale bucket and never treated as live.
NO_SESSION = "nosession"

_SAFE = re.compile(r"[^A-Za-z0-9._-]")


def current(explicit: str | None = None) -> str | None:
    """This session's id, or ``None`` when there is not one.

    ``explicit`` first — the status line receives the id in its stdin payload rather than
    its environment, which Claude Code scrubs. Then ``$CHARTER_SESSION_ID``, which any
    harness sets when it knows its own session (opencode's plugin reads it off
    ``shell.env``'s ``input.sessionID``, per invocation — one server hosts many sessions,
    so nothing may be cached). Then ``$CLAUDE_CODE_SESSION_ID``, kept so no session
    already running regresses the day the neutral name ships.

    Sanitised, because the value becomes a filename.
    """
    raw = (explicit
           or os.environ.get("CHARTER_SESSION_ID")
           or os.environ.get("CLAUDE_CODE_SESSION_ID"))
    if not raw:
        return None
    sid = _SAFE.sub("", raw.strip())
    return sid or None


def bucket(explicit: str | None = None) -> str:
    """A directory name for this session's state — :data:`NO_SESSION` when there is none.

    Use this only where a name is unavoidable. Prefer :func:`current` and handling
    ``None``: a shared bucket mixes two concurrent sessions' scratch, which is the failure
    the per-session split exists to prevent.
    """
    return current(explicit) or NO_SESSION
