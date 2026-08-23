"""Everything that touches the tmux binary, in one module, so the rest is testable.

The messages are asserted because `Deficit` already settled what an absent capability has
to read like: naming the limit and the command that closes it, never a guess, because "a
remedy that does not exist costs more than an honest gap".
"""

from __future__ import annotations

import subprocess
import unittest
from unittest import mock

from charter.frame import tmuxctl


class Version(unittest.TestCase):
    def test_a_release_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.7c"):
            self.assertEqual(tmuxctl.version(), (3, 7))

    def test_a_two_part_version_parses(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux 3.2"):
            self.assertEqual(tmuxctl.version(), (3, 2))

    def test_an_absent_binary_is_none_not_zero(self):
        """None is 'charter has nothing to say', which reads differently from 'version
        0.0' — the distinction `registry.deficits` makes for an unknown harness."""
        with mock.patch.object(tmuxctl, "_probe", return_value=None):
            self.assertIsNone(tmuxctl.version())

    def test_unparseable_output_is_none_rather_than_a_crash(self):
        with mock.patch.object(tmuxctl, "_probe", return_value="tmux next-3.9"):
            self.assertIsNone(tmuxctl.version())


class Floor(unittest.TestCase):
    def test_the_floor_is_the_version_the_frame_is_checked_against(self):
        """The NUMBER is unchanged; its justification was wrong. `FLOOR` used to be
        documented as "the version `display-popup` needs" — `display-popup` appears
        nowhere in shipped code, in any form, only in two comments. What actually sits
        near 3.2 is the pane-scoped hook (`set-hook -p`) the exit-code mechanism rests
        on; `display-menu`, the other named reason, is 3.0. Kept at 3.2 deliberately
        rather than lowered on an unverified reading of tmux's CHANGES: no tmux older
        than 3.7c exists on this machine to test against, the cost of the floor being
        too HIGH is one accurate warning, and the cost of it being too LOW is a frame
        that starts and then declines to attach."""
        self.assertEqual(tmuxctl.FLOOR, (3, 2))

    def test_the_resize_hook_floor_stays_above_the_frames_own(self):
        """Two floors, two meanings — folding them into one would refuse (or warn
        about) the whole frame over a gap that only costs cosmetic resize drift."""
        self.assertGreater(tmuxctl.RESIZE_HOOK_FLOOR, tmuxctl.FLOOR)


class Messages(unittest.TestCase):
    def test_the_absent_message_names_the_command_that_fixes_it(self):
        msg = tmuxctl.absent_message()
        self.assertIn("tmux", msg)
        self.assertIn("--no-frame", msg)

    def test_the_below_floor_message_names_both_versions(self):
        msg = tmuxctl.below_floor_message((3, 0))
        self.assertIn("3.0", msg)
        self.assertIn("3.2", msg)

    def test_the_below_floor_message_does_not_claim_the_hotkey_is_disabled(self):
        """It said "the frame starts with the hotkey disabled". Nothing disables it:
        `cmd_launch` warns and continues, and `conf_text` still emits the bind. A
        message describing a mechanism that does not exist is worse than none, because
        it is the one place an operator looks to find out what they are losing."""
        msg = tmuxctl.below_floor_message((3, 0))
        self.assertNotIn("hotkey disabled", msg)
        self.assertIn("stays bound", msg)
        self.assertIn("exit code", msg)


class RunArgv(unittest.TestCase):
    def test_run_rejects_a_string_with_typeerror(self):
        """The argv guard survives `python -O`, so it raises TypeError, not AssertionError."""
        with self.assertRaises(TypeError):
            tmuxctl.run("starting the frame", "tmux new-session")

    def test_interact_rejects_a_string_too(self):
        """The same rule on the other door out of this module — a guard on one of two
        entry points is not a guard."""
        with self.assertRaises(TypeError):
            tmuxctl.interact("tmux attach -t x")


class RunGuardsTheTimeout(unittest.TestCase):
    """The defect this wrapper exists for: `cmd_launch` issued eleven
    `subprocess.run(..., timeout=15)` calls with nothing catching
    `subprocess.TimeoutExpired`, and TEN of them run after `new-session` has already
    started the harness detached. A wedged tmux server therefore gave the operator a
    traceback, a filed charter crash report, an orphaned agent session and no reattach
    line — for a condition that is not a charter bug at all."""

    def test_a_timeout_becomes_a_return_code_rather_than_an_exception(self):
        with mock.patch("subprocess.run",
                        side_effect=subprocess.TimeoutExpired(["tmux"], 15)), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("installing the exit-status hook", ["tmux", "-V"])
        self.assertEqual(proc.returncode, tmuxctl.TIMED_OUT)
        self.assertNotEqual(proc.returncode, 0, "a timeout must never read as success")
        err.assert_called_once()

    def test_a_tmux_that_cannot_be_started_becomes_a_return_code_too(self):
        with mock.patch("subprocess.run", side_effect=OSError("no such binary")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("starting the frame", ["tmux", "-V"])
        self.assertEqual(proc.returncode, tmuxctl.COULD_NOT_RUN)
        err.assert_called_once()

    def test_a_failure_names_the_action_and_tmuxs_own_stderr(self):
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 1, stdout="", stderr="no such session: nope")), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            buf = []
            tmuxctl.run("ending the frame after an early death", ["tmux", "-V"])
        self.assertTrue(any("ending the frame" in m and "no such session" in m
                            for m in buf), buf)

    def test_report_false_stays_silent_but_still_reports_the_code(self):
        """`_live_sessions` asks against a socket no server has ever run on, where a
        non-zero return is the ORDINARY answer — reporting it would print an error on
        every first launch on a machine."""
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 1, stdout="", stderr="no server running")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("listing the frames already running", ["tmux", "-V"],
                               report=False)
        self.assertEqual(proc.returncode, 1)
        err.assert_not_called()

    def test_a_success_reports_nothing(self):
        with mock.patch("subprocess.run", return_value=subprocess.CompletedProcess(
                ["tmux"], 0, stdout="%7\n", stderr="")), \
             mock.patch("charter.util.err") as err:
            proc = tmuxctl.run("starting the frame", ["tmux", "-V"])
        self.assertEqual(proc.stdout.strip(), "%7")
        err.assert_not_called()

    def test_interact_neither_captures_nor_time_boxes(self):
        """`attach` IS the operator's terminal for as long as the harness runs, and
        `display-menu` waits for a keypress — capturing either would swallow the
        operator's own screen, and a timeout on either would end a frame for the crime
        of being used."""
        with mock.patch("subprocess.run",
                        return_value=subprocess.CompletedProcess(["tmux"], 0)) as run:
            tmuxctl.interact(["tmux", "attach"], env={"A": "b"})
        _, kwargs = run.call_args
        self.assertNotIn("timeout", kwargs)
        self.assertNotIn("capture_output", kwargs)
        self.assertEqual(kwargs["env"], {"A": "b"})


if __name__ == "__main__":
    unittest.main()
