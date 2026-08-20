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

#: A dispatch still marked in-flight after this long is assumed dead — the process
#: was killed, or PostToolUse never fired. Long enough not to prune a genuinely slow
#: sub-agent mid-run; short enough that strays cannot accumulate into a false warning.
TTL_SECONDS = 30 * 60


def _dir() -> Path:
    from . import config
    return config.STATE_DIR / "dispatch-inflight"


def _safe_name(agent: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in agent)[:64]


def live_records(exclude_token: str | None = None) -> list[tuple[str, float]]:
    """``(agent, started_at)`` per live dispatch, duplicates preserved, stale pruned.

    The start time is what separates "two agents are out" from "two agents have been
    out for forty minutes", and only the second is worth interrupting for. It is read
    from the record's own ``ts``, falling back to the file's mtime — the same instant,
    and the only answer available for a record written by a charter that predates the
    field.

    :func:`live` is a projection of this rather than a second walk of the directory:
    one glob, one pruning rule. A caller that wants the names only should keep calling
    it — the extra element is a cost the aggregate has no use for.
    """
    d = _dir()
    if not d.exists():
        return []
    out: list[tuple[str, float]] = []
    now = time.time()
    for p in d.glob("*.json"):
        try:
            mtime = p.stat().st_mtime
            if now - mtime > TTL_SECONDS:
                p.unlink(missing_ok=True)      # dead: killed process, or no PostToolUse
                continue
            if exclude_token and p.stem == exclude_token:
                continue
            rec = json.loads(p.read_text())
            ts = rec.get("ts")
            out.append((rec.get("agent") or p.stem,
                        float(ts) if isinstance(ts, (int, float)) else mtime))
        except (OSError, TypeError, ValueError):
            continue
    return out


def live(exclude_token: str | None = None) -> list[str]:
    """Agent names currently in flight, stale entries pruned.

    ``exclude_token`` drops one specific record — the caller's own, so a dispatch
    never reports itself as a concurrent peer.
    """
    return sorted(name for name, _ in live_records(exclude_token))


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
    """Clear one in-flight record for *agent* — the oldest, since a repeat dispatch
    of the same persona should retire the run that started first."""
    agent = (agent or "").strip()
    if not agent:
        return
    try:
        matches = sorted(_dir().glob(f"{_safe_name(agent)}.*.json"),
                         key=lambda p: p.stat().st_mtime)
        if matches:
            matches[0].unlink(missing_ok=True)
    except OSError:
        return
