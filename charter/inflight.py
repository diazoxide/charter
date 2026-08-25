"""In-flight work tracking — the signal the completion tally cannot give.

``personas/_dispatch/`` records a dispatch when it **finishes**, so two dispatches
five minutes apart sequentially are indistinguishable from two that overlapped.
That makes it useless for the one failure it would be worth catching: two
code-writing personas editing the same working tree at once, which fails quietly
— no error, just interleaved edits and whichever commit lands last.

This records work when it **starts** and clears it when it ends, so overlap is
actually observable.

**Every record carries a KIND, and #420 is why.** #387 promised the frame "a
spinner while a dispatch, clone or `gl-refresh` runs"; only dispatches animated,
because :func:`start` had exactly one caller. Wiring the other two in was not a
one-liner: the SAME records feed the dispatch-overlap nudge through
:func:`still_running`, and a record named ``clone`` would have made that nudge
tell an operator *"`x` writes code and `clone` are already running"* — wrong, and
wrong in the confident, human-readable way that is worse than silence.

So a record says what it is, and every reader says which kinds it means. The
default everywhere is :data:`DISPATCH`, deliberately: the readers that must not
see a clone (the nudge, the per-persona chips, the session's own ``⚡ N``) get
that by NOT asking, so the next kind somebody invents cannot leak into them by
being forgotten. The frame's spinner is the one caller that opts into "anything
live" (``kind=None``), which is exactly what it is for.

A record written by a charter that predates the field reads as a
:data:`DISPATCH`, which is what it was.

Local and ephemeral: it lives under the state dir, is never committed, and holds
only an agent name, a kind and a timestamp — the same discipline as the committed
tally, which deliberately stores counts and dates, never prompt text.

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


#: A sub-agent handed work by the `Task`/`Agent` tool. What this tracker held for its
#: whole life before #420, and still what every reader means unless it says otherwise —
#: including a record written before the field existed (:func:`_kind_of`).
DISPATCH = "dispatch"

#: One repo being cloned into a workspace (`commands.cmd_clone`, one per repo, so eight
#: parallel clones read as eight).
CLONE = "clone"

#: A forge-state refresh (`commands.cmd_gl_refresh`) — the detached child
#: `glstate.maybe_spawn` starts, not the parent that spawned it.
REFRESH = "gl-refresh"


def _dir() -> Path:
    from . import config
    return config.STATE_DIR / "dispatch-inflight"


def _kind_of(rec: dict) -> str:
    """What kind of work *rec* describes — :data:`DISPATCH` for anything that does not
    say, which is every record this tracker held before #420 and every one written by an
    older charter still sitting on disk. A non-string is treated the same way rather than
    passed through: the value is compared against a caller's filter, and a filter that
    can never match would silently hide a live record from the nudge that needs it."""
    kind = rec.get("kind")
    return kind if isinstance(kind, str) and kind else DISPATCH


def _safe_name(agent: str) -> str:
    return "".join(c if c.isalnum() or c in "._-" else "_" for c in agent)[:64]


def stamp() -> int | None:
    """One ``stat`` that answers "has anything about the tracker changed?" — or ``None``.

    A frame panel wants to ANIMATE while work is in flight and be perfectly still
    otherwise (#387), which means asking this question several times a second, forever,
    on an idle machine. :func:`live_records` is cheap but it is not free: it opens the
    directory and reads every entry in it. This is the cheap half — a single ``stat`` of
    the directory itself, whose mtime moves whenever a record is CREATED or REMOVED,
    which is the only way the set of live records can change. `frame/panel.py` re-reads
    the records only when this number moves.

    ``None`` for "no such directory", which is the common case on a machine that has never
    dispatched — and is a real answer, not an error: nothing can be in flight, and it costs
    the same single failed ``stat`` to learn it.

    The mtime is deliberately NOT enough on its own for one thing, and the caller owns
    that half: a record crossing :data:`PRESUMED_DEAD_SECONDS` changes what a caller
    should say about it while touching no file at all, so `panel._running` also re-reads
    when the earliest such deadline passes. Splitting it that way keeps the IDLE path —
    no records at all, so no deadline either — at exactly one syscall.
    """
    try:
        return _dir().stat().st_mtime_ns
    except OSError:
        return None


def live_records(exclude_token: str | None = None, *,
                 kind: str | None = DISPATCH) -> list[tuple[str, float, bool]]:
    """``(agent, started_at, presumed_dead)`` per record, duplicates preserved.

    *kind* selects which records answer: one of :data:`DISPATCH`/:data:`CLONE`/
    :data:`REFRESH`, or ``None`` for every kind. **It defaults to `DISPATCH`, and that
    default is the guard** — see the module docstring. The frame's spinner is the one
    caller that wants everything; every other reader means dispatches and gets them
    without having to remember to say so.

    The kind is not in the returned tuple. Nothing that filters also needs to display it,
    and a fourth element would have to be threaded through `panel._running`'s cache and
    every test that builds a record by hand for no reader's benefit.

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
            if kind is not None and _kind_of(rec) != kind:
                continue
            ts = rec.get("ts")
            started = float(ts) if isinstance(ts, (int, float)) else mtime
            out.append((rec.get("agent") or p.stem, started,
                        now - started > PRESUMED_DEAD_SECONDS))
        except (OSError, TypeError, ValueError):
            continue
    return out


def live(exclude_token: str | None = None, *,
         kind: str | None = DISPATCH) -> list[str]:
    """Agent names the tracker holds — presumed-dead ones included, since the aggregate
    this feeds counts records and the distinction is drawn per chip.

    ``exclude_token`` drops one specific record — the caller's own, so a dispatch
    never reports itself as a concurrent peer. *kind* is :func:`live_records`'.
    """
    return sorted(name for name, _, _ in live_records(exclude_token, kind=kind))


def still_running(exclude_token: str | None = None, *,
                  kind: str | None = DISPATCH) -> list[str]:
    """Agent names charter can still claim are *running* — presumed-dead ones dropped.

    For the callers that assert liveness rather than display it. The dispatch nudge says
    a peer "is already running", which stops being true at the presumed-dead threshold;
    keeping the record so a stuck dispatch stays visible must not turn that nudge into a
    nag that outlives the process by a day.

    **Its *kind* default is load-bearing rather than tidy.** This is the function whose
    output is read back to an operator as a sentence naming each peer, so a `clone`
    record reaching it produces *"`x` writes code and `clone` are already running"* —
    the wrong-and-confident failure #420 declined to ship. The nudge asks for nothing,
    and therefore gets dispatches.
    """
    return sorted(name for name, _, dead in live_records(exclude_token, kind=kind)
                  if not dead)


def start(agent: str, *, kind: str = DISPATCH) -> str | None:
    """Mark *agent* as in flight; returns an opaque token, or None on any failure.

    *kind* is what a reader filters on — :data:`DISPATCH` unless the caller says
    otherwise, which keeps every pre-#420 call site meaning exactly what it meant.
    """
    agent = (agent or "").strip()
    if not agent:
        return None
    try:
        from . import config

        d = _dir()
        config.private_mkdir(d)
        # mkstemp, not a timestamped name: two dispatches starting in the same
        # millisecond would collide and the second would overwrite the first —
        # losing exactly the overlap this exists to observe. The agent name stays
        # in the prefix so `finish` can still find its own records.
        fd, path = tempfile.mkstemp(prefix=f"{_safe_name(agent)}.", suffix=".json", dir=d)
        with os.fdopen(fd, "w") as fh:
            json.dump({"agent": agent, "kind": kind, "ts": time.time()}, fh)
        return Path(path).stem
    except OSError:
        return None


def finish(agent: str, *, kind: str = DISPATCH) -> None:
    """Clear one in-flight record for *agent* of *kind* — the oldest **still-running**
    one, since a repeat dispatch of the same persona should retire the run that started
    first.

    "Still running" is the qualification records surviving past the presumed-dead
    threshold made necessary. Oldest-first alone would hand a finishing dispatch the
    stuck record to retire and leave its own behind — deleting exactly what #308 exists
    to keep, and leaving a false live one in its place. Presumed-dead records are still
    eligible when there is nothing else, because a genuinely long dispatch does finish
    eventually and its record has to go when it does.

    **The kind is matched, not merely written.** The file NAME carries only the agent, so
    a clone of a repo called ``steward`` and a dispatch to a persona called ``steward``
    glob identically — and whichever finished first would retire the other's record,
    leaving a false live one behind and clearing a true one. Deciding it by reading each
    candidate rather than by renaming the files keeps the on-disk shape unchanged in both
    directions, so a record written by an older charter is still findable and one written
    by this charter is still findable by an older one.
    """
    agent = (agent or "").strip()
    if not agent:
        return
    try:
        now = time.time()
        matches = [p for p in sorted(_dir().glob(f"{_safe_name(agent)}.*.json"),
                                     key=lambda p: p.stat().st_mtime)
                   if _matches_kind(p, kind)]
        running = [p for p in matches
                   if now - p.stat().st_mtime <= PRESUMED_DEAD_SECONDS]
        for p in (running or matches)[:1]:
            p.unlink(missing_ok=True)
    except OSError:
        return


def _matches_kind(p: Path, kind: str) -> bool:
    """Is the record at *p* of *kind*? A file that cannot be read or parsed answers
    **False** — :func:`finish` deletes what this admits, and deleting a record charter
    could not read is deleting something it cannot claim is the caller's own. The
    unreadable file is pruned on its own schedule (:data:`PRUNE_SECONDS`, in
    :func:`live_records`), which is where a stray belongs."""
    try:
        return _kind_of(json.loads(p.read_text())) == kind
    except (OSError, TypeError, ValueError):
        return False
