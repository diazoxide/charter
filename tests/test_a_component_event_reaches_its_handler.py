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
one that asked — is `tests/test_frame_input_reaches_a_component.py`.
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

from charter import tui
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

    def test_the_refusal_contains_what_it_quotes_back(self):
        """The refusal repeats the value it would not take, and a provider's object
        answers `repr` with whatever it likes. A message is display text that reaches a
        terminal (`component.py`'s module docstring is entirely about this), so an escape
        sequence in that `repr` is an instruction rather than a character — and a refusal
        that could forge a second line is worse than the value it refuses."""
        class _Forges:
            def __repr__(self_):
                return "harmless\x1b[2J\x1b[Hgone"

        with self.assertRaises(component.ComponentError) as e:
            _component(on_event=_Forges())
        self.assertNotIn("\x1b[2J", str(e.exception))
        self.assertIn("harmless", str(e.exception))

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


class CharterDeliversFiveOfTheSixAndSaysWhich(unittest.TestCase):
    def test_delivered_is_a_subset_of_the_closed_vocabulary(self):
        """`EVENT_KINDS` is what may be DECLARED and `DELIVERED` is what charter carries.
        Two lists, and the smaller one must never contain a name the larger does not — a
        kind charter delivered that a provider could not declare would be undeliverable."""
        for kind in events.DELIVERED:
            self.assertIn(kind, component.EVENT_KINDS)

    def test_the_one_charter_does_not_carry_is_still_declarable(self):
        """`EVENT_KINDS`'s own rule: declaring is what you HANDLE, never a promise that it
        FIRES. `key` is the one left, because the harness owns the keyboard — a component
        that declares it must not be refused for it."""
        self.assertNotIn("key", events.DELIVERED)
        c, _seen = _component(events=("key",))
        self.assertEqual(c.events, ("key",))

    def test_every_kind_read_off_the_pane_is_a_kind_charter_delivers(self):
        """`_FROM_INPUT` is what makes `open` take the pane's input and change its terminal
        mode. A kind in it that `DELIVERED` does not carry would put a pane in cbreak, ask
        its terminal to report, decode what arrived and then drop every event — the cost of
        the feature with none of it, on a provider's pane."""
        for kind in events._FROM_INPUT:
            self.assertIn(kind, events.DELIVERED)

    def test_the_pointer_is_carried_now(self):
        """The whole of this change, at the one constant that decides it. `click` and
        `scroll` were declarable and delivered nowhere for a release; they are delivered
        now, and `frame/events.py`'s module docstring carries the measurement that made
        the difference — a pointer over a non-active pane acts where it points and moves
        no focus, on tmux 3.7c and at the 3.2 floor alike."""
        for kind in ("click", "scroll"):
            self.assertIn(kind, events.DELIVERED)

    def test_wanted_is_the_intersection_in_declared_order(self):
        c, _seen = _component(events=("resize", "key", "focus"))
        self.assertEqual(events.wanted(c), ("resize", "focus"))

    def test_a_component_charter_delivers_nothing_to_wants_nothing(self):
        """The cost promise: `key` alone builds no dispatcher, so nothing touches the
        pane's terminal."""
        c, _seen = _component(events=("key",))
        self.assertEqual(events.wanted(c), ())

    def test_a_component_with_no_handler_wants_nothing(self):
        c, _seen = _component(events=(), on_event=None)
        self.assertEqual(events.wanted(c), ())

    def test_a_handler_that_is_falsy_is_still_a_handler(self):
        """`wanted` asks nothing about the handler now — `Component` refuses the pair apart,
        so a component with none has no events either and the intersection is already `()`.

        This case is what stops that check coming back. A handler written as an instance
        with a falsy `__bool__` or an empty `__len__` is a callable `Component` accepts, and
        any `if c.on_event:` reintroduced here would silently give it no dispatcher —
        #607's defect with a new spelling. It went red for both spellings this branch
        shipped before the sweep proved the second equivalent."""
        class _Falsy:
            def __bool__(self_):
                return False

            def __call__(self_, ev):
                return True

        c, _seen = _component(events=("focus",), on_event=_Falsy())
        self.assertEqual(events.wanted(c), ("focus",))


class TheWithdrawalUndoesExactlyWhatTheRequestAsked(unittest.TestCase):
    """The two mode constants are a PAIR, and the cases that watch them being written can
    only ever compare a constant to itself.

    `FOCUS_ON`'s value is pinned for real by `tests/test_frame_input_reaches_a_component.py`
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
        """`overlay.MOUSE_ON` is the same shape for the pointer, and this module now writes
        that one too — off a DIFFERENT declaration. Which is what makes keeping the two
        constants distinct matter more than it did when only one was ever written: a
        copy-paste that armed mouse reporting on a panel that wanted to know whether it was
        focused would take the operator's text selection without `[frame] mouse` ever being
        set, and the case below is what stops it."""
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


class TheDecoderReadsTheButtonNumberAsTheBitfieldItIs(unittest.TestCase):
    """An SGR button number carries three separate things and each one is a defect if it
    is read as part of the number instead of as itself.

    The wheel's direction was already read by its low bit (#605's `button & 1`). These are
    the other two, and both are reachable: right- and middle-clicks were MEASURED arriving
    at a non-active pane on tmux 3.7c and 3.2 (`b'\\x1b[<2;20;5M'`, `b'\\x1b[<1;20;5M'`).
    """

    def test_each_button_is_named(self):
        for button, name in ((0, "left"), (1, "middle"), (2, "right")):
            with self.subTest(button=button):
                evs, _t = overlay.decode(b"\x1b[<%d;20;5M" % button)
                self.assertEqual([(e.kind, e.name) for e in evs],
                                 [(overlay.CLICK, name)])

    def test_a_modifier_does_not_become_a_different_button(self):
        """Shift is bit 2, so a shift+click is `4` — and it is a LEFT click, which is what
        the operator who pressed it meant. `_CSI`'s rule for a modified arrow, one
        protocol over: the low bits name the gesture, the high ones name the keyboard."""
        for button in (4, 8, 16, 4 | 8 | 16):
            with self.subTest(button=button):
                evs, _t = overlay.decode(b"\x1b[<%d;20;5M" % button)
                self.assertEqual([(e.kind, e.name) for e in evs],
                                 [(overlay.CLICK, "left")])

    def test_motion_is_not_a_click(self):
        """Bit 5 says the pointer MOVED. §4f closed the kinds without `drag`, so a report
        carrying it is dropped — read as a button it is `32 & 3 == 0`, a LEFT click, at
        every cell a drag crosses.

        tmux was measured filtering these out on 3.7c and 3.2 with its own `mouse` both
        off and on. This asks that charter's contract does not DEPEND on that filtering:
        the bit is in the protocol, and what a component is handed is decided by what the
        bit says rather than by another program's tidiness."""
        for button in (32, 33, 32 | 4):
            with self.subTest(button=button):
                self.assertEqual(overlay.decode(b"\x1b[<%d;20;5M" % button), ([], b""))

    def test_a_button_the_encoding_cannot_name_is_dropped_not_invented(self):
        """`3` is the X10 encoding's "no button". In SGR a release names the button it
        released and the trailing `m` is what makes it one, so a `3` is a terminal
        speaking the wrong encoding — dropped rather than given a fourth name."""
        self.assertEqual(overlay.decode(b"\x1b[<3;20;5M"), ([], b""))

    def test_the_thumb_buttons_are_not_a_left_click(self):
        """**Bit 7 is the third place xterm keeps the button number**, and reading only
        the low two bits makes button 8 a `left` click — which a component that acts on
        `left` then acts on.

        Reachable, not theoretical: measured on tmux 3.7c and 3.2, `\\x1b[<128;20;5M`
        injected at a non-active pane that asked for 1000+1006 was forwarded to it
        VERBATIM, along with 129, 130 and 131. §4f named no kind for these, so a button
        charter cannot name is one charter does not report."""
        for button in (128, 129, 130, 131):
            with self.subTest(button=button):
                self.assertEqual(overlay.decode(b"\x1b[<%d;20;5M" % button), ([], b""))

    def test_the_wheel_with_the_motion_bit_is_not_a_scroll(self):
        """`96` is the wheel bit and the motion bit together. Testing bit 6 before bit 5
        reports it as a scroll nobody performed; answering motion FIRST is what makes the
        order impossible to get wrong."""
        for button in (96, 97):
            with self.subTest(button=button):
                self.assertEqual(overlay.decode(b"\x1b[<%d;20;5M" % button), ([], b""))

    def test_every_button_a_terminal_can_send_is_named_or_dropped(self):
        """The whole 8-bit space, against the encoding rather than against the
        implementation: motion first, then one number reassembled from its three bit
        positions, then the six names this contract has. Nothing is left to a branch order.

        A loop rather than eight cases because the defect this closes was in the SPACE
        between the values anyone thought to write down — 0, 1, 2 and 64 were all correct
        while 128 was a left click."""
        names = {0: ("click", "left"), 1: ("click", "middle"), 2: ("click", "right"),
                 4: ("scroll", "up"), 5: ("scroll", "down")}
        for button in range(256):
            number = ((button & 3) + (4 if button & 64 else 0)
                      + (8 if button & 128 else 0))
            want = [] if button & 32 else [names[number]] if number in names else []
            evs, tail = overlay.decode(b"\x1b[<%d;20;5M" % button)
            with self.subTest(button=button):
                self.assertEqual([(e.kind, e.name) for e in evs], want)
                self.assertEqual(tail, b"")

    def test_press_and_release_are_told_apart_by_the_final_byte(self):
        press, _t = overlay.decode(b"\x1b[<0;20;5M")
        release, _t = overlay.decode(b"\x1b[<0;20;5m")
        self.assertTrue(press[0].pressed)
        self.assertFalse(release[0].pressed)

    def test_the_coordinates_are_zero_based_and_nothing_else_is_subtracted(self):
        """tmux has already taken `pane_left` and any `pane-border-status` row off —
        measured, and `overlay.Event` carries the readings. All this does is 1-based to
        0-based, and a decoder that "helpfully" did more would double tmux's own work."""
        evs, _t = overlay.decode(b"\x1b[<0;20;5M")
        self.assertEqual((evs[0].col, evs[0].row), (19, 4))


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

    def test_the_pointer_opens_it_and_asks_for_mouse_reporting_and_not_focus(self):
        """A component that declared only `click` is never told about focus.

        The half that costs something if it is wrong is the OTHER direction — see
        `test_focus_alone_never_arms_the_pointer` — but both are the same rule and neither
        is free: a pane asking for reports nobody declared fills its own input buffer with
        events that are decoded, dropped by `_deliver`, and paid for by the panel."""
        c, _seen = _component(events=("click",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.assertTrue(d.reading)
        self.assertEqual(self.tty.sent(), overlay.MOUSE_ON.encode())

    def test_focus_alone_never_arms_the_pointer(self):
        """The expensive direction, and the reason `open` asks per declaration rather than
        once for the pair. `instance.FRAME_FIELDS` records the trade `MOUSE_ON` makes: the
        instant a mouse-requesting pane is active, the operator's terminal stops doing its
        own drag-select. A panel that only wanted to know whether it was focused must not
        spend that on their behalf."""
        c, _seen = _component(events=("focus", "blur"))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        sent = self.tty.sent()
        self.assertEqual(sent, events.FOCUS_ON.encode())
        self.assertNotIn(b"1000", sent)
        self.assertNotIn(b"1006", sent)

    def test_declaring_both_asks_for_both_and_withdraws_both(self):
        c, _seen = _component(events=("focus", "scroll"))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.assertEqual(self.tty.sent(),
                         (events.FOCUS_ON + overlay.MOUSE_ON).encode())
        d.close()
        self.assertEqual(self.tty.sent(),
                         (overlay.MOUSE_OFF + events.FOCUS_OFF).encode())

    def test_the_withdrawal_is_exactly_what_was_asked_and_no_more(self):
        """A `close` that wrote both withdrawals unconditionally would tell a terminal to
        stop reporting a mouse this pane never asked about. Harmless on its own, and wrong
        in the way that matters here: charter writing a mode change into a pane on behalf
        of a component that declared nothing of the kind."""
        for declared, expected in (("focus", events.FOCUS_OFF),
                                   ("click", overlay.MOUSE_OFF)):
            with self.subTest(declared=declared):
                tty = _Pty(self)
                c, _seen = _component(events=(declared,))
                d = events.Dispatcher(c, stream=tty.stream)
                d.open()
                tty.sent()
                d.close()
                self.assertEqual(tty.sent(), expected.encode())

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

    def test_closing_a_path_that_was_never_opened_writes_nothing_to_the_pane(self):
        """`close` guards on `_fd`, and without that guard a dispatcher that never opened
        an input path would still write `\x1b[?1004l` into a pane that never asked for
        reporting — a resize-only component's, or one whose pane is not a terminal. The
        mode restore is harmless there; the write is not, because it is charter putting an
        escape sequence into a rectangle a provider is drawing in."""
        c, _seen = _component(events=("resize",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.assertFalse(d.reading)
        d.close()
        self.assertEqual(self.tty.sent(), b"",
                         "close() spoke to a pane it had never opened")

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

    def test_a_stream_whose_fileno_is_not_a_descriptor_is_left_alone(self):
        """`termios.tcgetattr(None)` raises `TypeError`, not `OSError` — measured — so a
        guard set widened only for `termios.error` still sent this to `panel._hold`,
        painting a refusal for a component that had done nothing wrong. A `MagicMock`
        stdout, which `mock.patch("sys.stdout")` installs, reaches this exact path."""
        class _Vague:
            def fileno(self_):
                return None

            def write(self_, _s):
                pass

            def flush(self_):
                pass

        with self.assertRaises(TypeError):
            termios.tcgetattr(None)          # the measurement this case exists for
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=_Vague())
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


class TheRestoreIsTheThingCloseCannotAffordToLose(unittest.TestCase):
    """`close` runs in two `finally`s and the mode it puts back is the operator's.

    These are the cases an adversarial read of the first version produced. Each one is a
    way the restore could be skipped or lost, and each was reachable.
    """

    def setUp(self):
        self.tty = _Pty(self)

    def _open(self):
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.sent()
        return d

    def test_a_withdrawal_that_raises_does_not_cost_the_mode(self):
        """The withdrawal writes to the pane and the restore does not, so the write is the
        half that can fail — and it used to run FIRST, with `_fd` already cleared, so a
        raise there lost the `tcsetattr` permanently: the retry from `_watch`'s `finally`
        found `_fd` `None` and returned."""
        before = _mode(self.tty.slave)
        d = self._open()
        self.assertNotEqual(_lflags(self.tty.slave), before[3])

        class _Refuses:
            def fileno(self_):
                return self.tty.slave

            def write(self_, _s):
                raise OSError("the pane went away")

            def flush(self_):
                pass

        d._stream = _Refuses()
        d.close()
        self.assertEqual(_mode(self.tty.slave), before,
                         "a failing withdrawal took the mode restore with it")

    def test_the_mode_is_back_before_the_withdrawal_is_even_attempted(self):
        """The case the ordering actually exists for, and the one a raising stream cannot
        show: a withdrawal that BLOCKS.

        `open` clears `ICANON`/`ECHO` and deliberately leaves the input flags alone, so
        `IXON` is still on — an operator who selects this pane and presses Ctrl-S puts the
        pty in XOFF and the flush inside `_write` waits for an XON that may never come.
        With the withdrawal ahead of the restore, `close` parks there in a `finally` with
        the operator's pane still echoing nothing and no process left to fix it.

        Run on a thread with a deadline, so a regression is red rather than a hang.
        """
        import threading

        before = _mode(self.tty.slave)
        d = self._open()
        released = threading.Event()

        class _Blocks:
            def fileno(self_):
                return self.tty.slave

            def write(self_, _s):
                released.wait(10.0)

            def flush(self_):
                pass

        d._stream = _Blocks()
        t = threading.Thread(target=d.close, daemon=True)
        t.start()
        try:
            deadline = time.time() + 5.0
            while time.time() < deadline and _mode(self.tty.slave) != before:
                time.sleep(0.02)
            self.assertEqual(
                _mode(self.tty.slave), before,
                "close() parked on the withdrawal with the operator's mode still changed")
        finally:
            released.set()
            t.join(10.0)

    def test_a_second_close_after_a_failed_one_is_not_a_silent_no_op(self):
        """Whatever `close` gives up on, the mode is back before it can."""
        before = _mode(self.tty.slave)
        d = self._open()
        d.close()
        d.close()
        self.assertEqual(_mode(self.tty.slave), before)

    def test_opening_twice_does_not_capture_cbreak_as_the_mode_to_restore(self):
        """A second `open` used to read the mode the FIRST one installed and keep it as
        `_before`, so `close` would "restore" the pane to ECHO off, for good."""
        before = _mode(self.tty.slave)
        d = self._open()
        d.open()
        d.close()
        self.assertEqual(_mode(self.tty.slave), before)


class AnEventReachesTheComponentThatOwnsThePane(unittest.TestCase):
    def setUp(self):
        self.tty = _Pty(self)
        # A pane charter CAN measure, which is what a panel in a tmux window has and what
        # this process does not: these cases hand the dispatcher an explicit `stream`,
        # while `pane.size()` measures the descriptor `pane.claim()` took — `sys.stdout`
        # here, with no tty behind it under a test runner. `_on_canvas` refuses to
        # translate a pointer against a rectangle charter could not measure, so without
        # this every pointer case below would pass for the wrong reason: nothing delivered,
        # because nothing was measurable. `AResizeIsTheRectangleMovingAndNotTheSignal`
        # mocks the same call for the same reason.
        patcher = mock.patch("charter.frame.pane.size",
                             return_value=os.terminal_size((40, 8)))
        patcher.start()
        self.addCleanup(patcher.stop)

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

    def test_a_click_on_the_pane_reaches_the_handler(self):
        """The other half of #607, and what this change is: bytes tmux routed by POSITION,
        an `Event` in the component that owns the rectangle they landed in."""
        c, seen = _component(events=("click",))
        d = self._open(c)
        self.tty.deliver(b"\x1b[<0;21;5M\x1b[<0;21;5m")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([(e.kind, e.name, e.row, e.col, e.pressed) for e in seen],
                         [(overlay.CLICK, "left", 4, 20, True),
                          (overlay.CLICK, "left", 4, 20, False)])

    def test_a_wheel_over_the_pane_reaches_it_as_a_scroll(self):
        c, seen = _component(events=("scroll",))
        d = self._open(c)
        self.tty.deliver(b"\x1b[<64;21;5M\x1b[<65;21;5M")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([(e.kind, e.name) for e in seen],
                         [(overlay.SCROLL, "up"), (overlay.SCROLL, "down")])

    def test_a_click_never_reaches_a_component_that_asked_only_to_scroll(self):
        """Two kinds, one request: `MOUSE_ON` arms both, so the declared-kind filter is the
        only thing between a `scroll` component and every click on its pane."""
        c, seen = _component(events=("scroll",))
        d = self._open(c)
        self.tty.deliver(b"\x1b[<0;21;5M\x1b[<64;21;5M")
        self.assertTrue(d.poll(1.0))
        self.assertEqual([e.kind for e in seen], [overlay.SCROLL])

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

    def test_nothing_available_right_now_is_not_end_of_input(self):
        """`EAGAIN` and `EINTR` mean "ask again"; the branch beside them retires the
        component's events for the life of the panel. Folding them together means one
        `O_NONBLOCK` race — which anything putting stdout on an asyncio loop creates — or
        one `SIGWINCH` landing in the read costs a component every later event, silently,
        with no `failure` to paint."""
        for err in (BlockingIOError(11, "EAGAIN"), InterruptedError(4, "EINTR")):
            with self.subTest(err=type(err).__name__):
                c, _seen = _component(events=("focus",))
                d = events.Dispatcher(c, stream=self.tty.stream)
                d.open()
                self.addCleanup(d.close)
                self.tty.deliver(b"\x1b[I")
                with mock.patch("charter.frame.events.os.read", side_effect=err):
                    self.assertFalse(d.poll(1.0))
                self.assertTrue(d.reading, "a retryable read retired the input path")
                self.assertIsNone(d.failure)

    def test_a_descriptor_select_can_no_longer_ask_about_closes_the_path(self):
        """`select` raises for a descriptor that has gone. Letting it escape would take the
        exception out of `poll`, out of `_watch` and into `_run`'s catch, which paints
        `panel stopped` and holds the pane — a refusal, for a pane that merely closed."""
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        with mock.patch("charter.frame.events.select.select",
                        side_effect=OSError(9, "Bad file descriptor")):
            self.assertFalse(d.poll(0.1))
        self.assertFalse(d.reading)

    def test_the_other_spelling_of_end_of_input_is_the_same_answer(self):
        """**"The other end is gone" has two spellings and this platform is not the one
        that decides which.** `palette._reader`'s docstring records the measurement that
        cost a red CI: closing a pty's far end answers `b""` on macOS and raises
        `OSError: [Errno 5]` on Linux. The case below this one reaches the `b""` arm on a
        macOS machine and would never reach the other, so a suite green twice over here
        would still have been red on every Linux operator's machine.

        Two survivors sat in `poll` and masked each other; this is the second of them.
        """
        c, _seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.deliver(b"\x1b[I")
        with mock.patch("charter.frame.events.os.read",
                        side_effect=OSError(5, "Input/output error")):
            self.assertFalse(d.poll(1.0))
        self.assertFalse(d.reading, "Linux's spelling of EOF left the path open")
        self.assertIsNone(d.failure, "a closed pane is not a component's failure")

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


class APointerEventArrivesInTheComponentsOwnColumns(unittest.TestCase):
    """Two subtractions stand between a terminal's column and a component's, and getting
    which one is charter's wrong is a defect in either direction.

    **tmux's is already done.** `pane_left` and any `pane-border-status` row are gone
    before the bytes reach this process — measured on 3.7c and at the 3.2 floor, both
    identical: a click at window column 100 with the pane at `left=80` arrived as
    `b'\\x1b[<0;20;5M'`, and with `pane-border-status top` set a click on window row 5
    arrived as row 4. Charter must not redo it.

    **Charter's is not.** `[frame] pad` is drawn by `slots.inset_rows` after the component
    has composed, and `slots.content_width` is the narrower canvas it was told it had.
    tmux knows nothing about either, so a dispatcher that passed the pane's column through
    would tell a component with `pad = 3` that a click on its first cell landed on its
    fourth.
    """

    def setUp(self):
        self.tty = _Pty(self)

    def _clicked(self, *, pad: int, cols: int, col: int, kind=overlay.CLICK):
        """EVERY event the handler was handed for a click at pane column *col*.

        The whole list, not ``seen[0] if seen else None``, and the sweep is why. That
        spelling could not tell a dropped event from a handler CALLED WITH ``None`` — the
        recording handler appends whatever it is given — so deleting `_deliver`'s
        ``if ev is None: return False`` left every margin case below still green while a
        margin click reached a stranger's code as `None`. A component doing `ev.kind` on
        it raises, is retired, and loses its pane. That survivor is what this signature
        change closes.
        """
        c, seen = _component(events=(kind,))
        d = events.Dispatcher(c, stream=self.tty.stream)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((cols, 8))), \
                mock.patch("charter.frame.slots.pad_for", return_value=pad):
            d.open()
            self.addCleanup(d.close)
            self.tty.sent()
            self.tty.deliver(b"\x1b[<0;%d;5M" % (col + 1))
            d.poll(1.0)
        return seen

    def test_the_pad_is_taken_off_the_column(self):
        """A pane 40 wide with `pad = 3` gives the component columns 0..33 at pane columns
        3..36. A click on its first cell is pane column 3, and the component must be told
        `0` — the cell it actually drew there."""
        seen = self._clicked(pad=3, cols=40, col=3)
        self.assertEqual([e.col for e in seen], [0])

    def test_the_last_cell_of_the_canvas_is_the_last_cell_of_the_canvas(self):
        """`content_width` is `cols - 2 * pad` = 34, so the component's last column is 33
        and it sits at pane column 36. One off at this end is the end that silently drops
        a real click rather than mis-reporting one."""
        seen = self._clicked(pad=3, cols=40, col=36)
        self.assertEqual([e.col for e in seen], [33])

    def test_a_click_in_the_left_margin_reaches_the_handler_not_at_all(self):
        """Those cells are charter's; the component never drew in them. A clamp would
        report a click on a cell the operator can SEE is empty as a click on the first cell
        of a row they can see is not — `EVENT_KINDS`'s "fires wrongly" at the one place the
        distinction can still be made.

        `assertEqual(seen, [])` and never `assertIsNone(seen[0] …)`: the handler must not
        be CALLED, and the weaker spelling could not tell that from being called with
        `None`. See :meth:`_clicked`."""
        for col in (0, 1, 2):
            with self.subTest(col=col):
                self.assertEqual(self._clicked(pad=3, cols=40, col=col), [])

    def test_a_click_in_the_right_margin_reaches_it_not_at_all_either(self):
        for col in (37, 38, 39):
            with self.subTest(col=col):
                self.assertEqual(self._clicked(pad=3, cols=40, col=col), [])

    def test_an_unpadded_pane_hands_the_column_straight_through(self):
        """The shipped default is `pad = 0`, and there the test is the pane's own bounds —
        which tmux has already guaranteed. A panel that asked for no pad pays nothing and
        sees exactly what tmux reported."""
        for col in (0, 17, 39):
            with self.subTest(col=col):
                self.assertEqual([e.col for e in self._clicked(pad=0, cols=40, col=col)],
                                 [col])

    def test_the_row_is_not_touched_at_all(self):
        """Nothing insets a row: `inset_rows` pads the LEFT of each one and `panel._write`
        homes the cursor before the first, so a component's row 0 is its pane's row 0. This
        goes red the day anything grows a top inset without teaching this module about it —
        which is the whole reason to assert a number that is currently a no-op."""
        self.assertEqual([e.row for e in self._clicked(pad=3, cols=40, col=10)], [4],
                         "the row was translated by something")

    def test_a_pane_that_could_not_be_measured_fires_nothing(self):
        """`slots._width()` answers its stated 80-column fallback for a pane it could not
        measure, and `pane.size()` answers `None` — the difference is the whole guard.

        Translating against the fallback would deliver a click in cells of an invented
        canvas, and would do it at the one moment the pane is showing `panel._unmeasured`'s
        message instead of the component's rows: the component would be told where it was
        clicked while what is on screen is not the component. `note_resize` refuses the
        same reading for the same reason — "a rectangle charter could not measure is not
        one end of a comparison"."""
        c, seen = _component(events=("click",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.sent()
        with mock.patch("charter.frame.pane.size", return_value=None):
            self.tty.deliver(b"\x1b[<0;5;5M")
            d.poll(1.0)
        self.assertEqual(seen, [])

    def test_a_scroll_is_translated_the_same_way(self):
        """Both pointer kinds carry a position, so both are translated. A `scroll` left in
        pane columns would be the same defect surviving in the kind nobody checked."""
        c, seen = _component(events=("scroll",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))), \
                mock.patch("charter.frame.slots.pad_for", return_value=3):
            d.open()
            self.addCleanup(d.close)
            self.tty.sent()
            self.tty.deliver(b"\x1b[<64;4;5M")     # pane column 3, the first cell
            d.poll(1.0)
        self.assertEqual([(e.kind, e.col) for e in seen], [(overlay.SCROLL, 0)])

    def test_a_focus_event_is_not_given_a_position_it_never_had(self):
        """`focus`, `blur` and `resize` carry no coordinates, so nothing is subtracted from
        them. Translating one would move a `0` that means "no position" to `-3` and then
        drop the event for landing in a margin it was never in."""
        c, seen = _component(events=("focus",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))), \
                mock.patch("charter.frame.slots.pad_for", return_value=3):
            d.open()
            self.addCleanup(d.close)
            self.tty.sent()
            self.tty.deliver(b"\x1b[I")
            self.assertTrue(d.poll(1.0), "a focus event was dropped as out of bounds")
        self.assertEqual([(e.kind, e.col) for e in seen], [(overlay.FOCUS, 0)])

    def test_the_pad_taken_off_is_the_pad_that_pane_was_drawn_with(self):
        """The cross-module agreement, and the one this whole class is worthless without:
        `panel._component_text` insets by `slots.inset_rows(rows, cid)` and sizes by
        `slots.content_width(cid)`, so the dispatcher has to ask under the SAME name or it
        subtracts one component's pad from another component's click.

        Asked by giving two names different pads and checking which one moved the column —
        a case that a dispatcher asking under a slot name, a title, or a hard-coded default
        would fail while every other case here still passed.
        """
        from charter import instance

        def styled(_frame, name):
            return {"bg": None, "pad": 3 if name == "acme.metrics" else 0}

        c, seen = _component(events=("click",))
        self.assertEqual(c.id, "acme.metrics")
        d = events.Dispatcher(c, stream=self.tty.stream)
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))), \
                mock.patch.object(instance, "component_style", side_effect=styled):
            d.open()
            self.addCleanup(d.close)
            self.tty.sent()
            self.tty.deliver(b"\x1b[<0;4;5M")          # pane column 3
            d.poll(1.0)
        self.assertEqual([e.col for e in seen], [0],
                         "the click was translated by some other name's pad")

    def test_the_dispatcher_measures_the_pane_once_for_one_event(self):
        """The pad taken off and the width tested against come from ONE reading. Asking
        twice would let a `SIGWINCH` land in between — translating a click by a pad
        afforded against one width and testing it against another — and `panel._watch`
        repaints on exactly that signal, so the window is real rather than theoretical.

        Asked of the dispatcher rather than of `content_rect`, because `content_rect` now
        takes the width as an argument and it is the CALLER that has to take only one
        reading."""
        c, _seen = _component(events=("click",))
        d = events.Dispatcher(c, stream=self.tty.stream)
        d.open()
        self.addCleanup(d.close)
        self.tty.sent()
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((40, 8))) as size:
            self.tty.deliver(b"\x1b[<0;5;5M")
            d.poll(1.0)
        self.assertEqual(size.call_count, 1)


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

    def test_a_reading_charter_could_not_take_does_not_swallow_the_resize(self):
        """A pty answers "no size" between a resize and the `TIOCSWINSZ` that follows —
        `frame/pane.py` calls that ordinary. Storing that `None` spends the comparison: the
        next tick sees `was is None` and the resize that really happened never fires."""
        c, seen = _component(events=("resize",))
        d = self._dispatcher(c, os.terminal_size((40, 8)))
        with mock.patch("charter.frame.pane.size", return_value=None):
            d.note_resize()
        self.assertEqual(seen, [])
        with mock.patch("charter.frame.pane.size",
                        return_value=os.terminal_size((120, 8))):
            d.note_resize()
        self.assertEqual([e.kind for e in seen], [overlay.RESIZE])

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

        with mock.patch("charter.frame.slots.content_width", return_value=78), \
             mock.patch("charter.frame.slots.inset_rows", side_effect=lambda t, n: t), \
             mock.patch("charter.frame.panel._rows", return_value=6):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertIn("acme.metrics", got)
        self.assertIn("KeyError", got)

    def test_the_reason_is_wrapped_across_the_pane_and_never_clipped_to_one_line(self):
        """Every other provider failure reaches a pane through `_wrap`, and `_wrap`'s own
        docstring says why: "the tail of a refusal is the half that says what to do about
        it". This message names the component first and what broke last, and
        " charter: acme.metrics stopped taking events - " is 45 characters on its own — so
        a single `tui.truncate` in a 30-column side panel clips the exception ALWAYS."""
        class _Evs:
            failure = ("acme.metrics stopped taking events — "
                       "ZeroDivisionError: division by zero")

        with mock.patch("charter.frame.slots.content_width", return_value=30), \
             mock.patch("charter.frame.slots.inset_rows", side_effect=lambda t, n: t), \
             mock.patch("charter.frame.panel._rows", return_value=8):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertGreater(len(got.split("\n")), 1, "the reason was clipped to one line")
        self.assertIn("ZeroDivisionError", got)
        for line in got.split("\n"):
            self.assertLessEqual(tui.width(line), 30, line)

    def test_the_reason_is_contained_before_it_is_measured(self):
        """#472's order. The reason quotes a stranger's exception text, and an escape
        sequence in it is an instruction to the terminal rather than a character."""
        class _Evs:
            failure = "acme.metrics stopped taking events — KeyError: 'a\x1b[2Jb'"

        with mock.patch("charter.frame.slots.content_width", return_value=200), \
             mock.patch("charter.frame.slots.inset_rows", side_effect=lambda t, n: t), \
             mock.patch("charter.frame.panel._rows", return_value=6):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertNotIn("\x1b[2J", got)

    def test_containing_after_the_width_arithmetic_silently_loses_the_reason(self):
        """The sharp half of #472, and neither the escaping nor the width can show it —
        which is exactly why this containment survived the sweep with two cases standing
        over it.

        `_fit` escapes every line on the way out, so "is the escape gone" is answered by
        `_fit` whichever order the two run in. `_fit` also `tui.truncate`s every line to the
        width, so "is any row too wide" can never fail either. What the ORDER decides is
        where `_wrap` puts the line breaks, and getting that wrong DROPS TEXT. Measured, at
        width 32, on `'stopped: ' + '\x1b[2J' * 3 + ' ENDMARK'`::

            tui.width(raw) = 17      tui.width(contained) = 38
            contain first  -> ('stopped: \\x1b[2J\\x1b[2J\\x1b[2J', 'ENDMARK')
            contain second -> ('stopped: \\x1b[2J\\x1b[2J\\x1b[2J …',)

        Contain second and `_wrap` sizes the escapes at nothing, packs the tail onto the
        same row, and `_fit` then cuts what it cannot fit. `ENDMARK` is gone — and in a
        real failure that tail is the exception's own message, the only part that says what
        broke. `_wrap`'s own docstring makes the same argument about a word wider than the
        pane: "the tail of a refusal is the half that says what to do about it".
        """
        class _Evs:
            failure = "stopped: " + "\x1b[2J" * 3 + " ENDMARK"

        with mock.patch("charter.frame.slots.content_width", return_value=32), \
             mock.patch("charter.frame.slots.inset_rows", side_effect=lambda t, n: t), \
             mock.patch("charter.frame.panel._rows", return_value=12):
            got = panel._component_text(object(), "acme.metrics", "f-1", evs=_Evs())
        self.assertNotIn("\x1b[2J", got)
        self.assertIn("ENDMARK", got,
                      "the tail was measured unescaped, packed onto one row, and cut")

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

    def test_an_open_that_raises_leaks_neither_the_handler_nor_the_mode(self):
        """`open` ends by writing `\\x1b[?1004h`, and a write to a pane whose far end has
        gone raises. From outside `_watch`'s `try` that leaked the SIGWINCH handler and
        left the tty in the mode `open` had installed a line earlier — with the panel then
        going to `_hold`, which never returns, so nothing on the machine put either back.
        """
        import signal as _signal

        class _Boom(self._Evs):
            def open(self_):
                self_.log.append("open")
                raise OSError("the pane went away")

        evs = _Boom()
        before = _signal.getsignal(_signal.SIGWINCH)
        with mock.patch("charter.frame.state.version", return_value="v1"):
            with self.assertRaises(OSError):
                panel._watch("top", "f-1", once=True, evs=evs,
                             paint=lambda name, fid: None)
        self.assertEqual(evs.log, ["open", "close"])
        self.assertIs(_signal.getsignal(_signal.SIGWINCH), before)

    def test_a_close_that_raises_still_hands_the_sigwinch_handler_back(self):
        """`_watch` exists as its own function so the handler it arms is restored by its
        own `finally` (`RunOnceLoop.test_once_true_restores_the_previous_sigwinch_handler`).
        A flat `close(); signal.signal(...)` puts the fallible call FIRST and defeats
        exactly that — leaving a handler waking a loop with nothing left to repaint for the
        rest of a held process's life."""
        import signal as _signal

        class _Raises(self._Evs):
            def close(self_):
                self_.log.append("close")
                raise RuntimeError("the pane went away mid-restore")

        evs = _Raises()
        before = _signal.getsignal(_signal.SIGWINCH)
        with mock.patch("charter.frame.state.version", return_value="v1"):
            with self.assertRaises(RuntimeError):
                panel._watch("top", "f-1", once=True, evs=evs,
                             paint=lambda name, fid: None)
        self.assertIs(_signal.getsignal(_signal.SIGWINCH), before)

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
        c, _seen = _component(events=("key",))
        self.assertIsNone(panel._dispatcher(self._Reg(c), c.id))

    def test_a_component_declaring_focus_gets_one(self):
        c, _seen = _component(events=("focus",))
        self.assertIsInstance(panel._dispatcher(self._Reg(c), c.id),
                              events.Dispatcher)

    def test_a_component_declaring_only_the_pointer_gets_one(self):
        """The other half of the same promise, and the half this change adds: a component
        that declared `click` and nothing else now has an event path, where before it had
        none and its pane's terminal was left untouched."""
        c, _seen = _component(events=("click",))
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


class ACompositesPartCannotDeclareEventsNothingWouldDeliver(unittest.TestCase):
    """#607's own defect, one level down, refused where the message names a fixable thing.

    `panel._dispatcher` asks `wanted()` of the component PLACED on the frame, and a part is
    never placed — its parent draws it inside its own pane (`Registry.on_edge`). So a
    child's declaration would pass every check `Component` makes, build a handler, and
    receive nothing, ever: exactly the state this whole branch exists to end.
    """

    def _leaf(self, cid, **kw):
        base = dict(id=cid, title=cid, edge="right", size=component.Fixed(1),
                    render=lambda ctx: [cid])
        base.update(kw)
        return component.Component(**base)

    def test_a_part_that_declares_events_is_refused_and_names_both(self):
        from charter.frame import registry

        reg = registry.Registry()
        reg.register(self._leaf("acme.head", events=("focus",),
                                on_event=lambda ev: True))
        reg.register(self._leaf("acme.body", size=component.Fill()))
        with self.assertRaises(component.ComponentError) as e:
            reg.register(self._leaf("acme.side",
                                    children=("acme.head", "acme.body")))
        self.assertIn("acme.head", str(e.exception))
        self.assertIn("acme.side", str(e.exception))
        self.assertIn("focus", str(e.exception))

    def test_a_composite_may_declare_them_itself(self):
        """The refusal is about WHERE the declaration sits, not about composites. The
        parent owns the pane, so the parent is what charter can dispatch to."""
        from charter.frame import registry

        reg = registry.Registry()
        reg.register(self._leaf("acme.head"))
        reg.register(self._leaf("acme.body", size=component.Fill()))
        side = reg.register(self._leaf(
            "acme.side", children=("acme.head", "acme.body"),
            events=("focus",), on_event=lambda ev: True))
        self.assertEqual(events.wanted(side), ("focus",))

    def test_a_composite_of_ordinary_parts_is_untouched(self):
        """Charter's own sidebar is this shape, so the refusal must not reach it."""
        from charter.frame import registry

        reg = registry.Registry()
        reg.register(self._leaf("acme.head"))
        reg.register(self._leaf("acme.body", size=component.Fill()))
        side = reg.register(self._leaf("acme.side",
                                       children=("acme.head", "acme.body")))
        self.assertEqual(side.children, ("acme.head", "acme.body"))


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
