"""No test may replace the process that is running the suite (`tests/_execguard.py`).

A real exec from the test runner is not a failure, it is a missing verdict: the runner is
gone, and what reports next is whatever program took its place. #998's deletion sweep
measured it — a mutant sent `tests.test_a_profile_launch_is_refused_before_tmux` through
`commands_frame.bypass` into a real `python -m charter frame-launch --select`, and the
mutation came back unresolved rather than red.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import commands_frame
from tests import _execguard


class AnExecThatWouldRunFailsTheTestThatMadeIt(unittest.TestCase):
    def test_bypass_into_a_real_program_is_refused_by_name(self):
        with self.assertRaises(_execguard.ReplacedTheRunner) as caught:
            commands_frame.bypass([sys.executable, "-c", "pass"])
        self.assertIn("replaces the test runner", str(caught.exception))
        self.assertIn(sys.executable, str(caught.exception))

    def test_it_is_not_an_oserror_so_no_candidate_loop_takes_it_for_a_miss(self):
        """`os._execvpe` walks `PATH` catching `OSError` per candidate; an `OSError` here
        would be swallowed and the NEXT candidate tried — which may be the real one."""
        self.assertFalse(issubclass(_execguard.ReplacedTheRunner, OSError))
        self.assertTrue(issubclass(_execguard.ReplacedTheRunner, AssertionError))

    def test_the_environment_spelling_is_refused_too(self):
        with self.assertRaises(_execguard.ReplacedTheRunner):
            getattr(os, "execvpe")(sys.executable, [sys.executable, "-c", "pass"],
                                   dict(os.environ))


class AnExecThatCannotRunStillFailsTheWayTheKernelSays(unittest.TestCase):
    """Nothing is replaced when an exec fails, so that path is real behaviour — and it is
    how `bypass`'s own "not installed" sentence is tested."""

    def test_a_program_nobody_installed_is_still_127(self):
        with mock.patch.object(commands_frame.util, "err"):
            self.assertEqual(commands_frame.bypass(["no-such-harness-anywhere-xyz"]), 127)

    def test_a_file_that_is_not_executable_is_still_a_permission_error(self):
        import tempfile

        with tempfile.NamedTemporaryFile() as f, \
                mock.patch.object(commands_frame.util, "err"):
            self.assertEqual(commands_frame.bypass([f.name]), 126)

    def test_a_directory_is_not_something_to_run(self):
        import tempfile

        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(PermissionError):
                getattr(os, "execv")(d, [d])


class AForkedChildIsNeverRefused(unittest.TestCase):
    def test_a_process_that_is_not_the_runner_execs_for_real(self):
        """`pty.fork()` then `os.execvp("tmux", …)` is how a real-terminal case puts a real
        client on a server; the process replaced there is the child's own."""
        reached: list = []
        real = mock.Mock(side_effect=lambda *a: reached.append(a))
        exec_ = _execguard._tripwire("execv", real)
        with mock.patch.object(_execguard.os, "getpid",
                               return_value=(_execguard._RUNNER_PID or 0) + 1):
            exec_(sys.executable, [sys.executable])
        self.assertEqual(reached, [(sys.executable, [sys.executable])])


if __name__ == "__main__":
    unittest.main()
