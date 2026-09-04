"""Committed dispatch tally — how often each persona is actually *used*.

Roster health used to be judged by memory volume alone, which is blind to the failure
that matters: a persona that exists, advertises a `delegate-when`, lints green, and is
**never dispatched** while the work it owns routes to a generic sub-agent. Memory counts
can't see that; only dispatch counts can.

Shape of the store::

    personas/_dispatch/<YYYY-MM>.<host>.jsonl     one JSON object per line

Three properties matter:

* **Counts and dates only.** A line is ``{"ts": …, "agent": …}`` for a dispatch, or
  ``{"ts": …, "event": "advice"}`` for routing advice having been shown — never the
  prompt, the description, or any tool input. There is no secret surface to scan, and the
  second shape was added under exactly that constraint: it records that the roster
  appeared, never what it appeared about.
* **Parallel-writer safe.** Lines are small and opened ``O_APPEND``, so concurrent
  dispatches (a fan-out of 8 sub-agents) append atomically without a lock.
* **Conflict-free across engineers.** The filename carries the host, so two people
  recording the same month never touch the same file — the committed tally merges by
  addition instead of conflicting.

Generic agents (``general-purpose``, ``Explore``) are tallied too, under their own name:
the ratio of persona-to-generic dispatch is the whole point, and dropping it would hide
the very gap this store exists to expose.
"""
from __future__ import annotations

import json
import os
import re
import socket
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path

from . import config, contain

DIR_NAME = "_dispatch"
#: Agents that aren't personas — tallied so the persona-vs-generic ratio stays visible.
GENERIC = ("general-purpose", "Explore", "claude", "Plan")


def _dir() -> Path:
    return config.PERSONAS_DIR / DIR_NAME


def _host() -> str:
    """Short, filename-safe hostname — keeps each engineer on their own file."""
    raw = (socket.gethostname() or "unknown").split(".")[0]
    return re.sub(r"[^A-Za-z0-9_-]", "", raw)[:32] or "unknown"


def _now() -> datetime:
    return datetime.now(timezone.utc)


def path_for(when: datetime | None = None) -> Path:
    when = when or _now()
    return _dir() / f"{when:%Y-%m}.{_host()}.jsonl"


def record(agent: str, when: datetime | None = None) -> Path | None:
    """Append one dispatch. Best-effort: a tally must never break a turn, so any failure
    is swallowed and reported as None."""
    agent = (agent or "").strip()
    if not agent:
        return None
    when = when or _now()
    p = path_for(when)
    if contain.write_refusal(p):
        return None  # a committed link at this fixed name — see contain.write_refusal
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps({"ts": when.isoformat(timespec="seconds"), "agent": agent},
                          sort_keys=True) + "\n"
        # O_APPEND: concurrent sub-agent dispatches interleave safely without a lock.
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        try:
            os.write(fd, line.encode())
        finally:
            os.close(fd)
        return p
    except OSError:
        return None


#: Marks a row that records ROUTING ADVICE being shown rather than a dispatch happening.
#: Same store, same two-fields-only discipline: a timestamp and this word. The pair
#: "advice shown" vs "dispatches that followed" is the only number that can falsify the
#: bet the roster block rests on — that seeing the roster changes routing — and a bet
#: shipped without it repeats the gap this module's docstring was written about.
ADVICE = "advice"


def record_advice(when: datetime | None = None) -> Path | None:
    """Append one 'routing advice was shown' event. Best-effort, like :func:`record`."""
    when = when or _now()
    p = path_for(when)
    if contain.write_refusal(p):
        return None  # a committed link at this fixed name — see contain.write_refusal
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps({"event": ADVICE, "ts": when.isoformat(timespec="seconds")},
                          sort_keys=True) + "\n"
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        try:
            os.write(fd, line.encode())
        finally:
            os.close(fd)
        return p
    except OSError:
        return None


def advice_tally(days: int | None = None) -> int:
    """How many times routing advice was shown, optionally within the last *days*."""
    rows = [o for o in _read_all() if o.get("event") == ADVICE]
    if days is not None:
        cutoff = _now() - timedelta(days=days)
        rows = [o for o in rows if (t := _ts(o)) and t >= cutoff]
    return len(rows)


#: Marks a row where work was handed to a persona that was ALREADY running — a resume,
#: not a new sub-agent. Its own kind because `DISP` answers "times dispatched as a
#: sub-agent", and folding resumes into that inflates the answer to a question nobody
#: asked, in the column people retire personas on.
RESUME = "resume"


def record_resume(agent: str, when: datetime | None = None) -> Path | None:
    """Append one 'more work handed to a running persona' event. Best-effort.

    `posttooluse_dispatch` fires on Task/Agent, so it sees a sub-agent being CREATED and
    nothing after. Continuing that agent is delegation the tally could not see: on the day
    the roster block shipped, one persona cut two releases and ran a sweep, and the sweep —
    a resume — was invisible to the pair of numbers that exist to measure exactly that.
    """
    agent = (agent or "").strip()
    if not agent:
        return None
    when = when or _now()
    p = path_for(when)
    if contain.write_refusal(p):
        return None  # a committed link at this fixed name — see contain.write_refusal
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps({"agent": agent, "event": RESUME,
                           "ts": when.isoformat(timespec="seconds")}, sort_keys=True) + "\n"
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        try:
            os.write(fd, line.encode())
        finally:
            os.close(fd)
        return p
    except OSError:
        return None


def resume_tally(days: int | None = None) -> int:
    rows = [o for o in _read_all() if o.get("event") == RESUME]
    if days is not None:
        cutoff = _now() - timedelta(days=days)
        rows = [o for o in rows if (t := _ts(o)) and t >= cutoff]
    return len(rows)


def first_advice() -> datetime | None:
    """When routing advice was FIRST shown here, or ``None`` if it never has."""
    stamps = [t for o in _read_all() if o.get("event") == ADVICE and (t := _ts(o))]
    return min(stamps) if stamps else None


def handoffs_since_first_advice() -> int:
    """Dispatches recorded at or after the first advice — the only count that can honestly
    sit beside :func:`advice_tally`.

    The pair is meant to read as fired-vs-followed, and a dispatch that happened before the
    roster ever appeared cannot have followed it. Pairing advice against the LIFETIME total
    is how this line came to claim, on the plane that shipped the feature, that five
    dispatches followed one piece of advice — three of them four days older than the
    feature. Same failure ADR 0016 forbids elsewhere: a conclusion stated with more
    confidence than its provenance earns, here in the very line that exists to check
    whether ADR 0016's mechanism works.

    Still not proof of causation, and the report says "since" rather than "because" for
    that reason. It is a window in which the claim is at least possible.

    Boundary, stated rather than hidden: rows are stamped to the second, so a handoff in
    the SAME second as the first advice counts as after it. It can only over-count, by at
    most the handoffs sharing one second with the moment advice first appeared.
    """
    since = first_advice()
    if since is None:
        return 0
    # Dispatches AND resumes: the pair measures whether the roster moved work to a persona,
    # and it does not care whether that persona was already running.
    return sum(1 for o in _read_all()
               if o.get("agent") and (t := _ts(o)) and t >= since)


#: Month files already parsed, keyed by path → ``((mtime_ns, size), rows)``.
#: Read and written only by :func:`_rows_of`, whose docstring is the argument for it.
_PARSED: dict[str, tuple[tuple[int, int], list[dict]]] = {}


def _rows_of(f: Path) -> list[dict]:
    """The rows of one month file, parsed at most once per version of that file.

    :func:`_read_all` walks EVERY month file ever written, and :func:`tally` sits on a
    per-turn repaint path: `persona.by_use` calls it for the switcher's order,
    `statusline._persona_chip_cells` draws that column, and `frame/panel.run` holds a
    process repainting it for as long as the frame lives. The cost of that walk is
    monotonic in the AGE of the plane and in nothing an operator does — measured at this
    plane's ~225 dispatches a month: 0.37 ms today, 2.10 ms after a year, 10.71 ms after
    five. It degrades silently, and only on the planes that have been used longest, whose
    operators would least expect it (#887).

    A month file that is not the current one is **closed**: :func:`path_for` only ever
    writes ``<this month>.<host>.jsonl``, and nothing reopens an earlier one. So every
    month but one is a permanent hit here, and the walk is bounded by the current month
    however old the plane is.

    **The key is the invalidation, not a summary.** Nothing derived from a row is kept —
    only the rows the jsonl already holds, discarded the instant the file they came from
    is not the file that was read. That is what stops this becoming a second source of
    truth for something the log answers, and ``(path, mtime, size)`` is the same shape
    `statusline._usage_stamp` and `frame/gather` already invalidate on.

    **Size is in the key because mtime alone is a filesystem's promise, not a fact.**
    APFS keeps nanoseconds, but some ext4 configurations report whole seconds, and
    :func:`record` appends to the current month file many times inside one such tick. Two
    versions of an append-only log always differ in length, so the size catches precisely
    what a coarse clock hides.

    **The stat is taken BEFORE the read, and that order is the correctness.** A row
    appended between the two is then memoised under the OLDER stamp and the next call
    re-reads it; taken after, that same row would be memoised under the NEWER stamp and
    stay invisible for the life of the process.

    Rows are shared rather than copied — every caller in this module only reads them, and
    a copy per call would hand back most of what the memo saves. A caller that wants to
    mutate a row must copy it first.
    """
    key = str(f)
    st = f.stat()
    stamp = (st.st_mtime_ns, st.st_size)
    hit = _PARSED.get(key)
    if hit is not None and hit[0] == stamp:
        return hit[1]
    rows: list[dict] = []
    for ln in f.read_text(errors="replace").splitlines():
        try:
            # No blank-line guard above this: `json.loads` raises ValueError on an empty
            # or whitespace-only line exactly as it does on a truncated one, so the pair
            # of lines that used to check for it here was a branch nothing could go red
            # without. It tolerates surrounding whitespace on a real row, too, and
            # `splitlines` has already taken the line endings off.
            o = json.loads(ln)
        except ValueError:
            continue
        if isinstance(o, dict) and (o.get("agent") or o.get("event")):
            rows.append(o)
    _PARSED[key] = (stamp, rows)
    return rows


def _read_all() -> list[dict]:
    d = _dir()
    if not d.exists():
        return []
    out = []
    for f in sorted(d.glob("*.jsonl")):
        try:
            out.extend(_rows_of(f))
        except OSError:
            continue
    return out


def _ts(o: dict) -> datetime | None:
    try:
        t = datetime.fromisoformat(str(o.get("ts", "")))
        return t if t.tzinfo else t.replace(tzinfo=timezone.utc)
    except ValueError:
        return None


def tally(days: int | None = None) -> Counter:
    """agent → dispatch count, optionally limited to the last *days*."""
    rows = _read_all()
    if days is not None:
        cutoff = _now() - timedelta(days=days)
        rows = [o for o in rows if (t := _ts(o)) and t >= cutoff]
    # A resume row carries an agent too, and must NOT land here: this column is read as
    # "times dispatched as a sub-agent", and it is what personas get retired on.
    return Counter(o["agent"] for o in rows if o.get("agent") and not o.get("event"))


def last_seen(agent: str) -> str | None:
    """ISO date this persona was last WORKED, or None. Counts resumes as well as
    dispatches — it feeds the roster block's "last dispatched" line, and a persona that was
    handed work an hour ago has not been idle since whenever it was first created."""
    best = None
    for o in _read_all():
        if o.get("agent") != agent:
            continue
        t = _ts(o)
        if t and (best is None or t > best):
            best = t
    return f"{best:%Y-%m-%d}" if best else None


def generic_share(days: int | None = None) -> tuple[int, int]:
    """(dispatches to generic agents, total dispatches) — the headline routing ratio."""
    t = tally(days)
    return sum(v for k, v in t.items() if k in GENERIC), sum(t.values())


# --------------------------------------------------------------------------- #
# Backfill — seed the tally from past sessions so the baseline exists TODAY     #
# instead of a week from now. Transcripts already record every dispatch; this   #
# just reads what is there. Writes to `*.backfill.jsonl`, kept separate from    #
# live records so a re-run replaces its own output instead of double-counting.  #
# --------------------------------------------------------------------------- #
BACKFILL_SUFFIX = ".backfill.jsonl"


def _transcript_dir() -> Path:
    """Claude Code stores a project's transcripts under ~/.claude/projects/<slug>, where
    the slug is the project path with every separator replaced by '-'."""
    slug = str(config.ROOT).replace("/", "-").replace("\\", "-")
    return Path.home() / ".claude" / "projects" / slug


def _live_keys() -> set[tuple[str, str]]:
    """Every dispatch the live hook actually recorded, by identity — (timestamp, agent).

    This replaced a single global cutoff: the earliest live record, with everything at or
    after it skipped. That prevented double-counting on one assumption — that once live
    recording had started it was **complete** — and #83 is the report that it is not. A
    `PostToolUse(Task|Agent)` hook does not fire for every background dispatch, so three
    real dispatches on 2026-08-10 were missing from a store whose earliest record was
    2026-08-07, and the cutoff put them permanently out of backfill's reach. One early
    live record disabled reconciliation for all later history.

    Identity expresses the same guarantee without the assumption, and it self-heals: a
    window the hook missed is imported the next time backfill runs, whenever that is,
    without anyone having to know which window it was.
    """
    keys: set[tuple[str, str]] = set()
    d = _dir()
    if not d.exists():
        return keys
    for f in d.glob("*.jsonl"):
        if f.name.endswith(BACKFILL_SUFFIX):
            continue
        for ln in f.read_text(errors="replace").splitlines():
            try:
                o = json.loads(ln)
                t = _ts(o)
            except ValueError:
                continue
            if t and o.get("agent"):
                keys.add((t.isoformat(timespec="seconds"), str(o.get("agent"))))
    return keys


def last_backfill() -> datetime | None:
    """When transcripts were last reconciled into the tally, or None if never.

    `stats` reports this because the tally is only as complete as its last reconciliation,
    and a count that silently omits every background dispatch reads exactly like a count
    that includes them (#83)."""
    d = _dir()
    if not d.exists():
        return None
    stamps = [f.stat().st_mtime for f in d.glob(f"*{BACKFILL_SUFFIX}")]
    return datetime.fromtimestamp(max(stamps)) if stamps else None


def scan_transcripts() -> list[dict]:
    """Every sub-agent dispatch recorded in this project's transcripts, as tally rows.
    Reads only the tool name, subagent_type and timestamp — never prompt text."""
    d = _transcript_dir()
    if not d.exists():
        return []
    rows = []
    for f in sorted(d.glob("*.jsonl")):
        try:
            text = f.read_text(errors="replace")
        except OSError:
            continue
        for ln in text.splitlines():
            try:
                o = json.loads(ln)
            except ValueError:
                continue
            if o.get("type") != "assistant":
                continue
            content = (o.get("message") or {}).get("content")
            if not isinstance(content, list):
                continue
            for b in content:
                if not isinstance(b, dict) or b.get("type") != "tool_use":
                    continue
                if b.get("name") not in ("Task", "Agent"):
                    continue
                agent = ((b.get("input") or {}).get("subagent_type") or "").strip()
                if agent:
                    rows.append({"ts": (o.get("timestamp") or "")[:19], "agent": agent})
    return rows


def backfill() -> tuple[int, int]:
    """Seed the tally from transcripts. Returns (imported, skipped-as-already-live).
    Idempotent: rewrites only the backfill files it owns."""
    live = _live_keys()
    keep: dict[str, set[str]] = {}
    seen: set[tuple[str, str]] = set()
    imported = skipped = 0
    for row in scan_transcripts():
        t = _ts(row)
        if not t:
            continue
        key = (t.isoformat(timespec="seconds"), str(row.get("agent") or ""))
        if key in live or key in seen:
            # Already tallied — by the live hook, or by an earlier line of this same run
            # (one dispatch can appear in more than one transcript file).
            skipped += 1
            continue
        seen.add(key)
        name = f"{t:%Y-%m}.{_host()}{BACKFILL_SUFFIX}"
        keep.setdefault(name, set()).add(
            json.dumps({"ts": key[0], "agent": row["agent"]}, sort_keys=True))
        imported += 1
    d = _dir()
    d.mkdir(parents=True, exist_ok=True)
    for f in d.glob(f"*{BACKFILL_SUFFIX}"):  # replace, never append — keeps re-runs clean
        f.unlink()
    for name, lines in keep.items():
        (d / name).write_text("\n".join(sorted(lines)) + "\n")
    return imported, skipped
