"""Telling a running frame that the plane changed.

Called from `charter/hooks.py`, where the posture is absolute (see that module's own
docstring, and `contain.py`'s): a hook may cost a session its briefing, never its turn.
`posttooluse` fires on every single tool call, so this has two jobs, not one — never
raise, and never cost the hot path anything worth measuring.

A FIFO was considered and rejected: opening one for write blocks until a reader exists,
which would put a hang directly in the hook path the first time no panel was listening.
`state.bump`'s plain version-file-plus-`stat` shape exists because it cannot hang, and
this module rides on top of it rather than inventing a second channel.

The debounce is a plain module-level dict rather than a `threading.Lock`-guarded
counter: each hook invocation is a fresh short-lived `python3 -m charter hook ...`
process (see `hooks.dispatch`), so there is never a second thread in here to race —
only ever a second *call*, later in the same process, when a handler is exercised
directly (as the tests below do).
"""

from __future__ import annotations

import os
import time

from . import state

#: At most one bump per this many seconds. A panel ticks at 0.2s (`panel.TICK`), so a
#: tighter debounce buys nothing a reader could ever see — it would only add cost to a
#: hot path (`posttooluse`, once per tool call) for no visible benefit.
DEBOUNCE = 0.25

#: Mutable through a dict, not a bare module global, so a test can reset it
#: (`notify._last["at"] = 0.0`) without `global` statements on both sides.
_last = {"at": 0.0}


def plane_changed() -> None:
    """Bump the frame this process is running inside, if any. Never raises.

    A no-op outside a frame (`$CHARTER_SESSION_ID` unset — the common case, most
    sessions run with no frame at all) and a no-op inside the debounce window. The
    `except Exception` is not defensive boilerplate: `state.bump` already swallows its
    own `OSError`s, but this also guards `os.environ.get`/`time.monotonic` and any
    future change to either — the one rule this module exists to keep is that NOTHING
    reaching it from a hook ever turns into a raised exception.
    """
    try:
        fid = os.environ.get("CHARTER_SESSION_ID")
        if not fid:
            return
        now = time.monotonic()
        if now - _last["at"] < DEBOUNCE:
            return
        _last["at"] = now
        state.bump(fid)
    except Exception:
        return
