"""#606/#611 — a pane-owning process's pane IS `sys.stdout`, so a library that rebinds that
global takes the pane away from charter without anything raising.

**The failure is the SECOND paint, and it is silent.** A test that patches `sys.stdout`
and asserts a width is not this defect: on `main` the first paint was correct, and only
once a provider's library had rebound the global did the panel start painting into that
library's in-memory log while measuring fd -1 and laying the frame out 80x24 in a 150x10
pane. Nothing raised, so `registry.Registry.draw`'s catch never fired and §4b's "a broken
component costs its own pane" was evaded rather than violated. So every assertion here is
about **what is on the pane**, never about an exception, and the shape reproduced is
paint → rebind → repaint rather than rebind → measure.

Measured on `main` before the fix, in a 150x10 pane::

    pane:            slots._width() = 150   slots._height() = 10
    after a rebind:  slots._width() = 80    slots._height() = 24   colour_ok() = True

`_PrintCapture` below is Textual's `redirect_stdout` stand-in as #605 measured it, but the
defect is not about Textual: `rich`, `click`, `tqdm`, `colorama`, a progress bar and a
logging handler installed at import all reach for the same global. What is pinned is the
property — *the pane this process was given* — and not the one library that took it.

**Two processes are handed a pane, and #611 is the second one.** `charter panel` is one;
`charter frame-palette --pane` is the other, and it had the same ordering the other way up
— `commands_frame._draw_palette` builds its action registry, which imports every installed
provider, and only then calls `palette.own_the_tty`, whose `out` resolved `sys.stdout` at
that later instant. So the classes below come in pairs: what the panel does at every
repaint, the palette does once, for the surface `F2` opens. The measurement is in
:class:`ThePalettesPaneIsClaimedAboveItsRegistryToo`.
"""

import io
import os
import pty
import sys
import threading
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from charter.frame import (builtin_actions, chrome, overlay, palette, pane, panel,
                           slots, state)

from tests._isolation import PersonaIso

#: The descriptor the pane below answers with, and the only one :func:`_measuring` will
#: measure. A real number rather than a sentinel: `os.get_terminal_size` takes an fd, and a
#: test that measured whatever it was handed could not tell the pane's descriptor from the
#: stand-in's.
_PANE_FD = 1

#: The rectangle #605 measured, kept as one constant so a test that asserts the pane's size
#: and a test that asserts the frame was laid out for it cannot drift apart.
_PANE_SIZE = os.terminal_size((150, 10))


class _Pane(io.StringIO):
    """A pane: a terminal, with a descriptor behind it that can be measured."""

    def isatty(self) -> bool:
        return True

    def fileno(self) -> int:
        return _PANE_FD


class _PrintCapture(io.StringIO):
    """What Textual's `redirect_stdout` installs, as #605 measured it: says it is a
    terminal, and hands back a descriptor that is not one.

    Both halves matter. `isatty() -> True` is what told `chrome.colour_ok` to keep emitting
    SGR, and `fileno() -> -1` is what made `os.get_terminal_size` raise and the measurement
    fall through to 80x24 — a stand-in answering `False` to the first would have been
    caught by the colour gate and a stand-in with no `fileno` at all by nothing.
    """

    def isatty(self) -> bool:
        return True

    def fileno(self) -> int:
        return -1


class _NotAPane(io.StringIO):
    """`charter panel bottom --session x > /tmp/log`: not a terminal, nothing to measure.

    The case `slots._DEFAULT_COLS`/`_DEFAULT_ROWS` have always existed for, and the reason
    "cannot measure" is not one state but two.
    """

    def isatty(self) -> bool:
        return False


def _probe(fid: str) -> str:
    """A renderer that draws the rectangle it was told it has.

    Asserting on `charter`'s own `bottom` output would pin the measurement only as far as
    truncation happens to make it visible; this makes the number charter believes the text
    on the pane.
    """
    return f"W={slots._width()} H={slots._height()}"


def _measuring(size=_PANE_SIZE, fd: int = _PANE_FD):
    """`os.get_terminal_size` answering for *fd* alone, raising for every other.

    The suite replaces the real one with a raise (`tests/_ttyguard`), so a test that wants
    a size states it. Stating it *per descriptor* is what makes these tests able to tell
    "measured the pane" from "measured whatever was asked": a blanket `return_value` would
    answer 150x10 for fd -1 too and go green on the bug.
    """
    def measure(fd_asked):
        if fd_asked != fd:
            raise OSError(f"[Errno 25] fd {fd_asked} is not a terminal")
        return size
    return mock.patch("os.get_terminal_size", side_effect=measure)


@contextmanager
def _claimed(stream):
    """Run the body with *stream* bound to `sys.stdout` AND claimed as this process's pane
    — what `panel.run` does at its first line, for a test that exercises something below
    `run` directly.
    """
    with redirect_stdout(stream):
        held = pane.claim()
        try:
            yield
        finally:
            pane.release(held)


def _paints(text: str) -> list[str]:
    """The paints in *text*, split the way four existing test call sites already split a
    pane's transcript: on the clear-screen that starts each one."""
    return text.split("\x1b[2J")[1:]


def _types_on_first_paint(stream_cls, master: int):
    """*stream_cls*, plus one Ctrl-C typed into *master* the first time it is written to.

    Ctrl-C rather than a named key: `overlay.decode` reads `\\x03` as Escape ("nothing else
    is going to turn this into a signal"), it is ONE byte so there is no sequence to split
    across reads, and it CANCELS — so nothing this class drives ever starts an action.

    A factory rather than two written-out subclasses because the arrangement has to be
    identical on both stand-ins; see `_ran_the_palette` for why both carry it.
    """
    class _Typing(stream_cls):
        def __init__(self) -> None:
            super().__init__()
            self._types = master

        def write(self, s):
            n = super().write(s)
            if self._types is not None:
                fd, self._types = self._types, None
                os.write(fd, b"\x03")
            return n

    return _Typing()


class _AttachedTo:
    """A `sys.stdin` whose descriptor is a real pty slave.

    `own_the_tty` asks `sys.stdin.fileno()` and hands the number to `termios.tcgetattr`,
    so this stands in for the palette pane's own tty. Deliberately nothing else: a rebound
    STDIN is #611's own stated residual — it RAISES rather than silently mispainting, and
    is not the failure this module is about.
    """

    def __init__(self, fd: int) -> None:
        self._fd = fd

    def fileno(self) -> int:
        return self._fd


class ARepaintReachesThePaneAfterALibraryTakesStdout(PersonaIso, unittest.TestCase):
    """The reported defect, end to end and in its real shape: a correct first paint, a
    rebind, and then a repaint.

    Red on `main` in both directions at once — the second paint landed in the capture and
    the pane kept the first frame forever, and the measurement that fed it dropped to
    80x24. Neither raised, so this asserts on the two transcripts and never on an
    exception. The `KeyboardInterrupt` is how the test STOPS a live panel (`_watch`'s
    `while True`), not what it measures — `run` deliberately does not catch one, because
    that is how a real panel is meant to end.
    """

    def test_the_second_paint_goes_to_the_pane_and_not_to_the_libraries_log(self):
        fid = "f-rebind"
        real, captured = _Pane(), _PrintCapture()
        ticks = []

        def sleep(_delay):
            ticks.append(1)
            if len(ticks) == 1:
                # A provider's library, mid-life: `textual.redirect_stdout`, a `rich`
                # console, a logging handler. It replaces the global and does not touch
                # the descriptor charter was given.
                sys.stdout = captured
                state.bump(fid)  # and something for the next tick to repaint for
                return
            raise KeyboardInterrupt

        with mock.patch.dict(slots.SLOTS, {"bottom": _probe}), _measuring(), \
             mock.patch("charter.frame.panel.time.sleep", side_effect=sleep):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                with self.assertRaises(KeyboardInterrupt):
                    panel.run("bottom", fid)

        painted = _paints(real.getvalue())
        self.assertEqual(len(painted), 2,
                         f"the pane did not get two paints: {real.getvalue()!r}")
        self.assertEqual(captured.getvalue(), "",
                         "a repaint went to the library's log instead of the pane")
        self.assertIn(f"W={_PANE_SIZE.columns} H={_PANE_SIZE.lines}", painted[1],
                      f"the repaint was laid out for a rectangle nobody has: {painted[1]!r}")

    def test_the_repaint_is_not_a_blank_pane(self):
        """The symptom as an operator sees it, stated separately from where the bytes
        went: on `main` the pane's second paint was a clear-screen and nothing else, which
        is what "blank on every repaint" means. Asserting only that the capture stayed
        empty would pass on a panel that had stopped painting altogether.
        """
        fid = "f-blank"
        real, captured = _Pane(), _PrintCapture()
        ticks = []

        def sleep(_delay):
            ticks.append(1)
            if len(ticks) == 1:
                sys.stdout = captured
                state.bump(fid)
                return
            raise KeyboardInterrupt

        with _measuring(), mock.patch("charter.frame.panel.time.sleep", side_effect=sleep):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                with self.assertRaises(KeyboardInterrupt):
                    panel.run("bottom", fid)

        self.assertTrue(_paints(real.getvalue())[1].strip(),
                        "the repaint cleared the pane and wrote nothing into it")


class ThePaneIsClaimedAboveAnythingAStrangerWrote(PersonaIso, unittest.TestCase):
    """The ordering, which is the whole of the fix and not a detail of it.

    `tests/_ttyguard` had to install ABOVE the import that pulls charter in, because
    `util._USE_COLOR` is `sys.stderr.isatty()` evaluated at that import (#545/#546). This
    is the same ordering one layer over: the first instant a stranger's code can run in a
    panel process is `registry.Registry.place`'s `importlib.import_module`, reached through
    `builtins.build()`, so the claim has to be above THAT.
    """

    def test_a_provider_rebinding_stdout_as_it_is_imported_does_not_move_the_pane(self):
        """`builtins.build()` stands in for the import it performs: a provider's module
        replacing `sys.stdout` at import time is exactly a logging handler or a `rich`
        console installed at module scope, and it happens before the panel has painted
        anything at all — so there is no correct first paint to hide behind here.

        Reached through a slot name no built-in claims, which is what sends `run` down the
        component path (`Registry.place`) rather than the four committed names.
        """
        real, captured = _Pane(), _PrintCapture()
        from charter.frame import builtins as _builtins
        real_build = _builtins.build

        def build_that_rebinds():
            sys.stdout = captured
            return real_build()

        with mock.patch.dict(slots.SLOTS, {"probe": _probe}), _measuring(), \
             mock.patch("charter.frame.builtins.build", side_effect=build_that_rebinds):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                panel.run("probe", "f-import", once=True)

        painted = _paints(real.getvalue())[0]
        self.assertEqual(captured.getvalue(), "",
                         "the first paint went to a stream the provider installed")
        # `Registry.place` answers a standin for an id nothing supplies, and the standin
        # names it — so this is the component pane's own content on the pane, not merely
        # bytes. §4b: a provider charter cannot load is a message, never a hole.
        self.assertIn("probe", painted,
                      f"the component's own pane never reached the pane: {painted!r}")

    def test_a_claim_does_not_outlive_the_panel_that_took_it(self):
        """`run(once=True)` is called in-process by tests, exactly as `_install_sigwinch`
        is — so a claim left set would make the NEXT caller in this process measure a
        rectangle that stopped existing. Pinned the way
        `RunOnceLoop.test_once_true_restores_the_previous_sigwinch_handler` pins the
        handler, and for the same reason.

        **The outer claim is taken and then let go of as `sys.stdout`**, and both halves
        of that are load-bearing. There must be a claim to put back, or a `run` that ended
        by clearing the global would answer `sys.stdout` and satisfy this by accident; and
        the outer claim must no longer BE `sys.stdout` when the assertion runs, or clearing
        the global would answer with the very object being compared against. Found by
        hand-mutation — `claim` returning `None` and `release` clearing instead of
        restoring both survived a version of this test that asserted from inside
        `redirect_stdout(outer)`.
        """
        outer = _NotAPane()
        with redirect_stdout(outer):
            held = pane.claim()
        try:
            with _measuring():
                with redirect_stdout(_Pane()), redirect_stderr(io.StringIO()):
                    panel.run("bottom", "f-claim", once=True)
            self.assertIsNot(pane.stream(), sys.stdout)
            self.assertIs(pane.stream(), outer)
        finally:
            pane.release(held)

    def test_a_process_that_claimed_nothing_answers_for_its_own_stdout(self):
        """The fallback is not a second answer to the same question: a hook, `charter
        statusline` and a test calling `slots._width()` on its own were never given a
        rectangle, and for them "this process's output" IS `sys.stdout`."""
        mine = _NotAPane()
        with redirect_stdout(mine):
            self.assertIs(pane.stream(), mine)


class TheMeasurementIsOfThePaneAndNotOfTheGlobal(PersonaIso, unittest.TestCase):
    """`slots._width`/`_height` directly, so a regression in the measurement cannot hide
    behind whatever a renderer happens to draw — the split `test_frame_slots.py::Width`
    already makes for `$COLUMNS`.
    """

    def test_width_and_height_follow_the_pane_through_a_rebind(self):
        with _measuring(), _claimed(_Pane()):
            sys.stdout = _PrintCapture()
            self.assertEqual(slots._width(), _PANE_SIZE.columns)
            self.assertEqual(slots._height(), _PANE_SIZE.lines)

    def test_both_halves_answer_for_the_same_descriptor(self):
        """A pane cannot be 150 columns wide and 24 rows tall. Two `os.get_terminal_size`
        calls with their own fallbacks could produce that — and did, once one of them was
        asked of a descriptor the other was not."""
        with _measuring(), _claimed(_Pane()):
            sys.stdout = _PrintCapture()
            self.assertEqual((slots._width(), slots._height()), tuple(_PANE_SIZE))


class AnUnmeasuredPaneIsNotAnEightyByTwentyFourPane(PersonaIso, unittest.TestCase):
    """The second half of #606, and #594's judgement one function over: what charter does
    when it genuinely cannot measure, and how that state is told apart from a successful
    measurement.
    """

    def test_a_tty_reporting_zero_columns_is_not_a_measurement(self):
        """#594's own case, which `tui.term_width` closed and this path still carried: a
        pty created without a window size reports zero until somebody calls `TIOCSWINSZ`
        (`os.openpty`, some CI shells, a terminal attached before its size is negotiated).
        Red before the fix at `slots._width() == 0`, which laid every table, row and panel
        out one column wide."""
        with _measuring(size=os.terminal_size((0, 0))), _claimed(_Pane()):
            self.assertIsNone(pane.size())
            self.assertEqual(slots._width(), slots._DEFAULT_COLS)
            self.assertEqual(slots._height(), slots._DEFAULT_ROWS)

    def test_either_half_being_zero_is_a_rectangle_nobody_has(self):
        """Both halves of the judgement, separately: a size is a rectangle, and a
        rectangle with no rows is no more a measurement than one with no columns. Asserted
        because `(0, 0)` alone is satisfied by half the check — either comparison on its
        own answers `None` for it, so the pair would look pinned while one of them was
        gone."""
        for reported in (os.terminal_size((150, 0)), os.terminal_size((0, 10))):
            with self.subTest(reported=tuple(reported)):
                with _measuring(size=reported), _claimed(_Pane()):
                    self.assertIsNone(pane.size())
                    self.assertEqual(slots._width(), slots._DEFAULT_COLS)
                    self.assertEqual(slots._height(), slots._DEFAULT_ROWS)

    def test_a_stand_in_with_no_descriptor_at_all_is_not_a_measurement(self):
        """A library's replacement need not be a file object at all, and one with no
        `fileno` raises `AttributeError` rather than the `OSError` a real descriptor with
        no tty behind it raises. Real — `test_frame_chrome`'s own `_Silent` is the same
        object one question over — and otherwise unreached here, which is exactly how
        `colour_ok`'s identical catch came to be narrowable to `ZeroDivisionError` with
        the whole suite still green until that test was written."""
        class _NoDescriptor:
            def isatty(self):
                return True

        with _measuring(), _claimed(_NoDescriptor()):
            self.assertIsNone(pane.size())
            self.assertEqual(slots._width(), slots._DEFAULT_COLS)

    def test_a_pane_that_cannot_be_measured_says_so_instead_of_being_drawn(self):
        """The distinguishable state, on the pane rather than in a return value. A frame
        laid out from `_DEFAULT_COLS` in a rectangle that is very probably not that shape
        is the destructive move `_measure_window` already refuses for a window (#501)."""
        real = _Pane()
        with mock.patch.dict(slots.SLOTS, {"bottom": _probe}), \
             mock.patch("os.get_terminal_size", side_effect=OSError("no size")):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                panel.run("bottom", "f-unmeasured", once=True)
        painted = _paints(real.getvalue())[0]
        self.assertIn("pane size unknown", painted)
        self.assertNotIn(f"W={slots._DEFAULT_COLS}", painted,
                         f"a guess was drawn as if it were a measurement: {painted!r}")

    def test_a_provider_component_is_not_handed_a_guessed_rectangle_either(self):
        """The other painter, and the reason it is not symmetry for its own sake:
        `ctx.build` hands a component a `width` and a `height` it may draw to the edge of,
        so a fallback that reached a component would be a guess a stranger's code had
        already been told was a measurement. Reached through a name no built-in claims,
        which is what sends `run` down the `Registry.place` path."""
        real = _Pane()
        with mock.patch.dict(slots.SLOTS, {"probe": _probe}), \
             mock.patch("os.get_terminal_size", side_effect=OSError("no size")):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                panel.run("probe", "f-component", once=True)
        painted = _paints(real.getvalue())[0]
        self.assertIn("pane size unknown", painted)
        self.assertNotIn("probe", painted,
                         f"the component drew into a rectangle nobody measured: {painted!r}")

    def test_a_pane_that_is_not_a_terminal_still_draws_at_the_stated_default(self):
        """`charter panel bottom --session x > /tmp/log`, run by hand for debugging, and
        every test in this suite. There is no rectangle for the frame to be wrong about,
        so the stated default is the right answer and always was — telling the two halves
        of "cannot measure" apart by the TTY is the property; "did the measurement raise"
        is the spelling, and it cannot separate them."""
        out = _NotAPane()
        with mock.patch.dict(slots.SLOTS, {"bottom": _probe}), \
             mock.patch("os.get_terminal_size", side_effect=OSError("no size")):
            with redirect_stdout(out), redirect_stderr(io.StringIO()):
                panel.run("bottom", "f-piped", once=True)
        painted = _paints(out.getvalue())[0]
        self.assertIn(f"W={slots._DEFAULT_COLS} H={slots._DEFAULT_ROWS}", painted)
        self.assertNotIn("pane size unknown", painted)

    def test_a_size_that_arrives_late_is_drawn_on_the_next_pass(self):
        """A pty gets its size from a `TIOCSWINSZ` that lands moments later, and the
        `SIGWINCH` after it is already a repaint reason. So the notice is painted per tick
        and never held: `_hold` would be permanent by construction, and would wedge the
        pane for the one failure that routinely fixes itself."""
        real = _Pane()
        with mock.patch.dict(slots.SLOTS, {"bottom": _probe}), _claimed(real):
            with mock.patch("os.get_terminal_size", side_effect=OSError("not yet")):
                panel._paint("bottom", "f-late")
            with _measuring():
                panel._paint("bottom", "f-late")
        first, second = _paints(real.getvalue())
        self.assertIn("pane size unknown", first)
        self.assertIn(f"W={_PANE_SIZE.columns} H={_PANE_SIZE.lines}", second)

    def test_a_refusal_is_still_readable_in_a_pane_nobody_can_measure(self):
        """`_hold` is the module's other paint and it deliberately does NOT take the
        notice: a panel that has already failed has something to say, and replacing its
        reason with "pane size unknown" would lose the only trace of why the pane is
        empty. It takes the stated fallback for its one truncation, which is
        `_window_size`'s half of the same split."""
        real = _Pane()
        with mock.patch("os.get_terminal_size", side_effect=OSError("no size")):
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                rc = panel.run("sideways", "f-refused", once=True)
        self.assertNotEqual(rc, 0)
        self.assertIn("sideways", _paints(real.getvalue())[0])


class ColourIsAskedOfThePane(PersonaIso, unittest.TestCase):
    """The third consequence of one rebind, and the quietest: `chrome.colour_ok` was told
    the pane is a terminal by a stand-in that is not one.
    """

    def test_a_stand_in_claiming_to_be_a_terminal_does_not_decide(self):
        with mock.patch.dict(os.environ, {}, clear=True), _claimed(_NotAPane()):
            sys.stdout = _PrintCapture()
            self.assertFalse(chrome.colour_ok(),
                             "SGR was written into a pane on a library's say-so")

    def test_the_panes_own_answer_is_what_decides(self):
        """The other direction, which a test of the first alone is satisfied by a
        `colour_ok` that answers `False` always."""
        with mock.patch.dict(os.environ, {}, clear=True), _claimed(_Pane()):
            sys.stdout = _NotAPane()
            self.assertTrue(chrome.colour_ok())


class ThePalettesPaneIsClaimedAboveItsRegistryToo(PersonaIso, unittest.TestCase):
    """#611 — the other pane-owning process, and the same ordering the other way up.

    `commands_frame._draw_palette` builds `builtin_actions.build` FIRST — which is
    `actions.ActionRegistry.add` → `registry.Providers` → `importlib.import_module` on
    every installed provider — and only then calls `palette.own_the_tty`, whose `out` was
    `sys.stdout` resolved at that later instant and whose `size` measured `out.fileno()`.

    **Measured on `main`, driving a whole real `charter frame-palette --pane` with a
    provider that rebinds `sys.stdout` as its module is imported**, into a `_Pane` that
    reports 150x10::

        pane transcript : ''
        library's log   : '\\x1b[?1049h\\x1b[?25l\\x1b[?1006h\\x1b[?1000h…'
        raised          : OSError('[Errno 25] fd -1 is not a terminal')

    Not one byte reached the pane — not even the alternate-screen enter, which is written
    before the first measurement — and the raise landed in a process whose `finally` had
    already run `_close_palette`, so what the operator sees is `F2` carving a pane off the
    harness, drawing nothing in it, and killing it again. `cmd_palette`'s documented
    "Always 0" goes with it. That is the palette's shape of #606's silent blank, so every
    assertion here is **on the pane's content** and none is on an exception.

    A REAL pty for stdin, because `own_the_tty` puts a tty in raw mode and `tcgetattr`
    refuses anything else — the same reason `test_frame_palette.TheTtyIsOwnedAndHandedBack`
    owns one.
    """

    FID = "f-palette-pane"

    def setUp(self) -> None:
        super().setUp()
        state.frame_dir(self.FID, create=True)
        state.record_server(self.FID, "charter")
        state.record_harness_pane(self.FID, "%3")
        state.record_identity(self.FID, {"CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        # Real tmux commands, and this test is not about them: the pane is closed by an
        # argv chain aimed at a server that is not running here.
        self.enterContext(mock.patch.object(commands_frame, "_close_palette"))
        self.master, self.slave = pty.openpty()
        self.addCleanup(self._close_pty)
        # Whatever claim was in force before this test, put back afterwards WHATEVER
        # happens below. `_ran_the_palette` deliberately survives a worker that never
        # returns (that is how a broken `out` is reported rather than hung on), and such a
        # worker never reaches `cmd_palette`'s own `finally` — so without this the next
        # test in the process reads a claim on a `_Pane` that stopped existing, and
        # `test_a_process_that_claimed_nothing_answers_for_its_own_stdout` fails for a
        # reason that has nothing to do with it. Asked through `claim`/`release` rather
        # than by reading the module's global, since a restore is exactly what they are.
        held = pane.claim()
        pane.release(held)
        self.addCleanup(pane.release, held)

    def _close_pty(self) -> None:
        """Close both ends — waking anything still blocked on the slave FIRST.

        `test_frame_palette.TheTtyIsOwnedAndHandedBack._close`'s reason, and it applies
        harder here because this class runs the surface on a WORKER: closing an fd does not
        wake a thread already inside `os.read` on it, and a daemon left there sits on an fd
        NUMBER that the next `pty.openpty` in this process may well be handed. One byte
        through the master ends the read.
        """
        try:
            os.write(self.master, b"\x03")
        except OSError:
            pass                       # already closed; nothing left to wake
        for fd in (self.master, self.slave):
            try:
                os.close(fd)
            except OSError:
                pass

    def _ran_the_palette(self, real, captured, deadline: float = 10.0):
        """One whole `charter frame-palette --pane`, with a provider taking `sys.stdout`
        as its module is imported, driven to an end and answered for.

        `builtin_actions.build` stands in for the import it performs, exactly as
        `ThePaneIsClaimedAboveAnythingAStrangerWrote` uses `builtins.build` for the panel's
        — a `rich` console or a logging handler at module scope IS this, and it runs before
        the palette has painted anything, so there is no correct first frame to hide behind
        here either.

        **The keystroke is typed by whichever stream the surface actually paints into**,
        and that is the one arrangement that neither hangs nor cheats. Typed BEFORE the
        call it is discarded: `tty.setraw` uses ``TCSAFLUSH``, which empties the input
        queue, so the surface then waits for a byte that is gone. Typed from the main
        thread in a loop it never arrives either — measured on this exact fixture, the
        worker sat inside `tcsetattr` for the whole deadline while the loop kept the
        slave's input queue full. So `_TypesOnFirstPaint` sends it from inside the first
        `write`, which `Surface.run` performs immediately after raw mode is entered and
        before its first `read` — and it is mixed into the LIBRARY's stand-in as well as
        the pane's, so a palette painting into the wrong one still ends, and this class
        reports a failed assertion about the pane rather than a hung suite.

        The worker is joined on a deadline for the residual case where NEITHER is written
        to: `assertFalse(is_alive())` is a red, and a bare call would have been a hang.
        """
        really_build = builtin_actions.build

        def build_that_rebinds(*a, **kw):
            sys.stdout = captured
            return really_build(*a, **kw)

        out: list = []

        def _work() -> None:
            try:
                out.append(commands_frame.cmd_palette(
                    SimpleNamespace(client="", pane=True)))
            except BaseException as e:              # noqa: BLE001 — reported, not raised
                out.append(e)

        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": self.FID}, clear=True), \
             mock.patch.object(builtin_actions, "build",
                               side_effect=build_that_rebinds), \
             mock.patch.object(sys, "stdin", _AttachedTo(self.slave)), _measuring():
            with redirect_stdout(real), redirect_stderr(io.StringIO()):
                worker = threading.Thread(target=_work, daemon=True,
                                          name="charter-test-palette")
                worker.start()
                worker.join(timeout=deadline)
        self.assertFalse(worker.is_alive(), "the palette never gave the pane back")
        return out[0] if out else None

    def _pane_and_capture(self):
        """The two streams this class tells apart, each able to end the surface — see
        `_ran_the_palette`."""
        return (_types_on_first_paint(_Pane, self.master),
                _types_on_first_paint(_PrintCapture, self.master))

    def test_the_palette_is_drawn_in_the_pane_and_not_in_the_librarys_log(self):
        real, captured = self._pane_and_capture()
        rc = self._ran_the_palette(real, captured)
        # The pane FIRST, and the return code last, deliberately. What the operator loses
        # is a surface, not a status: on `main` this pane was empty and the capture held a
        # whole palette, and a test that led with the exception would have been reporting
        # the symptom's shadow. `cmd_palette` documents "Always 0" and the raise took that
        # too, so it is asserted — after the thing it is a consequence of.
        painted = _paints(real.getvalue())
        self.assertTrue(painted, f"nothing was drawn in the pane: {real.getvalue()!r}")
        self.assertIn("detach — leave the harness running", painted[0],
                      f"the palette's own rows never reached the pane: {painted[0]!r}")
        self.assertEqual(captured.getvalue(), "",
                         "the palette was drawn into a stream the provider installed")
        self.assertEqual(rc, 0, f"the palette did not return quietly: {rc!r}")

    def test_the_palette_is_laid_out_for_the_rectangle_the_pane_reports(self):
        """The measurement, stated apart from where the bytes went — `render` answers
        exactly *height* lines, so the pane's own transcript carries the number the
        surface was laid out for. On `main` this raised instead: `os.get_terminal_size(-1)`
        on the stand-in's descriptor, with `own_the_tty` deliberately carrying no fallback
        to hide it."""
        real, captured = self._pane_and_capture()
        self._ran_the_palette(real, captured)
        painted = _paints(real.getvalue())
        self.assertTrue(painted, f"nothing was drawn in the pane: {real.getvalue()!r}")
        self.assertEqual(len(painted[0].split("\r\n")), _PANE_SIZE.lines,
                         "the palette was laid out for a rectangle nobody has")

    def test_a_stream_handed_in_is_still_the_one_written_to(self):
        """The claim is `out`'s DEFAULT, not a replacement for it — and that half had no
        test anywhere in the suite.

        Found by hand-mutation of the one line #611 changes: dropping the parameter
        entirely (`out = pane.stream()`, with no `if out is None`) left 338 tests green.
        Every caller that passes a stream today drives a real pty with `Surface.run`
        mocked (`test_frame_palette.TheTtyIsOwnedAndHandedBack`,
        `test_frame_pickers._RealPty`), so nothing was ever written to the stream they
        passed and nothing could notice it being ignored. A palette that painted into a
        claim instead of the stream it was handed would put a whole surface on the
        developer's own terminal in the middle of a test run.

        Both stand-ins type, so the mutation this pins is a failed assertion rather than a
        hang — see `_ran_the_palette`.
        """
        passed = _types_on_first_paint(_Pane, self.master)
        claimed = _types_on_first_paint(_Pane, self.master)
        surface = palette.Palette(catalogue=(overlay.Row(id="a.b", title="t"),))
        with _measuring(), _claimed(claimed):
            palette.own_the_tty(surface, fd=self.slave, out=passed)
        self.assertTrue(_paints(passed.getvalue()),
                        "the surface never painted into the stream it was handed")
        self.assertEqual(claimed.getvalue(), "",
                         "the surface ignored its `out` and painted into the claim")

    def test_a_claim_does_not_outlive_the_palette_that_took_it(self):
        """`cmd_palette` is a command in the one CLI that also runs `charter statusline`
        and every hook, so a claim left set is the next caller in this process measuring a
        rectangle that stopped existing — `panel.run`'s own rule (see
        `test_a_claim_does_not_outlive_the_panel_that_took_it`, which is this one function
        over) and `_install_sigwinch`'s before it.

        The outer claim is taken and then let go of as `sys.stdout` for that test's reason:
        a `release` that cleared the global instead of restoring would otherwise answer
        with the very object being compared against.
        """
        outer = _NotAPane()
        with redirect_stdout(outer):
            held = pane.claim()
        try:
            self._ran_the_palette(*self._pane_and_capture())
            self.assertIsNot(pane.stream(), sys.stdout)
            self.assertIs(pane.stream(), outer)
        finally:
            pane.release(held)

    def test_the_half_that_only_opens_the_palette_claims_no_pane(self):
        """The other branch of the same command, and the reason the claim is not simply
        hoisted above the `if`.

        Without ``--pane`` this process is the hotkey bind's `run-shell` child: it carves
        the overlay's pane off the harness with tmux commands and paints in nothing at all,
        so its stdout is a pipe. A claim there would record that pipe as "the pane this
        process was given", which is precisely the sentence `frame/pane.py`'s fallback
        exists to keep true — and `charter statusline`, every hook and every test calling
        `slots._width()` are the same case one command over.

        Asked through `stream()` rather than by reading the module's global: with a claim
        in force it answers the pipe this branch started in, and with none it follows
        `sys.stdout` wherever the process's own output went.
        """
        pipe, later = _NotAPane(), _NotAPane()
        seen: list = []

        def _open(_args) -> int:
            sys.stdout = later          # a library, mid-life, in a process with no pane
            seen.append(pane.stream())
            return 0

        with redirect_stdout(pipe), \
             mock.patch.object(commands_frame, "_open_palette", side_effect=_open):
            commands_frame.cmd_palette(SimpleNamespace(client="", pane=False))
        self.assertEqual(seen, [later],
                         "the half that is handed no rectangle claimed one anyway")


if __name__ == "__main__":
    unittest.main()
