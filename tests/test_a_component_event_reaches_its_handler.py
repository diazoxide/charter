"""`Component.events` was validated and read by nothing. These are the cases that make
it a delivery (#607).

The gap the issue names, said as a test: a provider could declare ``events = ["scroll"]``,
pass every check `frame/component.py` makes, and receive nothing, ever. So the first
question every case here asks is not "does the dispatcher work" but **"can this component
tell the difference between an event that fired and one that did not"** — which is why
each handler below records what it was handed rather than merely returning ``True``.

**The tty is real everywhere it matters.** The `Dispatcher` cases open a real `pty`, put
the real slave into the mode `frame/events.py` chooses, write the real bytes tmux writes,
and read the real mode back with `termios.tcgetattr` — the shape `test_frame_palette.py`
already uses for `own_the_tty`. A mocked `select` would prove the mock returns what the
test told it to; it would not have caught `termios.error` failing to be an `OSError`,
which is the one defect the first version of that module actually had.

The real tmux half — that tmux delivers ``\\x1b[I`` to a panel's pane at all, and only to
one that asked — is `tests/test_frame_focus_reaches_a_component.py`.
"""

from __future__ import annotations

import fcntl
import io
import os
import pty
import struct
import termios
import time
import unittest
from unittest import mock

from charter.frame import component, events, overlay, panel

from tests.test_component_providers import _source


def _component(**kw):
    """A component with a handler that records every event it is handed."""
    seen = []
    base = dict(id="acme.metrics", title="Metrics", edge="right",
                size=component.Fixed(4), render=lambda ctx: ["drew"],
                events=("focus", "blur"), on_event=lambda ev: seen.append(ev) or True)
    base.update(kw)
    c = component.Component(**base)
    return c, seen


class _Pty:
    """One real pty, and the pane's own end of it.

    *slave* is what a tmux pane's program is given — the descriptor `frame/events.py`
    reads and sets the mode on. *master* is the terminal side: writing to it is what tmux
    does when it delivers a focus report, and reading it is how a case sees what the pane
    asked the terminal for.
    """

    def __init__(self, case: unittest.TestCase, *, cols: int = 40, rows: int = 8) -> None:
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ,
                    struct.pack("HHHH", rows, cols, 0, 0))
        self.stream = os.fdopen(self.slave, "w", buffering=1, closefd=False)
        case.addCleanup(self._close)

    def sent(self) -> bytes:
        """Whatever the pane has written to its terminal and not yet been asked about."""
        if not _readable(self.master):
            return b""
        return os.read(self.master, 4096)

    def deliver(self, payload: bytes) -> None:
        """What tmux does: put bytes on the pane's input."""
        os.write(self.master, payload)

    def _close(self) -> None:
        # The wrapper first, and before the descriptors it sits on. A `TextIOWrapper`
        # flushes when it is finalised, and a case that closed the master to reach end of
        # input leaves it holding bytes for a descriptor that has gone — which surfaces as
        # `Exception ignored while finalizing file` on a later, unrelated test.
        try:
            self.stream.close()
        except OSError:
            pass
        for fd in (self.master, self.slave):
            try:
                os.close(fd)
            except OSError:
                pass


def _readable(fd: int, timeout: float = 0.5) -> bool:
    import select
    return bool(select.select([fd], [], [], timeout)[0])


#: What the KERNEL sets on its own, so a case comparing two modes does not read it as a
#: change charter made. macOS raises `PENDIN` (0x20000000) whenever input is queued for
#: re-processing, which is exactly what these cases arrange — `tests/test_frame_palette.py`
#: masks the same bit against `own_the_tty` for the same reason. `getattr` because it is
#: not defined on every platform's `termios`, and a missing bit masks nothing.
_KERNELS_OWN = getattr(termios, "PENDIN", 0)


def _mode(fd: int) -> list:
    """A tty's mode with the kernel's own bookkeeping bit masked out."""
    m = termios.tcgetattr(fd)
    return [m[0], m[1], m[2], m[3] & ~_KERNELS_OWN]


def _lflags(fd: int) -> int:
    return termios.tcgetattr(fd)[3] & ~_KERNELS_OWN


class TheDeclarationAndTheHandlerAreOneThing(unittest.TestCase):
    """#607's own shape, refused at construction where the message names a fixable thing.

    A declaration with nothing behind it IS the defect this issue is about, written by the
    provider instead of by charter — so it is refused in both directions rather than
    accepted and quietly never delivered.
    """

    def test_declaring_events_with_no_handler_is_refused_and_names_the_kinds(self):
        with self.assertRaises(component.ComponentError) as e:
            _component(events=("scroll",), on_event=None)
        self.assertIn("scroll", str(e.exception))
        self.assertIn("on_event", str(e.exception))

    def test_a_handler_with_no_declaration_is_refused(self):
        """A callable nothing would ever call is not a component that works — it is one
        whose author believes it is interactive."""
        with self.assertRaises(component.ComponentError) as e:
            _component(events=(), on_event=lambda ev: None)
        self.assertIn("events", str(e.exception))

    def test_a_handler_that_is_not_callable_is_refused(self):
        with self.assertRaises(component.ComponentError) as e:
            _component(on_event="not a function")
        self.assertIn("callable", str(e.exception))

    def test_the_pair_together_is_accepted(self):
        c, _seen = _component()
        self.assertEqual(c.events, ("focus", "blur"))
        self.assertTrue(callable(c.on_event))

    def test_a_component_without_either_is_untouched(self):
        """Every component charter itself registers is this shape, so the refusals above
        must not reach one."""
        c, _seen = _component(events=(), on_event=None)
        self.assertEqual(c.events, ())
        self.assertIsNone(c.on_event)


class CharterDeliversThreeOfTheSixAndSaysWhich(unittest.TestCase):
    def test_delivered_is_a_subset_of_the_closed_vocabulary(self):
        """`EVENT_KINDS` is what may be DECLARED and `DELIVERED` is what charter carries.
        Two lists, and the smaller one must never contain a name the larger does not — a
        kind charter delivered that a provider could not declare would be undeliverable."""
        for kind in events.DELIVERED:
            self.assertIn(kind, component.EVENT_KINDS)

    def test_the_undelivered_three_are_still_declarable(self):
        """`EVENT_KINDS`'s own rule: declaring is what you HANDLE, never a promise that it
        FIRES. A component that declares `click` today must not be refused for it."""
        for kind in ("key", "click", "scroll"):
            self.assertNotIn(kind, events.DELIVERED)
            c, _seen = _component(events=(kind,))
            self.assertEqual(c.events, (kind,))

    def test_wanted_is_the_intersection_in_declared_order(self):
        c, _seen = _component(events=("resize", "click", "focus"))
        self.assertEqual(events.wanted(c), ("resize", "focus"))

    def test_a_component_charter_delivers_nothing_to_wants_nothing(self):
        """The cost promise: `click` alone builds no dispatcher, so nothing touches the
        pane's terminal."""
        c, _seen = _component(events=("click", "scroll"))
        self.assertEqual(events.wanted(c), ())

    def test_a_component_with_no_handler_wants_nothing(self):
        c, _seen = _component(events=(), on_event=None)
        self.assertEqual(events.wanted(c), ())


class TheWithdrawalUndoesExactlyWhatTheRequestAsked(unittest.TestCase):
    """The two mode constants are a PAIR, and the cases that watch them being written can
    only ever compare a constant to itself.

    `FOCUS_ON`'s value is pinned for real by `tests/test_frame_focus_reaches_a_component.py`
    — a wrong private-mode number there means tmux sends nothing and the whole integration
    class goes red. `FOCUS_OFF` has no such witness: nothing observes a pane after charter
    has stopped reading it. So what is asked here is the RELATIONSHIP, which is where the
    realistic defect lives — a typo in one number of one of the two.

    It is not cosmetic. A `FOCUS_OFF` that withdrew a different mode would leave tmux
    reporting focus at a pane nobody is reading after a handler failure retired it, filling
    that pty's input buffer for the life of the frame.
    """

    def test_off_is_on_with_the_low_letter(self):
        self.assertTrue(events.FOCUS_ON.endswith("h"))
        self.assertEqual(events.FOCUS_OFF, events.FOCUS_ON[:-1] + "l")

    def test_it_is_a_private_mode_set_and_not_something_else(self):
        self.assertTrue(events.FOCUS_ON.startswith("\x1b[?"))

    def test_it_is_the_pair_the_overlay_does_not_already_own(self):
        """`overlay.MOUSE_ON` is the same shape for the pointer. Asking that these are not
        it keeps a copy-paste from arming mouse reporting on every panel that wanted to
        know whether it was focused — which would take the operator's text selection
        without `[frame] mouse` ever being set."""
        self.assertNotIn(events.FOCUS_ON, overlay.MOUSE_ON)
        self.assertNotIn("1000", events.FOCUS_ON)
        self.assertNotIn("1006", events.FOCUS_ON)


class TheDecoderNamesFocusAndBlur(unittest.TestCase):
    """The two sequences `overlay.decode` used to consume and drop."""

    def test_focus_in_and_out_decode_to_their_own_kinds(self):
        evs, tail = overlay.decode(b"\x1b[I\x1b[O")
        self.assertEqual([e.kind for e in evs], [overlay.FOCUS, overlay.BLUR])
        self.assertEqual(tail, b"")

    def test_neither_is_reported_as_a_keypress(self):
        """Before this they fell through to `_CSI_KEYS`, which has no name for either.
        Falling through to the SINGLE-BYTE path instead would have typed an `I`."""
        evs, _tail = overlay.decode(b"\x1b[I")
        self.assertNotEqual(evs[0].kind, overlay.KEY)

    def test_a_parameterised_form_is_not_a_focus_report(self):
        """`CSI 3 I` is ECMA-48's CHT, a cursor movement some program echoed. Reading it
        as three focus events would be `EVENT_KINDS`'s "fires wrongly"."""
        evs, tail = overlay.decode(b"\x1b[3I")
        self.assertEqual(evs, [])
        self.assertEqual(tail, b"")

    def test_a_report_split_across_two_reads_is_still_one_event(self):
        """A pane's `read` returns whatever the kernel had, and splits `\\x1b[I` as
        readily as not."""
        first, tail = overlay.decode(b"\x1b[")
        self.assertEqual(first, [])
        second, tail = overlay.decode(tail + b"I")
        self.assertEqual([e.kind for e in second], [overlay.FOCUS])

    def test_the_application_cursor_form_is_untouched(self):
        """`\\x1bOA` is SS3 Up and must not be read as a blur because it contains an `O`."""
        evs, _tail = overlay.decode(b"\x1bOA")
        self.assertEqual([(e.kind, e.name) for e in evs], [(overlay.KEY, "up")])


class TheOverlayIgnoresTheKindsItNeverAsksFor(unittest.TestCase):
    """The shared-machinery guard. `overlay.py` is the palette's and the pickers'; this
    branch added two kinds to its decoder, so a case has to prove the palette cannot start
    acting on one. It never receives one in production — the overlay is the ACTIVE zoomed
    pane and never writes `\\x1b[?1004h` — but "never receives" is not "would be harmless
    if it did"."""

    def _surface(self, mouse: bool) -> overlay.Surface:
        return overlay.Surface(rows=(overlay.Row(id="a", title="A"),
                                     overlay.Row(id="b", title="B")), mouse=mouse)

    def test_a_focus_event_neither_chooses_nor_cancels_nor_moves(self):
        for mouse in (False, True):
            for kind in (overlay.FOCUS, overlay.BLUR):
                with self.subTest(mouse=mouse, kind=kind):
                    s = self._surface(mouse)
                    self.assertIsNone(s.handle(overlay.Event(kind), 10))
                    self.assertEqual(s.selected.id, "a")


class ADispatcherTakesOnlyWhatItNeeds(unittest.TestCase):
    """What a panel's pane costs, per declaration."""

    def setUp(self):
        self.tty = _Pty(self)

    def test_a_resize_only_component_never_opens_the_panes_input(self):
        """A `SIGWINCH` already reaches this process, so `resize` costs the terminal
        nothing — no mode change and no `\\x1b[?1004h`."""
        c, _seen = _component(events=("resize",))
        before = _lflags(self.tty.slave)
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.assertFalse(d.reading)
        self.assertEqual(_lflags(self.tty.slave), before)
        self.assertEqual(self.tty.sent(), b"")

    def test_focus_opens_it_and_asks_the_terminal_to_report(self):
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.assertTrue(d.reading)
        self.assertEqual(self.tty.sent(), events.FOCUS_ON.encode())

    def test_it_clears_icanon_and_echo_and_leaves_the_output_flags_alone(self):
        """`tty.setraw` would clear `OPOST`/`ONLCR` too, and `panel._write` joins its rows
        with `\\n` — measured in a real 40x10 tmux pane, that draws a staircase:
        `['AAA', '   BBB', '      CCC']`."""
        before = termios.tcgetattr(self.tty.slave)
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        after = termios.tcgetattr(self.tty.slave)
        self.assertFalse(after[3] & termios.ICANON)
        self.assertFalse(after[3] & termios.ECHO)
        self.assertEqual(after[1], before[1], "an output flag moved")
        self.assertTrue(before[1] & termios.OPOST, "the fixture was not in cooked mode")
        self.assertEqual(after[6][termios.VMIN], 0)
        self.assertEqual(after[6][termios.VTIME], 0)

    def test_close_puts_the_mode_back_and_withdraws_the_request(self):
        before = _mode(self.tty.slave)
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.tty.sent()
        d.close()
        self.assertEqual(_mode(self.tty.slave), before)
        self.assertEqual(self.tty.sent(), events.FOCUS_OFF.encode())
        self.assertFalse(d.reading)

    def test_close_returns_even_when_nothing_is_draining_the_pane(self):
        """`TCSADRAIN` waits for the terminal to consume what is already written, and on a
        pane whose far end is not reading it never returns — the first version of this
        module hung its own suite there, in a cleanup, with the traceback on that line.

        Asserted on a THREAD with a deadline rather than by calling `close` directly: a
        regression that reinstated the drain would otherwise be a suite that hangs, and a
        hang is an `unresolved`, not a red.
        """
        import threading

        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()                       # writes FOCUS_ON, which nothing reads back
        done = threading.Event()
        t = threading.Thread(target=lambda: (d.close(), done.set()), daemon=True)
        t.start()
        self.assertTrue(done.wait(5.0), "close() blocked draining a pane nobody reads")

    def test_close_is_idempotent(self):
        """Both callers are a `finally`, and one of them runs after a handler failure has
        already closed it."""
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        d.close()
        d.close()

    def test_a_pane_that_is_not_a_terminal_is_left_alone_rather_than_refused(self):
        """`charter panel acme.metrics --session x > /tmp/log`, run by hand for debugging.
        `termios.error` is not an `OSError` — measured — so a guard written against
        `OSError` alone turns this into `panel._hold` painting a refusal for a component
        that did nothing wrong."""
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=io.StringIO())
        d.open()
        self.assertFalse(d.reading)
        d.close()

    def test_a_pane_behind_a_real_descriptor_with_no_tty_is_the_same_answer(self):
        fd = os.open(os.devnull, os.O_RDWR)
        self.addCleanup(os.close, fd)
        stream = os.fdopen(fd, "w", closefd=False)
        with self.assertRaises(termios.error):
            termios.tcgetattr(fd)          # the measurement this case exists for
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=stream)
        d.open()
        self.assertFalse(d.reading)


class AnEventReachesTheComponentThatOwnsThePane(unittest.TestCase):
    def setUp(self):
        self.tty = _Pty(self)

    def _open(self, c):
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.sent()
        return d

    def test_a_focus_report_on_the_pane_reaches_the_handler(self):
        """The whole issue, in one case: bytes on the pane, an `Event` in the component."""
        c, seen = _component(events=("focus", "blur"))
        d = self._open(c)
        self.tty.deliver(b"\x1b[I")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.FOCUS])

    def test_blur_arrives_as_its_own_kind(self):
        c, seen = _component(events=("focus", "blur"))
        d = self._open(c)
        self.tty.deliver(b"\x1b[O")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.BLUR])

    def test_a_kind_the_component_did_not_declare_is_dropped(self):
        """Decoding is one question for every panel; what a component asked for is
        another, and only the dispatcher knows the second."""
        c, seen = _component(events=("focus",))
        d = self._open(c)
        self.tty.deliver(b"\x1b[O\x1b[I")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.FOCUS])

    def test_a_keypress_never_reaches_a_handler_that_asked_for_focus(self):
        """The sharpest consequence of the declared-kind filter, and the one an operator
        would feel.

        Once a panel asks for focus reporting it is reading its pane, and a pane that is
        ACTIVE receives whatever the operator types. `key` is not a kind charter delivers —
        the harness owns the keyboard — so a handler that declared `focus` must not be
        handed the characters somebody typed into the wrong pane.
        """
        c, seen = _component(events=("focus",))
        d = self._open(c)
        self.tty.deliver(b"hello\r\x1b[A\x1b[I")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.FOCUS])

    def test_a_handler_that_answers_falsy_costs_no_repaint(self):
        """A component that ignores an event must not make the panel redraw for it."""
        c, _seen = _component(events=("focus",), on_event=lambda ev: False)
        d = self._open(c)
        self.tty.deliver(b"\x1b[I")
        self.assertFalse(d.poll(1.0))

    def test_every_event_in_one_read_is_delivered_even_after_one_asks_to_repaint(self):
        """A generator with `any` would stop at the first truthy handler and drop the
        rest, leaving a component's state describing an event that was superseded."""
        c, seen = _component(events=("focus", "blur"))
        d = self._open(c)
        self.tty.deliver(b"\x1b[I\x1b[O\x1b[I")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen],
                         [overlay.FOCUS, overlay.BLUR, overlay.FOCUS])

    def test_a_report_split_across_two_polls_is_still_one_event(self):
        c, seen = _component(events=("focus",))
        d = self._open(c)
        self.tty.deliver(b"\x1b[")
        d.poll(0.3)
        self.tty.deliver(b"I")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.FOCUS])

    def test_a_quiet_tick_answers_no_repaint_and_holds_the_pane(self):
        c, seen = _component(events=("focus",))
        d = self._open(c)
        self.assertFalse(d.poll(0.05))
        self.assertEqual(seen, [])
        self.assertTrue(d.reading)

    def test_end_of_input_closes_the_path_and_does_not_end_the_panel(self):
        """The pane is still a rectangle this process can paint into. A panel that stopped
        repainting because its input closed would be taking a pane it was only lent."""
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        os.close(self.tty.master)
        self.tty.master = -1
        for _ in range(10):
            if not d.reading:
                break
            d.poll(0.05)
        self.assertFalse(d.reading)


class AResizeIsTheRectangleMovingAndNotTheSignal(unittest.TestCase):
    def setUp(self):
        self.tty = _Pty(self)

    def _dispatcher(self, c, size):
        with mock.patch("charter.frame.pane.size", return_value=size):
            d = events.Dispatcher(c, stream=self.tty.stream)
            d.open()
        self.addCleanup(d.close)
        return d

    def test_a_first_pass_fires_nothing(self):
        """`panel._watch` seeds its resize flag True so the first pass always paints. A
        `resize` fired off that flag would announce one before the pane had been drawn."""
        c, seen = _component(events=("resize",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))):
            d.note_resize()
        self.assertEqual(seen, [])

    def test_a_rectangle_that_moved_fires_one(self):
        c, seen = _component(events=("resize",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((60, 8))):
            d.note_resize()
        self.assertEqual([e.kind for e in seen], [overlay.RESIZE])

    def test_it_fires_once_per_move_and_not_once_per_paint(self):
        c, seen = _component(events=("resize",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((60, 8))):
            d.note_resize()
            d.note_resize()
        self.assertEqual(len(seen), 1)

    def test_an_unmeasurable_rectangle_is_not_one_end_of_a_comparison(self):
        """`panel._unmeasured` means the component was never drawn at the earlier size, so
        there is nothing for it to have laid out against."""
        c, seen = _component(events=("resize",))
        d = self._dispatcher(c, None)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((60, 8))):
            d.note_resize()
        self.assertEqual(seen, [])
        with mock.patch("charter.frame.pane.size", return_value=None):
            d.note_resize()
        self.assertEqual(seen, [])

    def test_a_component_that_did_not_declare_resize_gets_none(self):
        c, seen = _component(events=("focus",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((60, 8))):
            d.note_resize()
        self.assertEqual(seen, [])

    def test_it_does_not_even_measure_the_pane_for_a_component_that_did_not_ask(self):
        """`note_resize` runs on every paint, so its own gate has to come BEFORE the
        measurement rather than relying on the delivery filter to drop the event.

        Without this the outcome is identical and the cost is not: every paint of every
        focus-only panel would pay an `os.get_terminal_size` for an event nothing would
        receive. `slots.ANIMATED`'s short-circuit is the same argument — "never repaints
        for the spinner and never even pays the `stat` that would have told it to".
        """
        c, _seen = _component(events=("focus",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((60, 8))) as measured:
            d.note_resize()
            d.note_resize()
        measured.assert_not_called()


class AHandlerThatRaisesCostsItsComponentItsEvents(unittest.TestCase):
    """§4b's fourth moment. `registry.Registry` names three — on import, while building,
    in `render` — and gives one answer to all of them: the pane says which component
    failed and why. `on_event` takes the same answer."""

    def setUp(self):
        self.tty = _Pty(self)

    def _boom(self, events_=("focus", "blur")):
        c, seen = _component(
            events=events_,
            on_event=lambda ev: (_ for _ in ()).throw(KeyError("no such row")))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.sent()
        return c, d

    def test_the_failure_names_the_component_and_what_it_raised(self):
        _c, d = self._boom()
        self.tty.deliver(b"\x1b[I")
        self.assertTrue(d.poll(1.0))
        self.assertIn("acme.metrics", d.failure)
        self.assertIn("KeyError", d.failure)

    def test_it_asks_for_a_repaint_so_the_pane_says_so_on_the_next_tick(self):
        _c, d = self._boom()
        self.tty.deliver(b"\x1b[I")
        self.assertTrue(d.poll(1.0),
                        "the pane would keep the old content until something else moved")

    def test_the_input_path_is_closed_and_the_mode_put_back(self):
        before = _mode(self.tty.slave)
        _c, d = self._boom()
        self.assertNotEqual(_lflags(self.tty.slave), before[3])
        self.tty.deliver(b"\x1b[I")
        d.poll(1.0)
        self.assertFalse(d.reading)
        self.assertEqual(_mode(self.tty.slave), before)

    def test_no_later_event_reaches_it(self):
        """Retired rather than retried: a handler that raised on one event will raise on
        the next, and delivering more spends the operator's repaints on a loop."""
        calls = []

        def boom(ev):
            calls.append(ev)
            raise RuntimeError("still broken")

        c, _seen = _component(events=("focus", "resize"), on_event=boom)
        d = events.Dispatcher(c, stream=self.tty.stream)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))):
            d.open()
        self.addCleanup(d.close)
        self.tty.deliver(b"\x1b[I")
        d.poll(1.0)
        self.assertEqual(len(calls), 1)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((99, 8))):
            d.note_resize()
        self.assertEqual(len(calls), 1)

    def test_the_operators_interrupt_is_not_a_component_failure(self):
        """`except BaseException` under `except KeyboardInterrupt: raise` — the pairing
        `Registry.draw` uses, meaning here exactly what it means there."""
        c, _seen = _component(
            events=("focus",),
            on_event=lambda ev: (_ for _ in ()).throw(KeyboardInterrupt()))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.deliver(b"\x1b[I")
        with self.assertRaises(KeyboardInterrupt):
            d.poll(1.0)
        self.assertIsNone(d.failure)

    def test_a_system_exit_is_contained_like_any_other_failure(self):
        """A provider calling `sys.exit()` in a handler must not end the panel — that is
        §4b's "costs its own pane, never the session", said for the one exception a
        stranger is most likely to reach for by accident."""
        c, _seen = _component(
            events=("focus",),
            on_event=lambda ev: (_ for _ in ()).throw(SystemExit(3)))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.deliver(b"\x1b[I")
        d.poll(1.0)
        self.assertIn("SystemExit", d.failure)


class ThePanelPaintsWhatTheEventChanged(unittest.TestCase):
    """The loop half: how an event reaches the screen, and how it does not."""

    def test_a_handled_event_is_a_reason_to_paint_and_the_only_new_one(self):
        painted = []
        with mock.patch("charter.frame.state.version", return_value="v1"):
            seen = panel._tick({"flag": False}, "v1", "top", "f-1",
                               paint=lambda name, fid: painted.append(name),
                               handled=True)
        self.assertEqual(painted, ["top"])
        self.assertEqual(seen, "v1")

    def test_nothing_handled_and_nothing_moved_paints_nothing(self):
        painted = []
        with mock.patch("charter.frame.state.version", return_value="v1"):
            panel._tick({"flag": False}, "v1", "top", "f-1",
                        paint=lambda name, fid: painted.append(name))
        self.assertEqual(painted, [])

    def test_a_resize_is_delivered_before_the_paint_and_not_after(self):
        """One repaint, not one that is wrong and one that corrects it."""
        order = []

        class _Evs:
            def note_resize(self_):
                order.append("event")

        panel._tick({"flag": True}, "v1", "top", "f-1", events=_Evs(),
                    paint=lambda name, fid: order.append("paint"))
        self.assertEqual(order, ["event", "paint"])

    def test_a_panel_with_no_dispatcher_sleeps_exactly_as_it_did(self):
        with mock.patch("charter.frame.panel.time.sleep") as slept:
            self.assertFalse(panel._wait(None, 0.2))
        slept.assert_called_once_with(0.2)

    def test_a_panel_with_one_polls_instead_of_sleeping(self):
        """A second mechanism beside the sleep would be two clocks to keep in step."""
        polled = []

        class _Evs:
            def poll(self_, timeout):
                polled.append(timeout)
                return True

        with mock.patch("charter.frame.panel.time.sleep") as slept:
            self.assertTrue(panel._wait(_Evs(), 0.2))
        self.assertEqual(polled, [0.2])
        slept.assert_not_called()

    def test_a_retired_handler_makes_the_pane_say_so_instead_of_drawing(self):
        class _Evs:
            failure = "acme.metrics stopped taking events — KeyError: 'no such row'"

        with mock.patch("charter.frame.slots._width", return_value=78):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertIn("acme.metrics", got)
        self.assertIn("KeyError", got)

    def test_the_reason_is_contained_before_it_is_measured(self):
        """#472's order. The reason quotes a stranger's exception text, and an escape
        sequence in it is an instruction to the terminal rather than a character."""
        class _Evs:
            failure = "acme.metrics stopped taking events — KeyError: 'a\x1b[2Jb'"

        with mock.patch("charter.frame.slots._width", return_value=200):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertNotIn("\x1b[2J", got)

    def test_a_component_still_taking_events_draws_its_own_rows(self):
        class _Evs:
            failure = None

        class _Reg:
            def get(self_, cid):
                return component.Component(
                    id=cid, title="M", edge="right", size=component.Fixed(4),
                    render=lambda ctx: ["metrics 42"])

            def draw(self_, cid, c):
                return ("metrics 42",)

        with mock.patch("charter.frame.slots.content_width", return_value=30), \
             mock.patch("charter.frame.slots.inset_rows", side_effect=lambda t, n: t), \
             mock.patch("charter.frame.panel._rows", return_value=6):
            got = panel._component_text(_Reg(), "acme.metrics", "f-1", evs=_Evs())
        self.assertEqual(got, "metrics 42")


class ThePanelOpensAndClosesTheEventPathAroundItsLoop(unittest.TestCase):
    class _Evs:
        def __init__(self):
            self.log = []

        def open(self):
            self.log.append("open")

        def close(self):
            self.log.append("close")

        def note_resize(self):
            pass

        def poll(self, timeout):
            return False

    def test_once_true_opens_and_closes(self):
        evs = self._Evs()
        with mock.patch("charter.frame.state.version", return_value="v1"):
            panel._watch("top", "f-1", once=True, evs=evs,
                         paint=lambda name, fid: None)
        self.assertEqual(evs.log, ["open", "close"])

    def test_a_loop_that_raises_still_puts_the_tty_back(self):
        """`panel._hold` never returns, so a mode left changed here is a pane in a state
        nothing on the machine puts back."""
        evs = self._Evs()

        def explode(_name, _fid):
            raise RuntimeError("paint blew up")

        with mock.patch("charter.frame.state.version", return_value="v1"):
            with self.assertRaises(RuntimeError):
                panel._watch("top", "f-1", once=True, evs=evs, paint=explode)
        self.assertEqual(evs.log, ["open", "close"])


class APanelBuildsNoDispatcherForAComponentThatDeclaredNothing(unittest.TestCase):
    """The cost promise, asked of the function that decides it."""

    class _Reg:
        def __init__(self, c):
            self._c = c

        def get(self, cid):
            return self._c

    def test_a_component_with_no_events_gets_none(self):
        c, _seen = _component(events=(), on_event=None)
        self.assertIsNone(panel._dispatcher(self._Reg(c), c.id))

    def test_a_component_declaring_only_undelivered_kinds_gets_none(self):
        c, _seen = _component(events=("click", "scroll"))
        self.assertIsNone(panel._dispatcher(self._Reg(c), c.id))

    def test_a_component_declaring_focus_gets_one(self):
        c, _seen = _component(events=("focus",))
        self.assertIsInstance(panel._dispatcher(self._Reg(c), c.id),
                              events.Dispatcher)


class TheCommittedRectangleDoesNotCostAComponentItsHandler(unittest.TestCase):
    """`Registry.place` re-builds a provider's component through `dataclasses.replace` to
    apply the committed `edge` and `size` (*arrangement is committed, execution is local*).

    That rebuild re-runs `__post_init__`, so the new pair of refusals runs against a
    component nobody re-declared — and a field dropped there would turn every configured
    provider into a load-time refusal naming a declaration its author did write.
    """

    def test_replacing_the_rectangle_keeps_both_halves(self):
        import dataclasses

        c, seen = _component(events=("focus",))
        moved = dataclasses.replace(c, edge="bottom", size=component.Fixed(2))
        self.assertEqual(moved.edge, "bottom")
        self.assertEqual(moved.events, ("focus",))
        self.assertIs(moved.on_event, c.on_event)
        moved.on_event(overlay.Event(overlay.FOCUS))
        self.assertEqual([e.kind for e in seen], [overlay.FOCUS])

    def test_the_registry_places_one_and_the_dispatcher_still_finds_it(self):
        from charter.frame import registry

        reg = registry.Registry()
        c, _seen = _component(events=("focus",))
        reg.register(c)
        placed = reg.place(c.id, edge="bottom", size=component.Fixed(2))
        self.assertEqual(events.wanted(placed), ("focus",))


class AProviderCanDeclareAHandlerAndCharterBuildsIt(unittest.TestCase):
    """The provider seam, end to end through a real installed distribution's source.

    `_source` is `tests.test_component_providers`'s, so what is exercised here is the
    module text a real `.dist-info` would import — not a `Component` this file built by
    hand, which could not catch a keyword the dataclass no longer takes.
    """

    def test_the_module_a_provider_ships_builds_a_component_with_a_handler(self):
        ns: dict = {}
        exec(_source(events=("focus", "blur"),
                     on_event="lambda ev: True"), ns)
        c = ns["metrics"]()
        self.assertEqual(c.events, ("focus", "blur"))
        self.assertEqual(events.wanted(c), ("focus", "blur"))
        self.assertTrue(c.on_event(overlay.Event(overlay.FOCUS)))

    def test_a_handler_can_change_what_the_next_render_draws(self):
        """The point of the whole path: an event that changes what a component SHOWS."""
        ns: dict = {}
        exec(_source(
            events=("focus", "blur"),
            head="state = {'on': False}",
            on_event="lambda ev: state.__setitem__('on', ev.kind == 'focus') or True",
            render="lambda ctx: ['FOCUSED' if state['on'] else 'idle']"), ns)
        c = ns["metrics"]()
        self.assertEqual(c.render(None), ["idle"])
        c.on_event(overlay.Event(overlay.FOCUS))
        self.assertEqual(c.render(None), ["FOCUSED"])
        c.on_event(overlay.Event(overlay.BLUR))
        self.assertEqual(c.render(None), ["idle"])


if __name__ == "__main__":
    unittest.main()
