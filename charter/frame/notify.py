"""Telling a running frame that the plane changed.

Called from `charter/hooks.py`, where the posture is absolute (see that module's own
docstring, and `contain.py`'s): a hook may cost a session its briefing, never its turn.

**Bumping the version and refreshing the gather cache share the one debounce below,
deliberately, not two.** `gather.refresh` is the expensive half (a cold `scan()` costs
~35ms and three git invocations; `state.bump` costs one `os.replace` of a few bytes) but
they exist for the same reason: "the plane changed enough that a panel should redraw
with new facts." A panel only ever looks at the cache *because* the version it polls
moved (`panel.py`'s own contract) — a separate, longer-lived debounce for the cache would
mean the version bumps on every debounce-eligible call while the cache trails behind on
its own slower clock, and a panel would repaint on a version bump into a cache that is
`stat`-fresh but factually stale, exactly the gap this task exists to close. One shared
gate means a panel that just saw the version move always finds a cache gathered at the
same moment, not an older one.

Refresh happens BEFORE the bump, not after, for the same reason: a panel's poll loop
(`panel.TICK`) reads `state.version` first and the cache second, so if the version were
bumped first there would be a window — however small — where a poller sees the new
version and still reads the stale cache. Refreshing first closes that window instead of
leaving it to be usually-too-small-to-matter.

The scan's own expense is bounded twice over, not once: this debounce caps it at once
per 250ms, and inside that, `statusline._repo_states`' 5-second TTL caps the actual `git
status` subprocesses far below even that — so in a burst of tool calls closer together
than 250ms apart, only one in the burst pays anything beyond the cheap
`now - _last["at"] < DEBOUNCE` check, and even that one is usually paying for cache-hit
work (~0.3ms), not a cold sweep.

**Every `posttooluse*` handler calls this, not just the bare-named `posttooluse`.**
`hooks/hooks.json` scopes `posttooluse` itself to `Write|Edit|MultiEdit` — Bash, Skill,
Task/Agent and SendMessage each route to their own handler (`posttooluse-bash`,
`-skill`, `-dispatch`, `-message`). Relying on `posttooluse` alone would leave the frame
blind to Bash specifically, which is where most plane-state changes that matter to a
panel actually happen — commits, branch moves, worktree edits — none of them a
Write/Edit/MultiEdit call. Every handler bumping, rather than picking the ones that seem
to matter today, is the one rule simple enough that a future sixth `posttooluse-*`
handler has an unambiguous answer for whether it should call this too (yes). Each call
site sits behind the same 250ms debounce below, so calling from five handlers instead of
one costs nothing extra on the common path — it only changes which single call in a
quiet stretch is the one that survives the debounce and actually writes.

Being called from five hot paths instead of one changes nothing about what this module
owes them: never raise, and never cost any of them anything worth measuring.

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
#: hot path (every `posttooluse*` handler, so once per Bash/Write/Edit/MultiEdit/Skill/
#: Task/Agent/SendMessage call) for no visible benefit.
DEBOUNCE = 0.25

#: Mutable through a dict, not a bare module global, so a test can reset it
#: (`notify._last["at"] = 0.0`) without `global` statements on both sides.
_last = {"at": 0.0}


def plane_changed() -> None:
    """Bump the frame this process is running inside, if any, and refresh its
    gather cache so panels stay pure readers. Never raises.

    A no-op outside a frame (`$CHARTER_SESSION_ID` unset — the common case, most
    sessions run with no frame at all) and a no-op inside the debounce window. Both
    checks happen before `gather` is even imported, so the common "no frame" path never
    touches the gather module at all, let alone gathers anything.

    The `gather.refresh` call is wrapped in its OWN `try/except`, separate from the
    outer one: `state.bump` must still run — and the version must still move — even if
    the refresh fails (a corrupt cache directory, a `TypeError` in some future field),
    because bumping the version is this function's original, load-bearing promise and a
    test (`test_a_change_bumps_the_running_frame`) already pins it. Losing a cache
    refresh degrades a panel to what it already shows; losing the bump would make a
    panel go stale forever. The outer `except Exception` is not defensive boilerplate
    even so: `state.bump` and `gather.refresh` already swallow their own errors, but
    this also guards `os.environ.get`/`time.monotonic`/the `gather` import itself and
    any future change to any of them — the one rule this module exists to keep is that
    NOTHING reaching it from a hook ever turns into a raised exception.
    """
    try:
        fid = os.environ.get("CHARTER_SESSION_ID")
        if not fid:
            return
        now = time.monotonic()
        if now - _last["at"] < DEBOUNCE:
            return
        _last["at"] = now
        try:
            from . import gather
            gather.refresh(fid)
        except Exception:
            pass
        state.bump(fid)
    except Exception:
        return
