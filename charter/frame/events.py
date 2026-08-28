"""Getting an event to the component that owns a pane — the half `Component.events`
never had.

`component.EVENT_KINDS` has been a closed vocabulary since §4f and, until this module,
nothing read it: a provider could declare ``events = ["scroll"]``, pass validation, and
receive nothing, ever (#607). The decoder already existed — `overlay.decode` turns a
pane's bytes into `overlay.Event` — but it was wired only to the overlay's OWN surface,
the palette and the pickers. Nothing carried an event from a PANEL's pane to the
component drawing in it. This is that carriage.

**Three kinds are delivered and three are not, and the split is measured rather than
chosen.** :data:`DELIVERED` is ``focus``, ``blur`` and ``resize``.

* ``key`` is not delivered because the harness owns the keyboard. tmux routes typing to
  the ACTIVE pane, which is the harness's, so a panel's pane receives keystrokes only
  while the operator has deliberately left the harness — and a component that acted on
  them would be acting on the few the operator typed into the wrong pane.
* ``click`` and ``scroll`` are not delivered because charter cannot yet say what a click
  on a NON-ACTIVE pane means. tmux routes pointer events by POSITION and a click does not
  focus the pane (measured against tmux 3.7c, #605), so the pointer would be a second
  focus, disagreeing with the keyboard's, with no rule anywhere saying which one a
  component is being driven by. The trade is relocated by "only the active pane requests
  mouse", not dissolved. `component.EVENT_KINDS` already promises a declaration is
  "what you HANDLE, never a promise that it FIRES", so both stay declarable and simply do
  not fire — which is the direction that constant asks for: **degrade to "never fires",
  never to "fires wrongly"**.

---

**Input is read off the pane this process CLAIMED, never off `sys.stdin`.** A tmux pane
runs its command with fds 0, 1 and 2 on one pty, so the descriptor `frame/pane.py` holds
as the pane is the descriptor the pane's input arrives on — measured against real tmux
3.7c with the probe's own stdin closed (``0<&-``)::

    fd=1 isatty=True
    DRIVER select probe    READ b'\\x1b[I'
    DRIVER select other    READ b'\\x1b[O'
    DRIVER select probe    READ b'\\x1b[I'

That is #606's fix reused rather than a second one written for the other direction:
`sys.stdin` is a mutable global any library the process imports may replace, and a panel
that read events off a framework's stand-in while painting into the operator's rectangle
would be the same silent split that module exists to close, one descriptor over.

**The tty goes to cbreak and never to raw, and the difference is the whole paint.**
`tty.setraw` clears ``OPOST``/``ONLCR``, and `panel._write` joins its rows with ``\\n``.
Measured, four modes, one 40x10 pane each, read back with `capture-pane`::

    cooked    -> ['AAA', 'BBB', 'CCC']
    raw       -> ['AAA', '   BBB', '      CCC']      <- the staircase
    cbreak    -> ['AAA', 'BBB', 'CCC']
    explicit  -> ['AAA', 'BBB', 'CCC']

So this module clears exactly ``ICANON`` and ``ECHO`` and sets ``VMIN``/``VTIME`` to zero,
and touches no output flag. `frame/palette.py` reaches for `tty.setraw` and is right to:
its surface joins with ``\\r\\n`` because it owns the whole pane. A panel does not, and a
panel that adopted the palette's mode would shear every repaint by a column a row.
``ICANON`` has to go because ``\\x1b[I`` carries no newline and the line discipline would
hold it forever; ``ECHO`` has to go because the terminal would otherwise print the
sequence into the rectangle the component is drawing in.

**Nothing is touched for a component that declared nothing.** A panel whose component
declares no delivered kind builds no dispatcher at all: the tty keeps its mode, no
``\\x1b[?1004h`` is written, and `panel._watch` sleeps exactly as it did before this
module existed. A component declaring only ``resize`` opens no input path either — a
resize is a `SIGWINCH` this process already receives. That scoping is `slots.ANIMATED`'s
argument one mechanism over: a feature that costs every panel is a feature every panel
pays for.

---

**A handler is a notification, not a second renderer.** :data:`Component.on_event`
receives one `Event` and answers truthy for "repaint me". Nothing it returns reaches the
pane — the rows on screen are `render`'s, which `Registry.draw` already escapes, measures
and clips (§4b properties 1 and 3). So the containment order #472 is about is kept by
construction here rather than by a second guard: there is no path from a handler's return
value to a terminal for an escape sequence to travel down.

It is also why a handler is handed no ``ctx``. A component reads the plane at the repaint
that follows, out of the one snapshot §4f gives it; a handler that could read a second one
would be the second timestamp that doctrine exists to prevent. For the same reason a
``resize`` event carries no size: `ctx.width` and `ctx.height` are where a component's
rectangle is read, and a number on the event would be a second answer to that question
which could disagree with the first.

**An event reaches the screen through the repaint path that already exists.** A handler
answering truthy is a fourth reason for `panel._tick` to paint, beside a version bump, a
resize and the spinner — not a second way to draw. The paint that follows is the same
`_write`, the same `Registry.draw`, the same rectangle.

---

**A handler that raises costs its component its events, and the pane says so** (§4b).
`registry.Registry`'s docstring names three moments a provider can fail — on import, while
building, and in ``render`` — and gives one answer to all three: the pane names the
component and what it raised. ``on_event`` is the fourth, and it takes the same answer,
because the alternative is the one this project keeps refusing: a component that quietly
stops being interactive is indistinguishable from one nobody has clicked yet, and #512 is
what a convincing empty costs.

It is retired rather than retried. A handler that raised on one event will raise on the
next, and delivering more would spend the operator's repaints on a loop of identical
failures — so the input path is closed, the mode is put back, and every later event is
dropped. The cost is stated plainly: a component whose ``render`` still works loses its
pane to a message because its HANDLER broke. That is the smaller of the two wrongs, and it
is reversible by relaunching the frame.

**A handler that never returns wedges its own panel, and nothing else** — which is §4b's
promise kept rather than evaded. There is no watchdog and that is deliberate: the panel is
one process holding one pane, so a handler that loops stops that pane repainting and
leaves every sibling pane, the harness and tmux itself untouched. It is exactly what an
infinite loop in ``render`` already does. A `signal.setitimer` watchdog was considered and
refused — it cannot interrupt a C-level block at all, so it would promise a bound it does
not have, and it would add a new way for the `select` and the `write` on either side of a
handler to fail half-way. The operator's way out is unchanged and is not charter's code:
`overlay.HATCH_KEY` is matched in tmux's own root key table before any byte reaches this
process.

**"And nothing else" has one exception and it belongs in the sentence rather than under
it**: a handler that never returns never reaches `panel._watch`'s ``finally`` either, so
that pane keeps the mode :meth:`Dispatcher.open` installed — ``ICANON`` and ``ECHO`` off —
for the life of the process. Typing into it echoes nothing. That is a WEDGED pane looking
wedged rather than a second failure, and it is bounded to the same one pane; but a
docstring that said the pane was simply frozen would be describing a tidier failure than
the one this design actually has.
"""

from __future__ import annotations

import os
import select
import termios
import time

from .. import contain
from . import overlay, pane
from .registry import _because

#: One decoded event, re-exported from the decoder that produces it so a provider can name
#: the type it handles without importing the overlay — which is a modal surface it has
#: nothing to do with. One class, not two: a second `Event` would be a second answer to
#: "what did the terminal just say".
Event = overlay.Event

#: The kinds charter routes to a component today — a subset of `component.EVENT_KINDS`,
#: and see this module's docstring for why the other three are not in it.
#:
#: Named here rather than in `component.py` because the two answer different questions:
#: that tuple is the closed vocabulary a provider may DECLARE (§4f), this is what charter
#: can currently deliver. Merging them would either refuse a component for declaring a kind
#: charter may deliver next release, or promise delivery by the act of declaring — and the
#: EVENT_KINDS docstring already turns down both.
DELIVERED = (overlay.FOCUS, overlay.BLUR, overlay.RESIZE)

#: The delivered kinds that need the pane's own input read. ``resize`` is not among them:
#: a `SIGWINCH` already reaches this process and `pane.size()` already answers the
#: rectangle, so a component that wants only that costs its pane's terminal nothing.
_FROM_INPUT = (overlay.FOCUS, overlay.BLUR)

#: What the pane writes to ask its terminal to report its own focus, and the withdrawal.
#: `overlay.MOUSE_ON`/`MOUSE_OFF` are the same pair for the pointer, one surface over.
#:
#: tmux delivers ``\\x1b[I``/``\\x1b[O`` to a pane's program only when that program has
#: asked, and only while the server's ``focus-events`` is on — both halves measured against
#: real tmux 3.7c, two panes, a real attached client::
#:
#:     focus-events on   pane that asked:      READ b'\\x1b[I' / b'\\x1b[O' on every select
#:                       pane that did not:    nothing, ever
#:     focus-events off  pane that asked:      nothing, ever
#:
#: `commands_frame.conf_text` writes ``set -g focus-events on`` for every frame charter
#: launches (#559), so these fire on charter's own server and do not inside an operator's
#: existing tmux, where charter sources no config at all. `docs/frame.md` says that half to
#: the operator.
FOCUS_ON = "\x1b[?1004h"
FOCUS_OFF = "\x1b[?1004l"

#: How much of the pane is read at once. One `read` never has to return a whole sequence —
#: `overlay.decode` holds a partial one back — so this only bounds how many events one
#: tick can carry. `palette._CHUNK`'s number, for the same reason it is that number.
_CHUNK = 4096

#: What a descriptor that cannot be asked raises. `frame/pane.py`'s own set — a `StringIO`
#: has no `fileno`, a closed stream raises `ValueError`, a real descriptor with no tty
#: behind it raises `OSError`, and a library's stand-in may have no such attribute at all —
#: **plus `termios.error`, which is not any of them.**
#:
#: That last one is measured rather than assumed, and the first version of this module had
#: it wrong::
#:
#:     termios.error.__mro__       -> (termios.error, Exception, BaseException, object)
#:     termios.tcgetattr(/dev/null) -> termios.error(19, 'Operation not supported by device')
#:     isinstance(that, OSError)    -> False
#:
#: `frame/pane.py`'s set is complete for the questions IT asks — `fileno`, `isatty` and
#: `os.get_terminal_size` all raise `OSError` — so this is a superset rather than a
#: disagreement, and the extra member is named here beside the call that needs it. Getting
#: it wrong would have taken a panel with stdout piped to a file (`charter panel
#: acme.metrics --session x > /tmp/log`) from "no focus events, which is fine" to
#: `panel._hold`, painting a refusal for a component that had done nothing wrong.
#:
#: `TypeError` is here for the same reason and was the same miss one step further out.
#: A stream stand-in answers `fileno()` with whatever it likes, and what it answers is
#: handed straight to `termios.tcgetattr`::
#:
#:     termios.tcgetattr(None)   -> TypeError: argument must be an int, or have a fileno()
#:     select.select([None], ..) -> TypeError: argument must be an int, or have a fileno()
#:
#: `frame/pane.py` never meets that case — it passes what it gets to
#: `os.get_terminal_size`, which raises `OSError` — so its set is complete for ITS
#: questions and this one is a superset rather than a disagreement. A `MagicMock` stdout,
#: which is what `mock.patch("sys.stdout")` installs, reaches this exact path.
_CANNOT_SAY = (AttributeError, OSError, TypeError, ValueError, termios.error)


def wanted(c) -> tuple[str, ...]:
    """The kinds charter will actually deliver to component *c*, in declared order.

    The intersection of what it declared and what :data:`DELIVERED` says charter carries —
    empty for a component with no handler, which `component.Component` already refuses to
    build alongside a non-empty ``events``, and empty for one whose every declared kind is
    a kind charter does not fire. That second emptiness is the one that matters: a
    component declaring only ``click`` gets no dispatcher, so its pane's terminal is left
    exactly as it was found.

    ``c.on_event`` rather than a ``getattr`` with a default: every `Component` carries the
    field, so a default here would be a branch nothing can reach and no test can falsify —
    which this project deletes rather than keeps (#568). What makes it unreachable is
    itself pinned, by the case that asks a component declaring neither for its ``on_event``
    and gets ``None``.

    ``is not None`` rather than truthiness, which is the same question `Component`'s own
    validation asks one module over. A callable object defining a falsy ``__bool__`` or an
    empty ``__len__`` — a handler written as an instance of a class rather than as a
    function — passes construction and would silently get no dispatcher here, which is
    #607's defect with a new spelling.
    """
    return tuple(k for k in c.events if k in DELIVERED) \
        if c.on_event is not None else ()


class Dispatcher:
    """The event path for one panel's one component: the pane's input, and the handler.

    One instance per panel process, built by `panel._run` only when :func:`wanted` answers
    something, and opened and closed by `panel._watch` around the loop it lives in.

    An object rather than a closure because it holds four things that must be put back or
    compared later: the tty attributes it displaced, the partial escape sequence it is
    holding, the rectangle it last saw, and whether the handler has already failed.
    """

    def __init__(self, component, *, stream=None) -> None:
        self._c = component
        self._kinds = frozenset(wanted(component))
        #: The pane, resolved once. ``None`` until :meth:`open`, and left ``None`` for a
        #: pane whose input cannot be read at all — see :meth:`open`.
        self._stream = stream
        self._fd: int | None = None
        self._before = None
        self._tail = b""
        self._size = None
        self._failure: str | None = None

    # -- what the panel asks ------------------------------------------------ #

    @property
    def failure(self) -> str | None:
        """Why this component stopped taking events, or ``None`` while it still does.

        Read by `panel._component_text`, which draws it INSTEAD of the component — see
        this module's docstring for the trade that is.
        """
        return self._failure

    @property
    def reading(self) -> bool:
        """Whether this dispatcher currently holds the pane's input open.

        ``False`` before :meth:`open`, for a component that wants only ``resize``, for a
        pane that is not a terminal, and after a handler failure retired it. Asked by tests
        rather than by production, which is the point: "charter touched nothing" is the
        promise a panel with no declared events makes, and a promise nothing can ask about
        is a promise nothing can pin.
        """
        return self._fd is not None

    def open(self) -> None:
        """Take what this dispatcher needs, and nothing it does not.

        Records the pane's rectangle so :meth:`note_resize` has something to compare
        against — always, because ``resize`` needs no input path — and then, only if a kind
        that comes from input was declared, puts the tty in cbreak and asks the terminal to
        report focus.

        **A pane that cannot be asked is left alone and reports nothing**, which is
        `component.EVENT_KINDS`'s "degrade to never fires" rather than a failure: this is
        `charter panel acme.metrics --session x > /tmp/log` run by hand for debugging, or a
        test, and neither has a terminal for a focus change to happen in. It is not a
        provider's fault and must not reach a provider's pane.
        """
        self._size = pane.size()
        if not any(k in self._kinds for k in _FROM_INPUT):
            return
        if self._fd is not None:
            # Already open. Without this a second `open` would read the mode the FIRST one
            # installed and keep it as `_before`, so `close` would "restore" the pane to
            # cbreak — ECHO off, for good. `panel._run` builds one dispatcher per panel so
            # this cannot happen there today; it is a property of the object rather than of
            # its one caller, and the case below is what keeps it one.
            return
        stream = pane.stream() if self._stream is None else self._stream
        try:
            fd = stream.fileno()
            before = termios.tcgetattr(fd)
            mode = termios.tcgetattr(fd)
            # Exactly two lflags and the two control characters, and no output flag at
            # all — see this module's docstring for the measured staircase `tty.setraw`
            # draws here. A second `tcgetattr` rather than a slice of the first, because
            # `tcsetattr` takes the list apart and a shared one would leave *before*
            # describing the mode this call installed instead of the one it displaced.
            mode[3] = mode[3] & ~(termios.ICANON | termios.ECHO)
            mode[6] = list(mode[6])
            mode[6][termios.VMIN] = 0
            mode[6][termios.VTIME] = 0
            termios.tcsetattr(fd, termios.TCSANOW, mode)
        except _CANNOT_SAY:
            # Everything up to here either all happened or changed nothing: the two
            # `tcgetattr`s are reads, and a `tcsetattr` that raised installed nothing. So
            # there is no half-open state to unwind, and `_fd` staying `None` is what says
            # this pane has no input path.
            return
        self._before = before
        self._fd = fd
        self._stream = stream
        self._write(FOCUS_ON)

    def close(self) -> None:
        """Withdraw the request and put the tty back exactly as it was found.

        Idempotent, and it does not raise for any of the ways a pane can be gone
        (:data:`_CANNOT_SAY`) — which matters because both callers are a ``finally``:
        `_watch`'s, which runs before `panel._hold` can paint a reason and hold the pane
        forever, and :meth:`_deliver`'s, which runs while a stranger's exception is being
        turned into a message.

        It is not an unconditional promise, and the honest version is worth more than the
        tidy one: something a stream stand-in raises from outside that set escapes the
        withdrawal below. What that costs is bounded by the order — the mode is already
        back — and `panel._watch` hands its `SIGWINCH` handler back from a nested
        ``finally`` for exactly this case.

        **`TCSANOW` and never `TCSADRAIN`, and this one was measured the hard way.**
        `TCSADRAIN` waits for the terminal to consume everything already written, and on a
        pty whose far end is not reading it simply never returns — the first version of
        this module hung its own test suite there, in a cleanup, with the traceback
        pointing at this line. `frame/palette.py:own_the_tty` uses `TCSADRAIN` and is right
        to: it clears `OPOST`, so output already queued would come out under different
        rules if the restore jumped the queue. This module changes no output flag at all
        (see :meth:`open`), so a drain here protects nothing and can only wedge — and a
        wedge in the `finally` that holds the operator's tty mode is the worst place on the
        path to put one.

        **The RESTORE goes first and the withdrawal second, and the order is the whole
        function.** It was the other way round, with ``_fd``/``_before`` cleared on the very
        first line, and that made "idempotent" mean "the second call is a guaranteed
        no-op": anything that raised or blocked in between lost the `tcsetattr` FOREVER,
        because the retry from `panel._watch`'s `finally` would find ``_fd`` already
        ``None`` and return. And there is something that can block. This clears ``ICANON``
        and ``ECHO`` and deliberately leaves the input flags alone, so ``IXON`` is still on:
        an operator who selects this pane and presses Ctrl-S puts the pty in XOFF, and the
        `flush` inside :meth:`_write` then waits for an XON that may never come. That
        hazard is not new — `panel._out` flushes into the same pane on every paint and has
        always been able to wait there — but doing it AHEAD of the mode restore would have
        been, and it would have left the operator with a pane that echoes nothing and no
        process left to put it back.
        """
        fd, before = self._fd, self._before
        if fd is None:
            return
        try:
            termios.tcsetattr(fd, termios.TCSANOW, before)
        except _CANNOT_SAY:
            pass
        # Only now: the mode the operator cares about is back, so a second call has
        # nothing left to retry and the withdrawal below may fail without costing it.
        self._fd = self._before = None
        try:
            self._write(FOCUS_OFF)
        except _CANNOT_SAY:
            pass

    def note_resize(self) -> None:
        """Deliver a ``resize`` if this pane's rectangle has actually moved.

        Called by `panel._tick` immediately BEFORE the paint, so a handler that recomputes
        a layout has done so by the time `render` is asked for rows — one repaint, not two.

        **The rectangle is what decides, never the signal.** `SIGWINCH` is how the panel
        learns to look, and `panel._watch` seeds its resize flag ``True`` so the first pass
        always paints; a `resize` fired off that flag would announce one before the pane
        had ever been drawn. Comparing `pane.size()` to the last one seen answers the
        question the event is actually about, and answers it correctly for the first tick
        (nothing moved) without a second "have I painted yet" sentinel.

        A rectangle charter could not measure is not one end of a comparison. ``None`` on
        either side fires nothing: a pty gets its size from a `TIOCSWINSZ` that arrives
        moments later, and `panel._unmeasured` means the component was never drawn at the
        earlier size in the first place, so there is nothing for it to have laid out
        against.
        """
        if overlay.RESIZE not in self._kinds:
            return
        was, now = self._size, pane.size()
        if now is not None:
            # Stored only when it IS a rectangle. Overwriting with `None` would spend the
            # comparison: a pty answers "no size" between a resize and the `TIOCSWINSZ`
            # that follows (`frame/pane.py` calls that ordinary, not exotic), and one such
            # reading in the middle would leave `was` as `None` on the next tick — so the
            # resize that actually happened would never fire at all.
            self._size = now
        if was is not None and now is not None and was != now:
            self._deliver(Event(overlay.RESIZE))

    def poll(self, timeout: float) -> bool:
        """Wait up to *timeout* for the pane to say something, and answer whether what it
        said changed what the panel should draw.

        This replaces `panel._watch`'s `time.sleep` rather than being added beside it: a
        `select` with a deadline is the same idle cost as a sleep of the same length, and
        it wakes the moment an event arrives instead of at the next tick boundary. A panel
        with no dispatcher still sleeps — `panel._wait` is the one place that choice is
        made.

        **"The other end is gone" has two spellings and this platform is not the one that
        decides which** — `palette._reader`'s docstring records the measurement that cost a
        red CI: closing a pty's far end makes the next read answer ``b""`` on macOS and
        raise ``OSError: [Errno 5]`` on Linux. Both are end of input, both mean there will
        never be another event, and both close the input path here rather than being
        answered differently on two operators' machines. The panel itself is NOT ended:
        its pane is still a rectangle it can paint into, and a panel that stopped
        repainting because its input closed would be this module taking a pane it was only
        ever lent.
        """
        if self._fd is None:
            time.sleep(timeout)
            return False
        try:
            ready = select.select([self._fd], [], [], timeout)[0]
        except _CANNOT_SAY:
            self.close()
            return False
        if not ready:
            # A quiet period is how every terminal program tells a lone `\x1b` from the
            # start of a sequence (`overlay.decode`'s *final*). Nothing this module
            # delivers is spelled with a bare Escape, so what this actually buys is that a
            # truncated sequence is dropped at the tick rather than held for the life of
            # the panel and prepended to whatever arrives next.
            return self._feed(b"", final=True)
        try:
            chunk = os.read(self._fd, _CHUNK)
        except (BlockingIOError, InterruptedError):
            # Not end of input, and telling them apart matters because the answer here is
            # PERMANENT. `EAGAIN` means "ready a moment ago, nothing now" — a race that
            # exists the instant anything in a provider's import graph puts `O_NONBLOCK`
            # on fd 1 (asyncio adding stdout to an event loop does exactly that) — and
            # `EINTR` is a signal this process handles, `SIGWINCH` being the one it arms
            # itself. Folding either into the branch below would retire a component's
            # events for the life of the panel over a syscall that meant "ask again".
            return False
        except _CANNOT_SAY:
            self.close()
            return False
        if not chunk:
            self.close()
            return False
        return self._feed(chunk, final=False)

    # -- the machinery ------------------------------------------------------ #

    def _feed(self, chunk: bytes, *, final: bool) -> bool:
        """Decode what arrived and hand each event to the component. Repaint?

        The tail is `overlay.decode`'s and is kept here rather than re-derived: a pane's
        `read` returns whatever the kernel had, which splits ``\\x1b[I`` across two reads as
        readily as not.

        ``any`` over a list rather than a generator, deliberately: a component with two
        handlers' worth of state must see EVERY event, and short-circuiting would stop
        delivering at the first one that asked for a repaint.
        """
        evs, self._tail = overlay.decode(self._tail + chunk, final=final)
        return any([self._deliver(ev) for ev in evs])

    def _deliver(self, ev) -> bool:
        """Hand *ev* to the handler if it declared that kind. Repaint?

        A kind the component did not declare is dropped here rather than never decoded, so
        that "what the terminal said" and "what this component asked for" stay two separate
        questions — the decoder answers the first for every panel identically, and only
        this line knows the second.

        **`except BaseException` under `except KeyboardInterrupt: raise` is the pairing
        `Registry.draw` uses, and it means here exactly what it means there**: everything a
        stranger's code does wrong is contained, and the operator's own interrupt is still
        theirs. Do not read `panel._component_text`'s single `except Exception` as the same
        thing — that clause guards charter's own code, one layer up, and #568 removed the
        interrupt arm from it precisely because it could never have fired.
        """
        if self._failure is not None or ev.kind not in self._kinds:
            return False
        try:
            return bool(self._c.on_event(ev))
        except KeyboardInterrupt:
            raise
        except BaseException as exc:
            self._failure = (f"{contain.one_line(self._c.id)} stopped taking events — "
                             f"{_because(exc)}")
            self.close()
            # True, so the pane repaints and says so on the very next tick rather than
            # whenever something else happens to move the frame's version.
            return True

    def _write(self, payload: str) -> None:
        """One mode request, flushed.

        Into the pane, never `sys.stdout` — `panel._out`'s reason, which is `frame/pane.py`
        entire. Not through `panel._write`: that clears the pane and honours `NO_COLOR` by
        stripping every CSI that is not SGR, and both would be wrong for a request that
        draws nothing and is not colour.
        """
        self._stream.write(payload)
        self._stream.flush()
