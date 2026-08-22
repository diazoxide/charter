"""What one frame knows about itself, on disk.

Under ``<STATE_DIR>/frame/<frame-id>/`` — per frame, never global, because two frames may
run at once (one per session, named by workspace and pid) and a shared version file would
make each frame's panels redraw for the other's activity.

``config.STATE_DIR`` is read as an attribute at call time, everywhere below, and never
imported as a bare name (``from ..config import STATE_DIR``) — the test harness repoints
it with ``config.use()`` after this module has already been imported, and a name bound at
import time would keep pointing at whatever ``STATE_DIR`` was when Python first loaded
this file, which on a developer's machine is the real plane.

**Minting an id is not resolving one.** :func:`frame_id` sanitises, because it is
producing a name from scratch — the same thing a slug generator does. :func:`frame_dir`
does the opposite: it is handed an id from a caller and must not invent a second identity
for a bad one by rewriting it into a good one. A later caller (``notify.plane_changed``)
reads its id out of ``$CHARTER_SESSION_ID`` rather than minting it here, so the id
``frame_dir`` resolves is not always one this module produced — it goes through
:func:`charter.contain.child`, which refuses a hostile name outright (see #328, #348).

**Nothing here raises, and nothing here mutates on a read.** ``bump()`` runs from
charter's hooks, where an exception costs a session its turn; ``version()`` is polled
several times a second by a panel, where a write-on-read would fight ``reap()`` over
whether a dead frame's directory should exist. A missing frame answers with a sentinel
("0", ``None``, ``[]``) rather than an exception or a side effect.

An id can be shaped correctly and still be unusable: ``contain.child`` bounds shape, not
length, so a multi-thousand-character ``$CHARTER_SESSION_ID`` passes it and then hits
``mkdir``'s own limit (``ENAMETOOLONG``) — reachable in practice, since
``session.py``'s id-safety regex strips characters but never bounds length. The read
paths (``version``, ``exit_code``) were already safe here, because a failing read was
already inside a ``try/except OSError``; ``frame_dir``'s ``mkdir`` and the writes in
``bump``/``record_exit`` needed the same guard to make the claim true rather than
aspirational.
"""

from __future__ import annotations

import os
import re
import shutil
import time
from pathlib import Path

from .. import config, contain

#: Anything outside this becomes an underscore. Only used to MINT an id in
#: :func:`frame_id` — never to rewrite one handed to :func:`frame_dir`, which resolves
#: through :func:`charter.contain.child` instead and refuses rather than rewrites.
_UNSAFE = re.compile(r"[^A-Za-z0-9._-]")


def frame_id(workspace: str, pid: int) -> str:
    """A stable id for one frame: the workspace it is for, and the launcher's pid.

    The same pair the tmux session and socket are named for, so a directory on disk and a
    session in `tmux list-sessions` can always be matched up by eye. *workspace* is a name
    read out of ``workspace.json`` or a directory listing rather than typed by an operator
    (#328), so it is sanitised here rather than trusted — this is the one place in the
    module that mints an identity instead of resolving one handed to it.
    """
    safe = _UNSAFE.sub("_", workspace).strip("._-") or "frame"
    return f"{safe}-{int(pid)}"


def _root() -> Path:
    return Path(config.STATE_DIR) / "frame"


def frame_dir(fid: str, *, create: bool = False) -> Path | None:
    """The directory *fid* owns, or ``None`` when *fid* cannot name one there.

    Resolves through :func:`charter.contain.child` rather than sanitising: *fid* may have
    come from ``$CHARTER_SESSION_ID`` rather than from :func:`frame_id`, and rewriting a
    hostile value into a safe-looking one would silently invent a second identity for it
    instead of surfacing the defect (the exact failure ``contain.child`` documents).

    ``create`` defaults to ``False`` so every READ in this module — ``version``,
    ``exit_code`` — never creates the directory it is only trying to look at. A panel
    polling the version of a frame that ``reap()`` already removed must see it stay gone,
    not have its own read resurrect it; the two write paths (``bump``, ``record_exit``)
    pass ``create=True`` because minting state IS their job.

    **The length guard lives here, not in `frame_id`.** `contain.child` bounds *shape*
    (no traversal, no separators) but not *length* — an id thousands of characters long
    still passes it — and `$CHARTER_SESSION_ID` reaches `bump()` without ever going
    through `frame_id`'s minting, so a cap there would guard some callers and not others.
    `mkdir` is the one place every caller's id necessarily passes through, so it is the
    one place a length cap protects all of them: an oversized-but-otherwise-valid id
    degrades to "no directory" (``ENAMETOOLONG``, caught below) exactly like a hostile
    one does above, rather than raising out of a hook that cannot afford it.
    """
    d = contain.child(_root(), fid)
    if d is None:
        return None
    if create:
        try:
            d.mkdir(parents=True, exist_ok=True)
        except OSError:
            # Shaped correctly (no traversal, no separators) but unusable anyway —
            # ENAMETOOLONG is the case this exists for, but any mkdir failure here
            # (permissions, a full filesystem) gets the same treatment: the caller
            # asked for a directory it cannot have, not for an exception.
            return None
    return d


def bump(fid: str) -> None:
    """Record that the frame changed. A caller on a must-not-crash path (a hook).

    Written to a temp file and moved into place with ``os.replace``. A failed write is
    swallowed (see below), not raised — which is exactly why the atomic replace matters
    more here than it would if a failure were still visible: ``os.replace`` only ever
    touches the target file by fully replacing it, never partially, so a write that
    cannot complete leaves the previous version exactly as a reader last saw it rather
    than corrupting it silently. Does nothing for an *fid* :func:`frame_dir` refuses, and
    nothing for a write that fails after the directory exists (a full filesystem, say) —
    this runs from charter's hooks, where raising costs a session its turn, so every
    failure here is a no-op rather than an exception.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "version.tmp"
    try:
        tmp.write_text(f"{time.time_ns()}\n")
        os.replace(tmp, d / "version")
    except OSError:
        # The directory existing doesn't guarantee the write does too (a filesystem
        # that fills up between the two calls above, say) — same must-not-raise
        # promise as the mkdir guard in frame_dir, covering the step after it.
        return


def version(fid: str) -> str:
    """The frame's version, cheap enough to poll several times a second.

    A probe, not a mutation: a frame with no version file yet — or no directory at all,
    because it was never bumped or was just reaped — answers with the sentinel ``"0"``
    rather than creating one by calling :func:`bump`. Doing that on a read would make
    every panel's poll resurrect a directory :func:`reap` had just deleted, and the two
    would fight forever over whether the frame still exists.
    """
    d = frame_dir(fid)
    if d is None:
        return "0"
    try:
        return (d / "version").read_text().strip() or "0"
    except OSError:
        return "0"


def record_exit(fid: str, code: int) -> None:
    """Record the harness's exit code. Same atomic-write shape as :func:`bump`."""
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "exit.tmp"
    try:
        tmp.write_text(f"{int(code)}\n")
        os.replace(tmp, d / "exit")
    except OSError:
        return


def exit_code(fid: str) -> int | None:
    """The recorded exit code, or ``None`` when the frame has not finished (or exist)."""
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return int((d / "exit").read_text().strip())
    except (OSError, ValueError):
        return None


def reap(live: set[str]) -> list[str]:
    """Remove state for frames whose tmux session is gone. Returns what was removed.

    Never by age: a frame open for two days is exactly a working frame, and an age
    heuristic would delete precisely that one. *live* names the sessions `tmux
    list-sessions` still reports, so the only frames removed are ones nothing is watching
    any more.
    """
    root = _root()
    if not root.is_dir():
        return []
    removed = []
    for d in sorted(root.iterdir()):
        if d.is_dir() and d.name not in live:
            shutil.rmtree(d, ignore_errors=True)
            removed.append(d.name)
    return removed
