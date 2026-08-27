"""The fourth tripwire: the terminal the suite was launched from is not a fixture either.

`test_plane_write_guard` and `test_plane_spawn_guard` cover the plane;
`test_no_test_reads_the_operators_shell` covers what that shell EXPORTS. This covers what
it IS — three file descriptors, and whether they are a terminal.

The defect it closes is not a wrong answer, it is a **hang**: `commands_frame._picker_wanted`
opens #518's workspace picker on ``sys.stdin.isatty() and sys.stdout.isatty()``, the
launcher module pinned only the second, and under a pty 122 of its tests sat at charter's
own prompt waiting for a human (#545). A hang is not a pass, not a fail, and not a report —
and CI, whose stdin is a pipe, cannot see it at all.

**Every case here is a control**, for the reason `test_no_test_reads_the_operators_shell`
gives: a guard nobody has watched fail is a guard nobody knows works. `TheGuardIsNotBlind`
makes the refusal happen for real, `EveryWayOutOfTheRefusalWorks` exercises each documented
escape — a tripwire with no usable exit is deleted by whoever hits it next — and
`WhatIsAnswered` pins the other half, which is silent by design.
"""

from __future__ import annotations

import io
import os
import sys
import unittest
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame
from tests import _ttyguard


class WhatIsAnswered(unittest.TestCase):
    """Move one: every stream answers what CI answers, on every machine."""

    def test_no_stream_reports_a_terminal(self):
        """`stdout` and `stderr` are answered and never refused — see the module docstring
        of `tests/_ttyguard.py` for why only `stdin` is loud."""
        self.assertFalse(sys.stdout.isatty())
        self.assertFalse(sys.stderr.isatty())

    def test_stdin_holds_nothing_so_a_prompt_ends_rather_than_blocks(self):
        """The half that matters when a path reaches a prompt anyway: `input()` reads
        `sys.stdin`, and `devnull` is at EOF. `commands_frame`'s picker treats `EOFError`
        as "no answer" and cancels, which is a verdict; blocking is not."""
        with self.assertRaises(EOFError):
            input()

    def test_charters_own_colour_decision_was_made_before_the_terminal_could_decide_it(self):
        """`util._USE_COLOR` is `sys.stderr.isatty()` evaluated at IMPORT of `charter.util`,
        which is why `_ttyguard.install()` runs above the import that pulls charter in.
        `test_mcp_approval` records what it cost when it did not: ANSI codes from the
        operator's terminal joined a transcript that test derives its expectations from, and
        a mutation that dies under a pipe "reported OK under a pty"."""
        from charter import util
        self.assertFalse(util._USE_COLOR)

    def test_what_the_terminal_actually_said_is_still_available(self):
        """`_envguard.scrubbed`'s counterpart: a case that genuinely has to know what the
        operator's terminal looked like has somewhere honest to ask, so the guard is never
        the thing standing in its way."""
        ambient = _ttyguard.ambient()
        self.assertEqual(sorted(ambient), ["stderr", "stdin", "stdout"])
        for value in ambient.values():
            self.assertIsInstance(value, bool)


class TheGuardIsNotBlind(unittest.TestCase):
    """Move two: an undeclared read of `sys.stdin.isatty()` is refused, for real."""

    def test_asking_whether_anybody_is_watching_gets_refused(self):
        with self.assertRaises(_ttyguard.AmbientTerminalRead):
            sys.stdin.isatty()

    def test_the_production_read_that_hung_the_suite_is_the_one_refused(self):
        """Not a synthetic call: `_picker_wanted` is the function that decides whether
        `charter claude` stops and asks, and it is where #545 lived."""
        args = SimpleNamespace(workspace=None, pick=False)
        with self.assertRaises(_ttyguard.AmbientTerminalRead):
            commands_frame._picker_wanted(args, None)

    def test_the_message_names_the_test_the_question_and_the_ways_out(self):
        with self.assertRaises(_ttyguard.AmbientTerminalRead) as e:
            sys.stdin.isatty()
        msg = str(e.exception)
        self.assertIn("test_the_message_names_the_test_the_question_and_the_ways_out", msg)
        self.assertIn("sys.stdin.isatty", msg)
        self.assertIn("_picker_wanted", msg)          # what the answer decides
        self.assertIn("return_value=False", msg)      # way out 1
        self.assertIn('mock.patch("sys.stdin"', msg)  # way out 2
        self.assertIn("no_terminal", msg)             # way out 3

    def test_it_is_a_base_exception_so_charters_own_fallbacks_cannot_eat_it(self):
        """The same reasoning `_planeguard.RealPlaneRead` carries: charter is full of
        `except Exception` fallbacks that would turn this tripwire into a degraded code
        path — `commands_frame` alone has several — and a tripwire something catches is a
        tripwire that reports a benign state instead of failing."""
        self.assertTrue(issubclass(_ttyguard.AmbientTerminalRead, BaseException))
        self.assertFalse(issubclass(_ttyguard.AmbientTerminalRead, Exception))

    def test_stdout_is_deliberately_not_refused(self):
        """The boundary, stated as a test rather than only as prose. Answering every
        stream is what makes the suite machine-independent; LOUDNESS is spent only on the
        question whose wrong answer is a process that never returns."""
        self.assertFalse(sys.stdout.isatty())


class EveryWayOutOfTheRefusalWorks(unittest.TestCase):
    def test_pinning_isatty_beside_the_stdout_pin(self):
        with mock.patch("sys.stdin.isatty", return_value=False):
            self.assertFalse(sys.stdin.isatty())

    def test_pinning_it_true_is_how_a_test_says_it_wants_the_prompt(self):
        args = SimpleNamespace(workspace=None, pick=False)
        with mock.patch("sys.stdin.isatty", return_value=True), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch.dict(os.environ, {}, clear=True):
            self.assertTrue(commands_frame._picker_wanted(args, None))

    def test_replacing_the_stream_declares_by_construction(self):
        """What the hook-driving helpers and `test_mcp_approval`'s `_Tty` already do: the
        guard's own stream is not consulted at all."""
        with mock.patch("sys.stdin", io.StringIO("hello\n")):
            self.assertFalse(sys.stdin.isatty())
            self.assertEqual(input(), "hello")

    def test_saying_nobody_is_watching(self):
        _ttyguard.no_terminal()
        self.assertFalse(sys.stdin.isatty())

    def test_the_pin_is_restored_afterwards_so_the_next_read_is_refused_again(self):
        with mock.patch("sys.stdin.isatty", return_value=True):
            self.assertTrue(sys.stdin.isatty())
        with self.assertRaises(_ttyguard.AmbientTerminalRead):
            sys.stdin.isatty()


class TheDeclarationDoesNotOutliveItsTest(unittest.TestCase):
    """The same save/restore `_envguard` needs, for the same reason: a few cases run an
    inner `TestCase` inside their own body, and the inner run must not disarm the outer."""

    class _Inner(unittest.TestCase):
        def runTest(self):
            _ttyguard.no_terminal()
            assert sys.stdin.isatty() is False

    def test_this_tests_own_state_survives_running_an_inner_case(self):
        self._Inner().run(unittest.TestResult())
        with self.assertRaises(_ttyguard.AmbientTerminalRead):
            sys.stdin.isatty()

    def test_a_completed_declaring_case_leaves_the_guard_armed(self):
        result = unittest.TestResult()
        self._Inner().run(result)
        self.assertEqual(result.errors, [])
        self.assertTrue(_ttyguard._active)


if __name__ == "__main__":
    unittest.main()
