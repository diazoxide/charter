"""What charter is *told* about a piece — never what it can see for itself.

A piece is a worktree, and a claim is that worktree coming into existence, so git already
arbitrates who gets it: ``git worktree add`` fails for the loser when two workers race the
same path or branch. Nothing here participates in that. What git cannot answer is whether
the worker considered itself finished — a branch with three commits and no further activity
looks identical whether its worker declared done, hit a denial, or was killed — and this is
where that answer lives (ADR 0011).

**Only what git cannot know.** No branch, no path, no dirty or pushed flag ever appears in a
line here; every one of those is read from git at read time, exactly as :mod:`charter.worktree`
does. The moment a derivable fact is cached here for convenience, ADR 0011 has been reversed
whether or not anyone says so.

**Events, never state.** *"Piece P claimed at T by X"* is a fact about the past and cannot
become false; *"X holds P"* becomes false the instant the worktree is removed by hand. Only
the first form is written. The present tense is reconstructed by joining git's answer about
what exists to this log — which is why there is no "current status" field and may never be.

Shape of the store::

    workspaces/<ws>/pieces/<host>.jsonl      one JSON object per line

Three properties matter, and two are borrowed wholesale from :mod:`charter.dispatch`:

* **Parallel-writer safe.** Lines are small and opened ``O_APPEND``, so a fan-out of eight
  workers appends without a lock.
* **Host in the filename**, so the pattern is unchanged if fleets ever span machines. They
  do not today, which is why the log is not committed — it describes worktrees that exist on
  exactly one disk, and a portable file describing a local reality is the mismatch ADR 0010
  dissects at length.
* **Best-effort.** A failed write returns ``None`` and never raises. The worktree *is* the
  claim; if it exists, the claim happened, and losing the bookkeeping must not undo it.

The workspace is not a field: the log lives inside the workspace directory, so writing the
name into every line would record a fact the path already carries.
"""
from __future__ import annotations

import json
import os
import re
import socket
from datetime import datetime, timedelta, timezone
from pathlib import Path

from . import contain, persona, session, workspace

#: Directory under a workspace holding its piece records. Deliberately absent from
#: ``workspace._live_block`` — see the module docstring.
DIR_NAME = "pieces"

#: Every key a line may carry. A closed set, asserted by the tests: this is where an
#: innocent-looking extra field would quietly reverse ADR 0011.
FIELDS = ("ts", "event", "repo", "piece", "session", "host", "persona", "reason")

#: The whole vocabulary. ``claimed`` is an observation charter makes when it creates the
#: worktree; the other two are the worker's declarations.
#:
#: There is deliberately no ``failed``, ``blocked`` or ``timed-out``. charter can verify
#: none of them, and a state nobody can verify is the marker that lies — a worker that dies
#: declares nothing at all, and that *absence* is what gets reported, with an age. Adding a
#: value here is how this design stops being honest.
EVENTS = ("claimed", "done", "abandoned")

#: The subset a worker declares about itself, as opposed to what charter observed.
DECLARATIONS = ("done", "abandoned")


def _host() -> str:
    """Short, filename-safe hostname — the `dispatch.py` rule, for the same reason."""
    raw = (socket.gethostname() or "unknown").split(".")[0]
    return re.sub(r"[^A-Za-z0-9_-]", "", raw)[:32] or "unknown"


def _now() -> datetime:
    return datetime.now(timezone.utc)


def dir_for(ws: str) -> Path:
    return workspace.workspace_dir(ws) / DIR_NAME


def log_path(ws: str) -> Path:
    return dir_for(ws) / f"{_host()}.jsonl"


def record(ws: str, event: str, repo: str, piece: str, reason: str | None = None,
           when: datetime | None = None) -> Path | None:
    """Append one event. Best-effort: returns ``None`` if it could not be written.

    Swallowing an I/O failure is deliberate. This is bookkeeping about a claim that git has
    already granted — raising there would fail a command whose real work succeeded, and a
    worker cannot un-create its worktree in response.

    An event outside :data:`EVENTS` raises instead, because it is a different kind of
    problem: not a disk that was full, but a caller inventing vocabulary. Swallowing that
    would let a ``failed`` state reach the log and be reported as if charter had verified
    it, which is the whole thing this design refuses to do.
    """
    if event not in EVENTS:
        raise ValueError(f"unknown piece event {event!r} — the vocabulary is {EVENTS}")
    when = when or _now()
    line = {
        "ts": when.isoformat(timespec="seconds"),
        "event": event,
        "repo": repo,
        "piece": piece,
        "session": session.current(),
        "host": _host(),
        "persona": persona.resolve_active(),
    }
    if reason:
        line["reason"] = reason
    p = log_path(ws)
    if contain.write_refusal(p):
        return None  # a committed link at this fixed name — see contain.write_refusal
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        blob = (json.dumps(line, sort_keys=True) + "\n").encode()
        # O_APPEND: concurrent workers interleave safely without a lock.
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        try:
            os.write(fd, blob)
        finally:
            os.close(fd)
        return p
    except OSError:
        return None


def events(ws: str) -> list[dict]:
    """Every recorded event, oldest first.

    A malformed line is skipped rather than fatal: this feeds the status line, which must
    render whatever it finds, and a half-written line from a killed process is exactly the
    kind of thing an append-only log collects.
    """
    d = dir_for(ws)
    if not d.exists():
        return []
    try:
        # Listing an unreadable directory RAISES on Linux and yields nothing on macOS.
        # The divergence is why this needs a guard rather than a comment: a suite green on
        # a developer's Mac went red on CI over exactly this line, and the failure it
        # produced was the status line falling back to `⬢ charter` — the blank footer this
        # module promises never to show.
        files = sorted(d.glob("*.jsonl"))
    except OSError:
        return []
    out = []
    for f in files:
        try:
            text = f.read_text()
        except OSError:
            continue
        for raw in text.splitlines():
            try:
                obj = json.loads(raw)
            except ValueError:
                continue
            if isinstance(obj, dict) and obj.get("piece"):
                out.append(obj)
    return sorted(out, key=lambda e: e.get("ts") or "")


def claims(ws: str) -> dict[tuple[str, str], dict]:
    """The latest ``claimed`` event per piece, keyed ``(repo, piece)``.

    Note what this is *not*: an answer to which pieces exist. A claim here for a worktree
    that was since removed is a true statement about the past and must not put a row in any
    listing — callers start from git and use this only to put a name on what git found.
    """
    out: dict[tuple[str, str], dict] = {}
    for e in events(ws):
        if e.get("event") == "claimed":
            out[(e.get("repo") or "", e["piece"])] = e
    return out


def claim_for(ws: str, repo: str, piece: str) -> dict | None:
    return claims(ws).get((repo, piece))


def declarations(ws: str) -> dict[tuple[str, str], dict]:
    """The latest declaration per piece, keyed ``(repo, piece)``.

    Latest wins, and the earlier ones stay in the log. A worker that declared ``done`` and
    then found it was not done must be able to say so, and the history of it having changed
    its mind is worth more than a record that pretends the first answer never happened.
    """
    out: dict[tuple[str, str], dict] = {}
    for e in events(ws):
        if e.get("event") in DECLARATIONS:
            out[(e.get("repo") or "", e["piece"])] = e
    return out


def declaration_for(ws: str, repo: str, piece: str) -> dict | None:
    return declarations(ws).get((repo, piece))


#: Liveness lives under here, one file per piece, OVERWRITTEN rather than appended.
#:
#: It is written by a hook that fires every turn. Appending it to the log would bury three
#: meaningful lines per piece under thousands and make the log unbounded, so it is a
#: separate store answering a separate question — the log is *what happened*, this is *the
#: latest observation*. Overwriting breaches nothing: "last seen at T" is a past
#: observation, not a cached derivable fact, and there is no reality it can contradict.
#: What it discards is heartbeat history, which nothing needs.
SEEN_DIR = "seen"


#: How long a persona stays counted in ``by`` after its last beat.
#:
#: Without a window the count only ever grows, and ``+3`` decays into "three personas were
#: in here at some point", which is not a fact worth a column. An hour is long enough to
#: cover a pause — a review, a long build, lunch — and short enough that whoever left this
#: morning is no longer presented as company.
PRESENCE_WINDOW = timedelta(hours=1)

#: Hard cap on ``by``, independent of the window. The window alone bounds the map in
#: practice; this bounds it in principle, so a pathological plane cannot grow one heartbeat
#: file without limit.
PRESENCE_KEEP = 8


def seen_path(ws: str, repo: str, piece: str | None) -> Path:
    """Where a tree's heartbeat lives. ``piece=None`` means the clone itself.

    The clone's record is a file *beside* the piece directory, never inside it: a piece
    named like any sentinel we might have used would otherwise collide with the clone's own
    record, and piece names come from branch names, which are not ours to constrain.
    """
    base = dir_for(ws) / SEEN_DIR
    return base / f"{repo}.json" if piece is None else base / repo / f"{piece}.json"


def seen(ws: str, repo: str, piece: str | None, session: str | None = None,
         persona: str | None = None, when: datetime | None = None) -> Path | None:
    """Mark the worker in this tree alive now. Best-effort — never raises, never blocks.

    ``persona`` is *recorded*, not derived. The claim log has carried a persona since ADR
    0011, and joining to it would have needed no new field at all — but that names whoever
    CREATED the piece, and a second persona picking up someone else's piece is the case the
    fleet spine exists for. This records who is *there*; the claim records who *was*.

    The file stays one small overwritten blob. ``by`` keeps the last beat per persona so a
    second worker is counted rather than silently overwritten, pruned by
    :data:`PRESENCE_WINDOW` and capped at :data:`PRESENCE_KEEP`.
    """
    p = seen_path(ws, repo, piece)
    when = when or _now()
    stamp = when.isoformat(timespec="seconds")
    by: dict[str, str] = {}
    if persona:
        prev = last_seen(ws, repo, piece) or {}
        old = prev.get("by")
        if isinstance(old, dict):
            for name, ts in old.items():
                at = _parse(ts if isinstance(ts, str) else None)
                if isinstance(name, str) and at is not None and when - at <= PRESENCE_WINDOW:
                    by[name] = ts
        by[persona] = stamp
        if len(by) > PRESENCE_KEEP:
            keep = sorted(by.items(), key=lambda kv: kv[1], reverse=True)[:PRESENCE_KEEP]
            by = dict(keep)
    blob = {"ts": stamp, "session": session}
    if persona:
        blob["persona"] = persona
        blob["by"] = by
    if contain.write_refusal(p):
        return None  # a committed link at this fixed name — see contain.write_refusal
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(blob, sort_keys=True) + "\n")
        return p
    except OSError:
        return None


def presence(ws: str, repo: str, piece: str | None) -> tuple[str, str, int] | None:
    """``(persona, age, others)`` for the tree, or ``None`` when no persona was recorded.

    An **observation**, never an assertion: charter cannot verify that anyone is working,
    so the caller renders a name and an age and lets the reader draw the conclusion — the
    grammar `silence` already uses, and the reason ADR 0011 has no ``failed`` state.

    ``None`` rather than a guess for a heartbeat written by an older charter, which carries
    no persona at all: those are overwritten within a turn, and a cell that invents a name
    for one turn is worse than a cell that waits.
    """
    rec = last_seen(ws, repo, piece)
    if not rec:
        return None
    who = rec.get("persona")
    if not isinstance(who, str) or not who:
        return None
    at = _parse(rec.get("ts"))
    now = _now()
    by = rec.get("by")
    others = 0
    if isinstance(by, dict):
        for name, ts in by.items():
            seen_at = _parse(ts if isinstance(ts, str) else None)
            if name != who and seen_at is not None and now - seen_at <= PRESENCE_WINDOW:
                others += 1
    return who, _presence_age(at, now), others


def _presence_age(at: datetime | None, now: datetime) -> str:
    """``now`` under a minute, then the coarse age.

    ``0m`` is technically correct and reads as broken; a bare name would be the claim of
    activity charter is not entitled to make.
    """
    if at is None:
        return "?"
    return "now" if (now - at).total_seconds() < 60 else since(at, now)


def last_seen(ws: str, repo: str, piece: str) -> dict | None:
    """The latest observation of this piece's worker, or ``None``.

    A malformed file reads as ``None`` rather than raising. An append-only world collects
    half-written files from killed processes, and this feeds a listing and a status line —
    both of which must render whatever they find.
    """
    try:
        obj = json.loads(seen_path(ws, repo, piece).read_text())
    except (OSError, ValueError):
        return None
    return obj if isinstance(obj, dict) and obj.get("ts") else None


def _parse(ts: str | None) -> datetime | None:
    try:
        return datetime.fromisoformat(ts) if ts else None
    except (TypeError, ValueError):
        return None


def since(when: datetime | None, now: datetime | None = None) -> str:
    """A coarse age — ``3m``, ``5h``, ``3d``. Coarse on purpose: the number is context for
    a human decision, not an input to one charter is making."""
    if when is None:
        return "?"
    secs = max(0, int(((now or _now()) - when).total_seconds()))
    if secs < 3600:
        return f"{secs // 60}m"
    if secs < 86400:
        return f"{secs // 3600}h"
    return f"{secs // 86400}d"


def seen_age(ws: str, repo: str, piece: str) -> str:
    """How long since this piece's worker was observed, falling back to its claim.

    Unlike :func:`silence` this answers even for a piece that has declared an outcome — it
    is "how long ago", not "how long has it said nothing", and the collision warning needs
    the first question.
    """
    mark = last_seen(ws, repo, piece)
    claim = claim_for(ws, repo, piece)
    return since(_parse((mark or {}).get("ts")) or _parse((claim or {}).get("ts")))


def silence(ws: str, repo: str, piece: str) -> str | None:
    """How long this piece has said nothing, or ``None`` if it declared an outcome.

    Measured from the last observation, falling back to the **claim** when the worker never
    got as far as a first turn — that is precisely the case worth seeing, and answering
    "unknown" there would lose it.

    Note what is deliberately absent: any judgement. This returns an age. Whether an age is
    a problem is the reader's call, and no threshold here may ever make it charter's
    (ADR 0009, ADR 0011).
    """
    if declaration_for(ws, repo, piece):
        return None
    claim = claim_for(ws, repo, piece)
    if not claim:
        return None
    mark = last_seen(ws, repo, piece)
    return since(_parse((mark or {}).get("ts")) or _parse(claim.get("ts")))


def outcome(entry: dict | None) -> str:
    """How a declaration reads in a listing. Empty when there is none — which is *silence*,
    not a third outcome, and #98 is what gives it an age."""
    if not entry:
        return ""
    reason = entry.get("reason")
    return f"{entry['event']}: {reason}" if reason else entry["event"]


def claimant(entry: dict | None) -> str:
    """How a claim reads in a listing. ``unknown`` when nobody claimed it — a worktree made
    by hand with plain git is first-class and simply has no claim event."""
    if not entry:
        return "unknown"
    return entry.get("persona") or entry.get("session") or entry.get("host") or "unknown"
