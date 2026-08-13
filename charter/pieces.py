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
from datetime import datetime, timezone
from pathlib import Path

from . import persona, session, workspace

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
    out = []
    for f in sorted(d.glob("*.jsonl")):
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
