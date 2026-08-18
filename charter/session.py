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


#: Environment variables that identify ONE PANE — one shell, so at most one Claude
#: session. Safe to key a pointer on.
_PANE_ID_VARS = (
    "TERM_SESSION_ID",   # iTerm2 / Terminal.app — per pane, survives reopen
    "TMUX_PANE",         # tmux pane
    "STY",               # GNU screen
    "SSH_TTY",           # the pty of this ssh session
)

#: Identifies a WINDOW, which holds many tabs and splits. Listed to be explicit that it
#: is deliberately NOT used — see :func:`terminal`.
_WINDOW_ID_VARS = ("WINDOWID",)


def terminal(explicit: str | None = None) -> str | None:
    """A stable id for the current terminal PANE, or ``None`` when there isn't one.

    Unlike the session id this survives closing and reopening Claude in the same pane,
    which is the whole reason a per-terminal pointer exists. It lives here, beside
    :func:`current`, because "who is this session" and "which pane is it in" are the two
    halves of one question, and this module exists because that question had three
    disagreeing answers.

    ``WINDOWID`` is deliberately absent, and used to sit second in this chain. It
    identifies a *window*, and a window holds many tabs and splits — so every Claude
    session in that window received the same "terminal" id, wrote the same pointer, and
    read each other's. Observed exactly that way: two sessions in one window, one runs
    `charter ws use user-reporting`, and the other — which had no pointer of its own and
    so fell through to the terminal one — silently moved with it. Parallel workspaces are
    the feature; sharing one behind the user's back is the failure it exists to prevent.

    So the rule is: key the pointer on something that identifies a pane, or do not write
    one at all. Returning ``None`` costs only the survives-a-restart convenience, and only
    in terminals that cannot tell their panes apart — the caller then falls to its
    per-session pointer, which Claude Code always provides. An id that is wrong in the
    sharing direction is worse than no id.
    """
    raw = explicit or next(
        (v for v in (os.environ.get(k) for k in _PANE_ID_VARS) if v), None)
    if not raw:
        try:
            raw = os.ttyname(0)              # controlling tty, if stdin is one — per pane
        except Exception:
            raw = None
    if not raw:
        return None
    return _SAFE.sub("-", raw.strip()) or None


def bucket(explicit: str | None = None) -> str:
    """A directory name for this session's state — :data:`NO_SESSION` when there is none.

    Use this only where a name is unavoidable. Prefer :func:`current` and handling
    ``None``: a shared bucket mixes two concurrent sessions' scratch, which is the failure
    the per-session split exists to prevent.
    """
    return current(explicit) or NO_SESSION
