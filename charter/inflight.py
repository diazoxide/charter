"""In-flight dispatch tracking — the signal the completion tally cannot give.

``personas/_dispatch/`` records a dispatch when it **finishes**, so two dispatches
five minutes apart sequentially are indistinguishable from two that overlapped.
That makes it useless for the one failure it would be worth catching: two
code-writing personas editing the same working tree at once, which fails quietly
— no error, just interleaved edits and whichever commit lands last.

This records a dispatch when it **starts** and clears it when it ends, so overlap
is actually observable.

Local and ephemeral: it lives under the state dir, is never committed, and holds
only an agent name and a timestamp — the same discipline as the committed tally,
which deliberately stores counts and dates, never prompt text.

Everything here is best-effort. A tracker that breaks a turn is worse than one
that misses an overlap.
"""

from __future__ import annotations

import json
import os
import tempfile
import time
from pathlib import Path

# One number used to do both jobs, and doing both is what made it wrong (#308). A record
# past the old TTL was DELETED, so the single most interesting thing this tracker can hold
# — a dispatch that has outlived every reasonable expectation — rendered as nothing at all,
# and irreversibly: "presumed dead" and "never happened" were the same picture. The two
# jobs want opposite horizons, so they get their own numbers.

#: A dispatch still marked in-flight after this long is **presumed dead** — the process was
#: killed, or PostToolUse never fired. Still returned, flagged, and drawn (`45m?`); charter
#: cannot know whether it died or is genuinely still working, only that nobody should still
#: be expecting it. Long enough not to doubt a genuinely slow sub-agent mid-run.
PRESUMED_DEAD_SECONDS = 30 * 60

#: When a record is finally discarded. Far out, because everything before it is a thing a
#: human might still be looking at — but finite, so a stray from a killed process cannot
#: accumulate into a permanent false warning.
PRUNE_SECONDS = 24 * 60 * 60


def _dir() -> Path:
    from . import config
    return config.STATE_DIR / "dispatch-inflight"


def _safe_name(agent: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in agent)[:64]


def live_records(exclude_token: str | None = None) -> list[tuple[str, float, bool]]:
    """``(agent, started_at, presumed_dead)`` per record, duplicates preserved.

    The start time is what separates "two agents are out" from "two agents have been
    out for forty minutes", and only the second is worth interrupting for. It is read
    from the record's own ``ts``, falling back to the file's mtime — the same instant,
    and the only answer available for a record written by a charter that predates the
    field.

    ``presumed_dead`` is measured from that same start time, not from the mtime the
    pruning reads: it is the flag on the age a caller draws, so the two can never
    disagree about which side of the threshold a record sits on. Pruning stays on the
    mtime because it happens *before* the parse — which is what lets a corrupt stray be
    cleaned up at all.

    :func:`live` is a projection of this rather than a second walk of the directory:
    one glob, one set of rules. A caller that wants the names only should keep calling
    it — the extra elements are a cost the aggregate has no use for.
    """
    d = _dir()
    if not d.exists():
        return []
    out: list[tuple[str, float, bool]] = []
    now = time.time()
    for p in d.glob("*.json"):
        try:
            mtime = p.stat().st_mtime
            if now - mtime > PRUNE_SECONDS:
                p.unlink(missing_ok=True)      # a stray, long past anyone watching for it
                continue
            if exclude_token and p.stem == exclude_token:
                continue
            rec = json.loads(p.read_text())
            ts = rec.get("ts")
            started = float(ts) if isinstance(ts, (int, float)) else mtime
            out.append((rec.get("agent") or p.stem, started,
                        now - started > PRESUMED_DEAD_SECONDS))
        except (OSError, TypeError, ValueError):
            continue
    return out


def live(exclude_token: str | None = None) -> list[str]:
    """Agent names the tracker holds — presumed-dead ones included, since the aggregate
    this feeds counts records and the distinction is drawn per chip.

    ``exclude_token`` drops one specific record — the caller's own, so a dispatch
    never reports itself as a concurrent peer.
    """
    return sorted(name for name, _, _ in live_records(exclude_token))


def still_running(exclude_token: str | None = None) -> list[str]:
    """Agent names charter can still claim are *running* — presumed-dead ones dropped.

    For the callers that assert liveness rather than display it. The dispatch nudge says
    a peer "is already running", which stops being true at the presumed-dead threshold;
    keeping the record so a stuck dispatch stays visible must not turn that nudge into a
    nag that outlives the process by a day.
    """
    return sorted(name for name, _, dead in live_records(exclude_token) if not dead)


def start(agent: str) -> str | None:
    """Mark *agent* as in flight; returns an opaque token, or None on any failure."""
    agent = (agent or "").strip()
    if not agent:
        return None
    try:
        d = _dir()
        d.mkdir(parents=True, exist_ok=True)
        # mkstemp, not a timestamped name: two dispatches starting in the same
        # millisecond would collide and the second would overwrite the first —
        # losing exactly the overlap this exists to observe. The agent name stays
        # in the prefix so `finish` can still find its own records.
        fd, path = tempfile.mkstemp(prefix=f"{_safe_name(agent)}.", suffix=".json", dir=d)
        with os.fdopen(fd, "w") as fh:
            json.dump({"agent": agent, "ts": time.time()}, fh)
        return Path(path).stem
    except OSError:
        return None


def finish(agent: str) -> None:
    """Clear one in-flight record for *agent* — the oldest **still-running** one, since a
    repeat dispatch of the same persona should retire the run that started first.

    "Still running" is the qualification records surviving past the presumed-dead
    threshold made necessary. Oldest-first alone would hand a finishing dispatch the
    stuck record to retire and leave its own behind — deleting exactly what #308 exists
    to keep, and leaving a false live one in its place. Presumed-dead records are still
    eligible when there is nothing else, because a genuinely long dispatch does finish
    eventually and its record has to go when it does.
    """
    agent = (agent or "").strip()
    if not agent:
        return
    try:
        now = time.time()
        matches = sorted(_dir().glob(f"{_safe_name(agent)}.*.json"),
                         key=lambda p: p.stat().st_mtime)
        running = [p for p in matches
                   if now - p.stat().st_mtime <= PRESUMED_DEAD_SECONDS]
        for p in (running or matches)[:1]:
            p.unlink(missing_ok=True)
    except OSError:
        return
