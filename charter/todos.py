"""A workspace's **todos** — its intent, as against what it learned or what happened.

charter already records two things about a task: durable facts (**memory**) and a
chronological record of what was done (the **journal**). Neither can hold what the task
still *means* to do. That gap is why every session on a long-running task re-derives its
own plan: work consciously deferred leaves no trace, so it is indistinguishable from work
nobody ever thought of.

A todo is the third category, and it differs from the other two in one way that decides
the whole design: **it expires.** A memory is true indefinitely; a todo stops being true
the moment it is done or abandoned. So todos get their own store beside ``memory/`` rather
than a state flag inside it — a ``charter recall`` hit on something finished last month
would read with exactly the authority of a fact, which is worse than no hit at all
(docs/adr/0004).

Two states only, open and done, and done *deletes* — the journal is already the permanent
record of what happened. ``in_progress`` is deliberately absent: it is a session concept,
correctly owned by Claude Code's own ephemeral task list, and a durable list claiming
something has been in progress for three weeks is simply lying (docs/adr/0006).

Workspace-scoped only. A persona is a role, not a worker with a queue (docs/adr/0005).
"""

from __future__ import annotations

import datetime
from pathlib import Path

from . import memstore, workspace

_HEADER = (
    "# Todos — workspace `{name}`\n\n"
    "One line per todo; each links a file holding one thing this task still means to do.\n"
    "Open or done — and done removes it, leaving its trace in the journal instead.\n"
)

#: Word-overlap above which two todos are "the same intent". Borrowed from the memory
#: store's own duplicate detection rather than tuned separately: the failure it prevents
#: is identical in kind, and one threshold is easier to reason about than two.
_DUPLICATE_THRESHOLD = 0.5


def todos_dir(name: str) -> Path:
    """``workspaces/<name>/todos/`` — a *sibling* of ``memory/``, never a child."""
    return workspace.workspace_dir(name) / "todos"


def index(name: str) -> Path:
    return memstore.index_path(todos_dir(name))


def scaffold(name: str) -> Path:
    """Ensure the todo directory and its index exist. Returns the index path."""
    return memstore.ensure_index(todos_dir(name), _HEADER.format(name=name))


def add(name: str, text: str, *, stamp: datetime.datetime | None = None) -> Path:
    """Record one todo as its own timestamp-prefixed file, and index it.

    Timestamped for a reason beyond tidiness: oldest-first is the only ranking this feature
    has, and taking it from the filename means the directory listing *is* the order. No
    priority field, so nothing to leave blank and nothing to disagree with age.

    That ordering is only as fine as the stamp, which is **whole seconds**: several todos
    recorded in the same second sort by name among themselves, not by which came first.
    Harmless for a human, and worth knowing for an agent recording a burst — "the three
    oldest" is then arbitrary within that second.

    *stamp* overrides the recorded time, passed straight through to the store. It exists so
    ordering can be tested across real time gaps rather than within one second, where
    alphabetical tie-breaking would make any such test pass for the wrong reason.
    """
    scaffold(name)
    return memstore.write(todos_dir(name), text, timestamped=True, index=True, stamp=stamp)


def duplicate_of(name: str, text: str) -> str | None:
    """The title of an existing todo that already says this, or None.

    Deliberately a query rather than something :func:`add` enforces: the *policy* — warn
    and skip — belongs to the command, so the store stays a store. Scoped to one workspace,
    since an identical todo in another is a different task's business.
    """
    existing = memstore.entries(todos_dir(name))
    if not existing:
        return None
    words = _words(text)
    if not words:
        return None
    for _p, title, body in existing:
        other = _words(f"{title} {memstore.body(body)}")
        if not other:
            continue
        # Jaccard — intersection over UNION, the same metric `memstore.duplicates` uses.
        # Dividing by max(len) instead looks equivalent and is not: on text this short a
        # single shared word is half the content, so "first thing" and "second thing"
        # scored 0.5 and the second was refused as a duplicate of the first.
        if len(words & other) / len(words | other) >= _DUPLICATE_THRESHOLD:
            return title
    return None


def _words(text: str) -> set[str]:
    return memstore.wordset(text or "")


def open_todos(name: str) -> list[dict]:
    """Every open todo, **oldest first**, each with its age in days.

    Oldest-first makes the session reminder self-correcting: what surfaces is what is being
    avoided, rather than what you already have in mind. Age is reported and never enforced —
    an open todo is a commitment a human made, so nothing here deletes one on a timer.
    """
    today = datetime.date.today()
    out = []
    for p, title, text in memstore.entries(todos_dir(name)):
        recorded = memstore.memory_date(text, p.name)
        out.append({
            "slug": p.stem,
            "title": title,
            "text": memstore.body(text),
            "age_days": (today - recorded).days if recorded else 0,
        })
    return out


def search(name: str, query: str, limit: int = 8) -> list[tuple[Path, str, int]]:
    """Keyword search across this workspace's todos."""
    return memstore.search([todos_dir(name)], query, limit=limit)


def count_open(name: str) -> int:
    return len(memstore.files(todos_dir(name)))
