"""`tui.term_width` judges the ANSWER, not the source that produced it (#594).

The guard that mattered was already written, one rung up its own ladder: ``$COLUMNS=0``
is refused because a real environment in this project exports it and ``int("0")`` parses
happily. One rung down, the identical value walked through — ``os.get_terminal_size()``
answering zero columns produced ``max(1, 0) == 1``, and charter drew every table, row and
panel one column wide.

**Why these cases construct the condition instead of measuring the machine.** A test that
asks `term_width()` what this machine's terminal is answers whatever the runner happens to
have and would have passed against the defect on every developer box and in CI — the
defect only shows on a terminal whose size was never set. So both sources are stated here:
``$COLUMNS`` explicitly (`_envguard` removes the operator's own, and this suite states the
value it wants — #544), and `os.get_terminal_size` through the same `mock.patch` escape
`_ttyguard` documents.

**And the consequence is probed, not only the number.** ``1`` is a plausible-looking
integer; what it means is a report nobody can read. The readability probe renders a real
`tui` node at whatever `term_width` answered and asks whether the value is still there —
the same distinction #589 measured for a mis-sized column, where every alignment assertion
passed against the broken width because the value was silently *cut* rather than pushed.
"""

from __future__ import annotations

import contextlib
import os
import unittest
from unittest import mock

from charter import tui
from tests import _ttyguard

#: A width no machine running this would hand back by accident, so a case that passes
#: because the real terminal leaked in is a case that fails here instead (#588's shape:
#: a fixture agreeing with the machine's default tests nothing).
STATED = 137

#: What a terminal whose size was never negotiated reports. Not a hypothetical — see
#: :meth:`ThePremise.test_a_pty_with_no_window_size_reports_zero_columns`.
NO_SIZE = os.terminal_size((0, 0))


def _tty(cols: int):
    """`os.get_terminal_size` answering *cols*, via the escape `_ttyguard` documents."""
    return mock.patch("os.get_terminal_size",
                      return_value=os.terminal_size((cols, 24)))


@contextlib.contextmanager
def _columns(value: str | None):
    """``$COLUMNS`` stated outright — set to *value*, or ABSENT when it is None.

    Absent rather than empty: ``int("")`` raises too, so an empty string would exercise
    the same branch while claiming to test the other one.
    """
    with mock.patch.dict(os.environ, {} if value is None else {"COLUMNS": value}):
        if value is None:
            os.environ.pop("COLUMNS", None)     # `patch.dict` puts the whole dict back
        yield


class ThePremise(unittest.TestCase):
    """A tty reporting zero columns is ordinary, and this is where that is measured."""

    def test_a_pty_with_no_window_size_reports_zero_columns(self):
        """`os.openpty` sets no window size, so the slave reports 0x0 until somebody
        calls ``TIOCSWINSZ``. That is what tooling, some CI shells and a terminal attached
        before its size is negotiated all hand a process — the production case #594 is
        about, asserted against the real syscall rather than described in a comment.

        `_ttyguard.real_get_terminal_size` because the suite's own `os.get_terminal_size`
        refuses to read a terminal at all (#544); this asks the kernel about a pty this
        test made, which is not the operator's terminal by construction.
        """
        master, slave = os.openpty()
        try:
            self.assertEqual(_ttyguard.real_get_terminal_size(slave).columns, 0)
        finally:
            os.close(master)
            os.close(slave)


class TheAnswerIsWhatIsJudged(unittest.TestCase):
    """Zero is not a width, and which source said it is not part of the question."""

    def test_a_tty_reporting_zero_columns_falls_through_to_the_default(self):
        """The defect verbatim: this answered ``max(floor, 0)`` — the floor — because the
        zero-check was attached to ``$COLUMNS`` rather than to the width."""
        with _columns(None), _tty(0):
            self.assertEqual(tui.term_width(default=80, floor=24), 80)

    def test_a_tty_reporting_zero_columns_does_not_become_one_column(self):
        """At the DEFAULT floor of 1 — which is what `commands_frame` and every caller
        that does not state one gets — the old spelling returned literally 1."""
        with _columns(None), _tty(0):
            self.assertEqual(tui.term_width(), 80)

    def test_a_report_rendered_at_that_width_is_still_readable(self):
        """The consequence, not the integer. One column is a plausible number and an
        unreadable report; `tui.Text` truncates to fit, so the probe is whether the value
        survived rather than whether the line is the right length."""
        name = "charter-control-plane"
        with _columns(None), _tty(0):
            drawn = tui.Text(name).render(tui.term_width())
        self.assertEqual(drawn, [name],
                         "the width came back unusable and the whole line was truncated "
                         "into it — this is what `term_width() == 1` looks like on screen")

    def test_a_zero_in_the_environment_still_falls_through(self):
        """The rung that was already right stays right, and now falls through to a tty
        that CAN answer rather than to whatever came next."""
        with _columns("0"), _tty(STATED):
            self.assertEqual(tui.term_width(default=80, floor=24), STATED)

    def test_a_negative_in_the_environment_still_falls_through(self):
        with _columns("-5"), _tty(STATED):
            self.assertEqual(tui.term_width(default=80, floor=24), STATED)

    def test_unparseable_columns_still_falls_through(self):
        with _columns("wide"), _tty(STATED):
            self.assertEqual(tui.term_width(default=80, floor=24), STATED)

    def test_both_sources_unusable_lands_on_the_stated_default(self):
        """``COLUMNS=0`` and a tty answering zero are two unusable answers, not one
        unusable answer and one absent source — and the caller's *default* is what a
        function that could measure nothing is for."""
        with _columns("0"), _tty(0):
            self.assertEqual(tui.term_width(default=STATED, floor=24), STATED)

    def test_a_usable_environment_width_still_wins_over_the_tty(self):
        """The env-first order is the reason the function exists: a status line's stdout
        is a pipe, so ``$COLUMNS`` is the only thing that knows the rectangle."""
        with _columns(str(STATED)), _tty(40):
            self.assertEqual(tui.term_width(default=80, floor=24), STATED)

    def test_a_usable_tty_width_is_taken_when_the_environment_is_silent(self):
        with _columns(None), _tty(STATED):
            self.assertEqual(tui.term_width(default=80, floor=24), STATED)

    def test_the_floor_is_the_last_word_on_every_path(self):
        """Three paths reach a return and the clamp has to be on all of them, which is
        the other half of asking the question once."""
        with _columns("10"), _tty(0):
            self.assertEqual(tui.term_width(default=80, floor=24), 24)   # env
        with _columns(None), _tty(10):
            self.assertEqual(tui.term_width(default=80, floor=24), 24)   # tty
        with _columns(None), _tty(0):
            self.assertEqual(tui.term_width(default=10, floor=24), 24)   # default


class EachSourceAnswersOrDeclines(unittest.TestCase):
    """The two readers state a number or nothing, and neither has an opinion about
    whether the number is a width — that is what keeps the judgement in one place."""

    def test_the_environment_reader_declines_rather_than_guessing(self):
        with _columns(None):
            self.assertIsNone(tui._env_columns())
        with _columns("wide"):
            self.assertIsNone(tui._env_columns())

    def test_the_environment_reader_passes_an_unusable_number_on_unchanged(self):
        """It reports what the variable SAYS. A reader that filtered here would be the
        defect rebuilt one function lower down."""
        with _columns("0"):
            self.assertEqual(tui._env_columns(), 0)

    def test_the_tty_reader_declines_when_there_is_no_terminal(self):
        with mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
            self.assertIsNone(tui._tty_columns())

    def test_the_tty_reader_passes_an_unusable_number_on_unchanged(self):
        with _tty(0):
            self.assertEqual(tui._tty_columns(), 0)


if __name__ == "__main__":
    unittest.main()
