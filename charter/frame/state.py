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

    ``except (OSError, ValueError)``, not just ``OSError``: a version file holding bytes
    that are not valid UTF-8 makes ``read_text()`` raise ``UnicodeDecodeError`` — a
    ``ValueError`` subclass, never caught by ``OSError`` alone — and this module's own
    docstring already promises "nothing here raises... a missing frame answers with the
    sentinel rather than an exception." A panel polls this function several times a
    second and has nothing of its own guarding the call (`panel._tick` reads it
    directly); an uncaught decode error here used to reach the run loop uncaught and
    kill the pane it was drawn in — precisely the hole this whole module exists to
    close, just reached through content corruption rather than a missing file.
    ``exit_code`` below already caught this shape (``int()`` raises ``ValueError`` for
    unparseable text, which happens to catch a decode error too); this brings
    ``version`` in line with it rather than leaving the two read paths inconsistent.
    """
    d = frame_dir(fid)
    if d is None:
        return "0"
    try:
        return (d / "version").read_text().strip() or "0"
    except (OSError, ValueError):
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


def clear_exit(fid: str) -> None:
    """Forget any exit code recorded under *fid*, because a new frame is claiming the id.

    The bill for #383's rule, and the reason it is only a bill and not a defect. A frame
    id is ``<workspace>-<launcher pid>`` and pids are recycled — Linux wraps at
    ``kernel.pid_max``, 32768 by default — so a launcher for the same workspace really
    does land on a pid an earlier launcher already used. Since :func:`reap` keeps a
    directory for as long as the pid in its name is live, and on a launch that pid is
    live BECAUSE IT IS THE LAUNCHER'S OWN, the earlier frame's directory survives to be
    adopted by the new one, ``exit`` file included. `cmd_launch` then reads that stale
    code back as its own and returns it: a harness running perfectly well, detached, is
    reported as having failed with a dead frame's number.

    A launch beginning is the one moment that can be certain about this — whatever is
    recorded under the id was recorded before this frame existed. Only ``exit`` is
    removed: ``version`` is a counter panels poll, and moving it is :func:`bump`'s job.
    Never raises, and never creates, for the same reasons as everything else here.
    """
    d = frame_dir(fid)
    if d is None:
        return
    try:
        (d / "exit").unlink(missing_ok=True)
    except OSError:
        # Same must-not-raise promise the rest of this module makes: a launch is not
        # worth failing over a file that could not be deleted, and the stale-code
        # reading below is the caller's own to notice.
        return


def record_server(fid: str, server: str) -> None:
    """Write down which tmux server this frame's session (or window) lives on.

    Charter runs frames on two servers now: its own private one (``tmux -L charter``,
    where a frame is a SESSION named by frame id) and, when charter is started from
    inside a tmux the operator already has, theirs (``tmux -S <socket>``, where a frame
    is a WINDOW named by frame id). Neither server's liveness list mentions the other's
    frames, so :func:`reap` needs to know which server each directory belongs to before
    it can decide that "not live" means "dead" rather than "not this server's".

    Same must-not-raise, atomic-write shape as :func:`bump` and :func:`record_exit`, and
    for the same reason: this runs on the launch path, where an id ``frame_dir`` refuses
    (or a filesystem that will not take the write) has to degrade to "unknown server"
    rather than take the launch down with it.
    """
    d = frame_dir(fid, create=True)
    if d is None:
        return
    tmp = d / "server.tmp"
    try:
        tmp.write_text(f"{server}\n")
        os.replace(tmp, d / "server")
    except OSError:
        return


def frame_server(fid: str) -> str | None:
    """Which server *fid* was launched on, or ``None`` when charter does not know.

    ``None`` is the migration case and nothing else: every frame this charter starts
    records one (see :func:`record_server`), so a directory without the marker was
    written by a charter that only ever ran frames on its own private server. See
    :func:`reap` for what that means there.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return (d / "server").read_text().strip() or None
    except (OSError, ValueError):
        return None


def exit_code(fid: str) -> int | None:
    """The recorded exit code, or ``None`` when the frame has not finished (or exist)."""
    d = frame_dir(fid)
    if d is None:
        return None
    try:
        return int((d / "exit").read_text().strip())
    except (OSError, ValueError):
        return None


#: The subdirectory :func:`respawn_attempt` counts in, named once so
#: :func:`clear_respawn` cannot drift away from it — the two are only correct together.
_RESPAWN_DIR = "respawn"


def respawn_attempt(fid: str, slot: str) -> int | None:
    """Claim the next respawn attempt for *slot* in *fid*, or ``None`` when it cannot.

    Read-increment-write of a single integer, one file per slot. tmux has no way to
    count anything, and a panel pane's `pane-died` hook SURVIVES the respawn it triggers
    (verified against real tmux 3.7c — `show-hooks -p` reads the hook back unchanged
    afterwards), so without a count on disk a panel that dies instantly on every start
    respawns in a hot loop forever. This is the only thing bounding it; see
    `commands_frame._RESPAWN_ATTEMPTS`.

    **Never reset, and not by oversight.** A successful respawn does not zero the count,
    so the budget is three deaths across the whole life of one frame rather than three
    in a row. The alternative needs a definition of "the panel came back up and stayed
    up" that nothing here can observe — tmux only reports that a process was started,
    not that it kept running — and guessing at one is how a panel that dies every
    ninety seconds gets respawned forever while still looking healthy at each individual
    check. What keeps that budget attached to a FRAME rather than to a reused id is
    :func:`clear_respawn`, which a launch calls as it claims the id.

    ``None`` — never ``0``, never a silent restart of the count — for every way this can
    fail to record: an *fid* or *slot* :func:`contain.child` refuses, a directory that
    cannot be made, a write that cannot complete. The caller reads it as "give up",
    which is the safe direction: the failure mode of counting wrong here is an unbounded
    respawn loop, and a dead pane still shows tmux's own `Pane is dead (status N)`.

    **``create=False``, deliberately, even though this writes.** A frame that still
    exists always has its directory (`cmd_launch` creates and `bump`s it before any pane
    is split), so refusing to count for a frame whose state is already gone costs a live
    frame nothing. What `create=True` would buy is the power to REMAKE a directory
    :func:`reap` has removed — one orphan per frame, surviving until some later launch
    reaps it, which is exactly the resurrect-what-reap-just-deleted hazard this module's
    own docstring records for `version()`. Every panel pane dies when its frame is torn
    down, so every panel's `pane-died` hook fires on the way out and lands here; since
    #383 :func:`reap` keeps a directory while the launcher pid in its name is live, so on
    that path the directory is usually still present and a count is spent on a frame that
    is already over. Harmless, and deliberately not special-cased: `cmd_respawn` re-asks
    whether the session exists after its backoff, and the directory goes when the next
    launch reaps it or clears it.
    """
    d = frame_dir(fid)
    if d is None:
        return None
    root = d / _RESPAWN_DIR
    try:
        root.mkdir(exist_ok=True)
    except OSError:
        return None
    # One file per slot under a directory of their own, rather than a `respawn-<slot>`
    # sibling of `version`/`exit` in the frame directory itself. The prefix version put
    # the slot in the SECOND path component, where every separator-carrying name lands
    # under a `respawn-<x>` directory that never exists — so the write failed and this
    # function answered `None` whether `contain.child` was here or not, making the
    # guard unobservable and, by the only test that could have pinned it, dead. With
    # the slot as the first component under a directory that DOES exist, a name like
    # `../y` would really be written outside this frame's own directory, and refusing
    # it is a behaviour a test can tell apart from a failed write.
    f = contain.child(root, slot)
    if f is None:
        return None
    try:
        previous = int(f.read_text().strip())
    except (OSError, ValueError):
        # No file yet is the ordinary case (the first death of this frame's own panel);
        # unparseable content is treated identically rather than specially, exactly as
        # `version` and `exit_code` above treat theirs — this is a counter, not a
        # record anything else has to agree with.
        previous = 0
    n = previous + 1
    try:
        f.write_text(f"{n}\n")
    except OSError:
        return None
    return n


def clear_respawn(fid: str) -> None:
    """Forget every respawn count under *fid*, because a NEW frame is claiming the id.

    The third line on :func:`clear_exit`'s bill, and the same recycled pid underneath it
    (#383). A frame id is ``<workspace>-<launcher pid>``; since #383 :func:`reap` keeps a
    directory for as long as the pid in its name is live, and on a launch that pid is
    live BECAUSE IT IS THE LAUNCHER'S OWN — so a launcher landing on a pid an earlier
    launcher for the SAME workspace already used adopts that earlier frame's whole
    directory, its ``respawn/`` counts included.

    Inheriting those is not litter, it is a budget already spent. :func:`respawn_attempt`
    never resets, and every panel's `pane-died` hook fires during the previous frame's
    teardown, so an adopted count is at least one per slot and may already sit at
    `commands_frame._RESPAWN_ATTEMPTS`. The new frame's panels would then die once and
    stay dead without ever being brought back — the precise outcome the count exists to
    ration, handed out for a pid number rather than for anything that happened. Clearing
    as the id is claimed keeps "three deaths" a property of a frame, not of a name.

    Never raises, and never creates: ``rmtree(ignore_errors=True)`` is the same tool
    :func:`reap` uses for the same must-not-fail reason, and the ordinary first launch
    for a workspace has no directory here at all — it must not mint one just to empty it.
    """
    d = frame_dir(fid)
    if d is None:
        return
    shutil.rmtree(d / _RESPAWN_DIR, ignore_errors=True)


def _launcher_pid(name: str) -> int | None:
    """The launcher pid :func:`frame_id` put at the end of *name*, or ``None``.

    The inverse of :func:`frame_id`'s last line and nothing more — a reader, not a
    parser of arbitrary text. ``None`` means "this name carries no pid", which is a real
    answer and not a failure: the frame root can hold debris, a hand-made directory, or a
    name minted by an older charter, and none of those may become undeletable just
    because nothing can be read out of them.

    Requires the separator, so a bare ``12345`` reads as ``None`` rather than as a pid —
    :func:`frame_id` always emits ``<workspace>-<pid>`` with a non-empty workspace (its
    ``or "frame"`` fallback guarantees it), so a name without one did not come from here
    and its digits are not a claim about any process. ``rpartition`` rather than
    ``split``, because a workspace may contain ``-`` itself (``harness-wrapper-4242``).
    """
    head, sep, tail = name.rpartition("-")
    if not sep or not head or not tail.isdigit():
        return None
    pid = int(tail)
    # `0` is "every process in my group" to `kill(2)`, not a process — and `frame_id`
    # can only ever have written a real `os.getpid()` here, which is never 0 or negative.
    return pid if pid > 0 else None


def _launcher_is_alive(pid: int) -> bool:
    """Is the process *pid* names still running? Asks; never signals anything.

    ``os.kill(pid, 0)`` is a QUESTION on POSIX and an ANSWER on Windows, where it maps to
    ``TerminateProcess`` and would kill whatever the name happened to hold (the same trap
    ``news._outer_probe`` documents). Off POSIX the pid is taken at its word — a frame
    directory that outlives its launcher there costs a few hundred bytes, where getting
    it wrong costs somebody else's process. Belt and braces in practice: :func:`reap` is
    only ever called from `commands_frame.cmd_launch`, which has already refused if there
    is no tmux, and there is no tmux off POSIX.

    ``PermissionError`` (EPERM) is an ANSWER too, and the opposite of what it looks like:
    the process exists, it simply is not ours to signal. Reading it as "gone" would make
    every frame launched by another user reapable while it was still running.
    """
    if os.name != "posix":
        return True
    try:
        os.kill(pid, 0)   # signal 0 asks whether it exists; it sends nothing
    except ProcessLookupError:
        return False
    except OverflowError:
        # A number too large for a `pid_t` — `int()` accepted it happily and `os.kill`
        # would raise straight out of a launch. NOT an `OSError`, so it needs naming
        # separately. It cannot name a live process, so the directory stays reapable.
        return False
    except OSError:
        return True       # alive, and not ours to signal
    return True


def reap(live: set[str], *, server: str) -> list[str]:
    """Remove state for frames of *server* that are gone. Returns what was removed.

    Never by age: a frame open for two days is exactly a working frame, and an age
    heuristic would delete precisely that one. *live* names what that server still
    reports — sessions on charter's own private one, windows on an operator's — so the
    only frames removed are ones nothing is watching any more.

    **Scoped to one server, because "not live" is only an answer the frame's OWN server
    can give.** A frame launched inside the operator's tmux is a window on their socket
    and appears in no `tmux -L charter list-sessions` output at all; reaping on that
    list alone deletes a running frame's version file (its panels stop noticing the
    agent) and its recorded exit code (its launcher reads back `None` and reports the
    wrong status) while the frame is still on screen. *server* is matched against
    :func:`frame_server`, which the launcher records when it creates the directory.

    A directory with NO recorded server matches every one, and that is the migration
    case rather than a loophole: only a charter that predates :func:`record_server`
    leaves one, every such frame was on the private server, and refusing to reap them
    would trade one release's transient wrongness for a permanent leak.

    **And never on a live session alone either, because a frame outlives its session
    (#383).** Between a harness exiting and its launcher reading :func:`exit_code`, the
    session is already gone from `tmux list-sessions` while the launcher is still
    sitting in `cmd_launch` with the answer one line away. A `reap()` from a SIBLING
    frame's launch — and one runs at every launch — landing in that window deleted the
    `exit` file before it was ever read: `exit_code` answered ``None``, `cmd_launch`
    turned that into a returned ``0``, and a harness that had genuinely failed was
    reported as a success to whatever `&&` chain or CI step invoked charter. Ordering
    `cmd_launch`'s reap before its own `frame_dir(create=True)` narrows that window; it
    cannot close it, because by then the sibling's session is genuinely absent from
    *live*. Server scoping does not close it either — a sibling on the SAME server is
    exactly the case that bites — so the two guards are independent and both are asked.

    So a second question is asked, and :func:`frame_id` is what makes it answerable
    without inventing any new state: the id ENDS in the launcher's own pid. A directory
    whose launcher is still a live process is one somebody may still come back to, and
    is left alone — no age, no timestamps, no extra file, nothing to keep in sync.

    Both ways of being wrong point the same way, deliberately. A pid the OS has recycled
    onto an unrelated process reads as "alive" and costs one directory's cleanup, deferred
    until that process ends and some later launch reaps it — bounded, silent, and measured
    in bytes. Reading a live launcher as dead costs a real exit code, which is the defect
    above. A launcher also no longer reaps its OWN directory on the way out (its pid is,
    necessarily, still alive); the next launch takes it, which is what already happens for
    every frame that ended while nothing else was starting.

    Recycling has one sharper form that is NOT paid for in bytes, and it is paid for in
    :func:`clear_exit` rather than here: a pid recycled onto a launcher for the SAME
    workspace mints the same id, so the kept directory is not merely litter, it is the id
    the new frame is about to adopt — stale ``exit`` file included. Deciding that here
    would mean guessing which of two frames a directory belongs to; the launcher knows,
    because it is the one claiming the id, so it clears the file as it starts.
    """
    root = _root()
    if not root.is_dir():
        return []
    removed = []
    for d in sorted(root.iterdir()):
        if not d.is_dir() or d.name in live:
            continue
        # Two independent reasons to keep a directory, and BOTH must be absent before
        # anything is deleted. They answer different questions and neither implies the
        # other: the frame may belong to the OTHER tmux server (#381), or its launcher
        # may still be running on this one (#383).
        owner = frame_server(d.name)
        if owner is not None and owner != server:
            continue
        pid = _launcher_pid(d.name)
        if pid is not None and _launcher_is_alive(pid):
            continue
        shutil.rmtree(d, ignore_errors=True)
        removed.append(d.name)
    return removed
