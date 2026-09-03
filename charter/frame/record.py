"""Recording the plane as it changes — the half of `frame/reopen.py` that is not a quit.

**Record, never save.** `charter save` commits and pushes a tree; this writes a local file.
The two are unrelated and a message that said "save" would be ambiguous about which one did
not happen, so this module uses the verb `reopen.write` and 0.55.0's news entry already
use: *"quitting charter records the plane"*.

**When it writes: debounced on the bumps that already exist.** `state.bump` is charter's own
"something changed", moved by every writer in the frame — the launcher, the hooks, a
detached gather, a keypress that changed the shape. Reusing it means there is no second
notion of change to keep in step with the first. Two rejected alternatives, and both were
rejected for the same measured reason:

* **On every bump.** `cmd_launch` bumps immediately after `new_chat_id` and before
  `new-window` runs, so a record taken there names a chat that has no window and no harness
  — a plane that never existed. The quiet period is what buys back the consistency a quit
  got for free by stopping the world first.
* **On a timer.** Same defect (a tick can land in that same window) plus work spent on a
  plane that did nothing, which is most of the time.

**Who writes: the frame process, alone** — the launcher that is the operator's terminal for
the life of the frame (`commands_frame.cmd_launch`'s `attach`), and `charter reopen`'s own
attach. Panels are separate processes: six of them notice one bump, so recording where the
bump was NOTICED is six processes racing one manifest. A designated panel is worse, because
it stops recording the moment that panel dies, which is exactly when the record matters.

**And being that writer costs a watch this process did not have.** Only panels watch bumps
today (`panel._watch`), so the frame process gains a polling thread it never carried. That
cost is paid rather than avoided by moving the writer, and there is no cheaper mechanism to
pay it with: a bump is a file written by ANOTHER process, and the standard library has no
file-change notification to subscribe to — which is why `panel._watch` polls as well.
:data:`POLL` is deliberately slower than `panel.TICK`: a panel is drawing for a human, this
is deciding whether to write a file that is already :data:`QUIET` seconds behind.

Nothing here raises. This runs on a daemon thread inside the process holding the operator's
terminal, so an exception on the loop would either kill the thread silently or print into a
frame that is about to switch to the alternate screen — `frame/state.py`'s own promise, for
the same reason.
"""

from __future__ import annotations

import contextlib
import os
import threading
import time

from . import chats, state

#: How long the plane must go without a bump before it is written down. Two seconds is
#: chosen against the thing it exists to exclude rather than by feel: the window between
#: `state.bump(fid)` in `cmd_launch` and the chat's window actually existing is the tmux
#: round trips in between — `new-session`/`new-window`, the option writes, the panel splits
#: — which is tens of milliseconds on a warm server and was measured under 400ms on a cold
#: one. Two seconds clears that by a wide margin and still costs an operator at most two
#: seconds of a plane that changed and was not yet on disk.
QUIET = 2.0

#: How often the frame process reads the bumps. Deliberately slower than `panel.TICK`
#: (0.2s): a panel is redrawing for someone who is looking at it, and this is deciding
#: whether to write a file whose answer is :data:`QUIET` seconds old by design. Five times
#: a second would be four extra `stat`-sized reads per chat per second for no earlier write.
POLL = 0.5

#: How long :meth:`Recorder.stop` waits for the loop to come back before giving up on it.
#:
#: Two seconds, and it is a fact about the WRITE rather than about :data:`POLL`: a tick that
#: is already under way is inside `_plane_live`, whose `list-windows` carries a five-second
#: timeout of its own, so no arithmetic over the poll interval says anything useful about
#: how long the loop might legitimately take to return. What this number is for is the other
#: end — a write stuck on a wedged filesystem must not stand between an operator and their
#: shell, and the cost of giving up early is a daemon thread that dies with the process.
#:
#: It was `max(poll * 4, 1.0)`, and the deletion sweep reported BOTH halves as survivors:
#: neither the product nor the floor decides anything any test can see, because neither is
#: derived from anything. A number nothing can pin is a number that should not be computed.
JOIN = 2.0


def fingerprint() -> frozenset[tuple[str, str]]:
    """Every chat on this plane and the version it is at, as a SET.

    **`state.version` and nothing else**, so "has this chat changed" has one answer rather
    than one for the panels and one for the recorder.

    **A frozenset rather than a sorted tuple, and the deletion sweep is what settled it.**
    This value is only ever compared for equality (`Debounce.saw`), so what it has to be is
    a function of the FACTS and not of the order `os.scandir` happened to hand them over in.
    It was `sorted(...)`, and the sweep reported `sorted` → `list` as a survivor. Measured
    on this machine (APFS) rather than argued: readdir order is a function of the name set —
    identical across a `rmdir`/`mkdir` reclaim of the same ordinal (the case
    `frame/reopen.py` says happens *very often*) and across a chat added and removed between
    two readings — so `sorted` was defending against something that cannot arrive, which is
    a survivor wearing a guard. POSIX still promises nothing about that order, so the answer
    is not to delete the sort and depend on the measurement: it is to stop asking. A set has
    no order to be wrong about, and it says what this value is.

    Held to `chats.ID_RE` and `is_dir()` rather than to `chats.is_chat`, and that is a
    deliberate widening: `is_chat` reads a second file per chat (`state._launcher_pid`) to
    tell a chat from the `{workspace}-{pid}` frames that predate them, and this is a
    CHANGE DETECTOR rather than a list of things to act on. A directory that turns out not
    to be a chat costs one write of a plane that reads the same either way; a second file
    read per chat per poll is paid forever. What the filters do buy is the property the
    caller depends on: the manifest and the transcripts live in this same directory and are
    not directories themselves, so writing the record cannot move the fingerprint and
    provoke another write.

    Empty for a plane with no frame root at all — a machine that has never launched one.
    Never raises: this is a poll loop, and `state.version` already answers ``"0"`` rather
    than raising for every way a version can fail to be read.
    """
    try:
        names = [e.name for e in os.scandir(state._root())
                 if e.is_dir() and chats.ID_RE.fullmatch(e.name)]
    except OSError:
        return frozenset()
    return frozenset((n, state.version(n)) for n in names)


#: What :class:`Debounce` has seen before it has seen anything. Its own object rather
#: than ``None`` because ``None`` is a fingerprint a reader could legitimately hand in, and
#: the first reading of a plane has to count as a change: a plane that came back from
#: `charter reopen` and then sat still has had no bump since, and `_consume` deleted the
#: manifest that put it there — without this it would be recorded nowhere until something
#: happened to move it.
_NOTHING = object()


class Debounce:
    """One write per quiet period, driven by what was seen rather than by a clock.

    Pure: no clock, no thread, no filesystem. The time is an argument, so the decision can
    be asserted at exact instants instead of by sleeping — and so the loop below is left
    with nothing in it but sleeping and calling.
    """

    def __init__(self, *, quiet: float = QUIET) -> None:
        self.quiet = quiet
        self.seen = _NOTHING
        #: Whether what was last seen still owes a write. A flag rather than a comparison
        #: of "seen" against "written": those two would have to be told apart by identity
        #: (a plane that changed and changed back is genuinely a change, and an equal tuple
        #: is a different object), and an identity test here is a mutation nothing can kill
        #: — swapping it for equality behaves identically on every reading `saw` can
        #: produce, because `saw` only ever replaces `seen` with something unequal.
        self.pending = False
        self.at = 0.0

    def saw(self, fp, *, now: float) -> None:
        """Record one reading. A reading equal to the last one is not a change, and
        deliberately does not move the deadline: a plane that is quiet must eventually be
        written, not held off forever by being read."""
        if fp != self.seen:
            self.seen = fp
            self.at = now
            self.pending = True

    def due(self, *, now: float) -> bool:
        """Whether what was last seen is worth writing yet."""
        return self.pending and now - self.at >= self.quiet

    def wrote(self) -> None:
        """The reading that was due has been acted on."""
        self.pending = False


class Recorder:
    """The frame process's own watch on the bumps, and the one thing that writes.

    *write* is handed the chat this terminal is standing in and answers whether the record
    landed — `commands_frame` supplies it, so this module never imports the thing that
    knows what a plane is. *read* and the clock are injected for the same reason the
    debounce takes its time as an argument: the loop is then two lines and every decision
    in it is assertable without a thread.
    """

    def __init__(self, write, *, quiet: float = QUIET, poll: float = POLL,
                 read=fingerprint, clock=time.monotonic) -> None:
        self._write = write
        self._read = read
        self._clock = clock
        self.poll = poll
        self.debounce = Debounce(quiet=quiet)
        #: The chat this process's terminal is on, which becomes the manifest's `focus` —
        #: the workspace a reopen attaches to. Mutable and set from outside (`focus_on`)
        #: for `commands_frame.Reopening.fid`'s reason: the id is allocated well after this
        #: starts, and inferring it from the plane would be reading back a fact the
        #: launcher has in hand. ``""`` until it is known, which `reopen`'s own reader
        #: already treats as "no workspace recorded" and falls back from.
        self.chat = ""
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def tick(self, *, now: float | None = None) -> bool:
        """Read the plane once and write it if it is due. Whether it wrote.

        **Never raises**, in either direction. A reading that failed is a plane charter
        could not see, which is not a plane that changed — so it is skipped rather than
        recorded as empty. A write that raised is over as far as this loop is concerned:
        it is marked acted-on, so a filesystem that refuses does not turn a two-second
        debounce into a hot loop.
        """
        when = self._clock() if now is None else now
        try:
            fp = self._read()
        except Exception:  # noqa: BLE001 - a poll loop in the operator's terminal
            return False
        self.debounce.saw(fp, now=when)
        if not self.debounce.due(now=when):
            return False
        self.debounce.wrote()
        try:
            return bool(self._write(self.chat))
        except Exception:  # noqa: BLE001 - same promise, on the writing half
            return False

    def _loop(self) -> None:
        # `Event.wait` and not `time.sleep`, so a stop is answered inside the wait it
        # interrupts rather than one whole POLL later — which is what makes the join in
        # :meth:`stop` bounded by a tick rather than by the poll interval.
        #
        # It WAITS BEFORE IT READS, which is the ordering that matters: the launcher starts
        # this before it has built anything, and a first reading taken instantly would be
        # of a plane mid-launch. The debounce would hold that reading anyway; waiting first
        # means the loop never even asks about a plane nobody has finished making.
        while not self._stop.wait(self.poll):
            self.tick()

    def start(self) -> None:
        """Begin watching. A daemon thread, so a launcher that exits without stopping it
        cannot hang the process on the way out."""
        self._thread = threading.Thread(target=self._loop, name="charter-record",
                                        daemon=True)
        self._thread.start()

    def stop(self) -> None:
        """Stop watching, and wait for the loop to notice.

        Joined rather than abandoned: the caller is about to reap the very directories the
        loop reads (`cmd_launch`'s closing `state.reap`), and a thread still inside `tick`
        would be recording a plane that is being deleted. Bounded by :data:`JOIN`, because a
        write stuck on a wedged filesystem must not stand between an operator and their
        shell.
        """
        self._stop.set()
        if self._thread is not None:
            self._thread.join(timeout=JOIN)

    def alive(self) -> bool:
        """Whether the watch is still running — asked of the thread rather than of a flag,
        so :meth:`stop` failing to join would be visible instead of asserted away."""
        return self._thread is not None and self._thread.is_alive()


#: The one recorder this process may have, and the reason it is a module-level singleton
#: rather than a value passed down: the automatic restore reaches `cmd_reopen` from inside
#: `cmd_launch`, and both of those attach — so the same process would otherwise start two
#: loops racing one manifest, which is the exact failure keeping panels out of this avoids,
#: reached from inside instead of from six processes.
_RUNNING: Recorder | None = None


def running() -> Recorder | None:
    """This process's recorder, or ``None``."""
    return _RUNNING


def start(write, **kw) -> Recorder | None:
    """Start recording, or ``None`` when this process is already doing it.

    The answer is what tells a caller whether it owns the :func:`stop` — a nested caller
    gets ``None`` and leaves the outer one to end it, which is what makes a restore inside
    a launch safe.
    """
    global _RUNNING
    if _RUNNING is not None:
        return None
    r = Recorder(write, **kw)
    _RUNNING = r
    r.start()
    return r


def stop() -> None:
    """Stop this process's recorder. Safe to call when there is none."""
    global _RUNNING
    r, _RUNNING = _RUNNING, None
    if r is not None:
        r.stop()


@contextlib.contextmanager
def recording(write, **kw):
    """Record the plane for the duration. A no-op when this process already is.

    The nesting is not hypothetical: bare `charter` restores through `cmd_reopen`, which
    attaches and therefore wants a recorder of its own, from inside a `cmd_launch` that has
    already started one. Only the caller that actually started it stops it — otherwise the
    inner one would end the outer one's watch on its way out and the frame would be
    unrecorded from then on.
    """
    started = start(write, **kw)
    try:
        yield started
    finally:
        if started is not None:
            stop()


def focus_on(chat: str) -> None:
    """Tell the recorder which chat this terminal is on. A no-op when nothing is
    recording, so a caller never has to ask first."""
    if _RUNNING is not None:
        _RUNNING.chat = chat
