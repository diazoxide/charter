"""#606 — a panel's pane IS `sys.stdout`, so a library that rebinds that global takes the
pane away from charter without anything raising.

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
"""

import io
import os
import sys
import unittest
from contextlib import contextmanager, redirect_stderr, redirect_stdout
from unittest import mock

from charter.frame import chrome, pane, panel, slots, state

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


if __name__ == "__main__":
    unittest.main()
