"""Committed skill tally — which skills each persona actually *invokes*.

A persona declares the skills it starts holding (``skills:`` — see
:func:`charter.persona.declared_skills`), and the host preloads their full text into the
sub-agent at startup. Declaring is cheap to write and expensive to keep: the text is
injected on **every dispatch** of that persona, for as long as the line exists.

Nothing could see whether any of it was used. That is the same blindness ``dispatch.py``
was built for one level up — a persona that lints green and is never dispatched — aimed
here at a persona's *equipment* rather than at the persona itself.

Shape of the store::

    personas/_skills/<YYYY-MM>.<host>.jsonl     one JSON object per line

The three properties are ``dispatch.py``'s, for the same reasons:

* **Counts and dates only.** A line is ``{"ts": …, "skill": …, "persona": …}`` — never the
  arguments a skill was invoked with, which is where a workspace or client name would
  travel. There is no secret surface to scan.
* **Parallel-writer safe.** Small lines opened ``O_APPEND``, so a fan-out of sub-agents
  each invoking skills appends without a lock.
* **Conflict-free across engineers.** The host is in the filename, so two people recording
  the same month never touch the same file — the committed tally merges by addition rather
  than conflicting.

A separate store from the dispatch tally rather than a column added to it, because they
answer different questions: dispatch asks *was this persona used*, this asks *was the
equipment it carries worth carrying*. Merging them would make one of the two questions
unanswerable, which is ADR 0010's rule stated the other way round.
"""
from __future__ import annotations

import json
import os
import re
import socket
from datetime import datetime, timezone
from pathlib import Path

from . import config

#: Directory under ``personas/`` holding the tally. Leading underscore so it can never
#: collide with a persona name — the same convention ``_dispatch`` and ``_shared`` use.
DIR_NAME = "_skills"


def _host() -> str:
    """Short, filename-safe hostname — keeps each engineer on their own file."""
    raw = (socket.gethostname() or "unknown").split(".")[0]
    return re.sub(r"[^A-Za-z0-9_-]", "", raw)[:32] or "unknown"


def _now() -> datetime:
    return datetime.now(timezone.utc)


def _dir() -> Path:
    return config.ROOT / "personas" / DIR_NAME


def path_for(when: datetime | None = None) -> Path:
    when = when or _now()
    return _dir() / f"{when:%Y-%m}.{_host()}.jsonl"


def record(skill: str, persona_name: str | None = None,
           when: datetime | None = None) -> Path | None:
    """Append one skill invocation. Best-effort: a tally must never break a turn, so any
    failure is swallowed and reported as ``None``."""
    skill = (skill or "").strip()
    if not skill:
        return None
    when = when or _now()
    p = path_for(when)
    try:
        p.parent.mkdir(parents=True, exist_ok=True)
        line = json.dumps({"ts": when.isoformat(timespec="seconds"),
                           "skill": skill,
                           "persona": (persona_name or "").strip() or None},
                          sort_keys=True) + "\n"
        # O_APPEND: concurrent sub-agents interleave safely without a lock.
        fd = os.open(p, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
        try:
            os.write(fd, line.encode())
        finally:
            os.close(fd)
        return p
    except OSError:
        return None


def _read_all() -> list[dict]:
    d = _dir()
    if not d.exists():
        return []
    out: list[dict] = []
    try:
        files = sorted(d.glob("*.jsonl"))
    except OSError:
        # Listing an unreadable directory raises on Linux and yields nothing on macOS —
        # the divergence that took CI red once already (#171's neighbourhood).
        return []
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
            if isinstance(obj, dict) and obj.get("skill"):
                out.append(obj)
    return out


def by_persona(name: str) -> dict[str, int]:
    """``{skill: count}`` for one persona. Leaf names, so a declaration written
    ``superpowers:tdd`` matches an invocation recorded as ``tdd``."""
    out: dict[str, int] = {}
    for e in _read_all():
        if (e.get("persona") or "") != name:
            continue
        leaf = str(e["skill"]).split(":", 1)[-1]
        out[leaf] = out.get(leaf, 0) + 1
    return out


def drift(name: str) -> dict[str, list[str]]:
    """What a persona declared against what it actually used.

    Returns ``{"unused": [...], "undeclared": [...]}``.

    **unused** — declared, never invoked. Not merely untidy: the skill's full text is
    preloaded on every dispatch of this persona, so an unused declaration is a standing
    context cost bought for nothing.

    **undeclared** — invoked, never declared. The more interesting direction, and the one
    the declaration makes answerable at all: the persona is doing work its charter does not
    describe, so either the charter is out of date or the persona is reaching past its
    remit. charter names the divergence and does not resolve it (ADR 0013), because which
    of those it is depends on intent charter cannot read.
    """
    from . import persona as _persona
    declared = {s.split(":", 1)[-1] for s in _persona.declared_skills(name)}
    used = set(by_persona(name))
    return {"unused": sorted(declared - used), "undeclared": sorted(used - declared)}
