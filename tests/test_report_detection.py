"""Automatic crash detection — the third `except` in `cli.main`.

`main` already caught exactly `KeyboardInterrupt` and `util.ProcTimeout`, the latter with
the comment *"A child that outlived its budget is a **condition, not a bug**"*. That line
predates upstream reporting and defines its rule for free: a **condition** never produces a
report, an uncaught exception always does.

Detection is on by default because it only ever writes to the Reporter's own disk — see
docs/adr/0003 for why publishing is a separate, human-gated step.
"""
from __future__ import annotations

import unittest
from unittest import mock

from charter import cli, report, util
from tests._isolation import PersonaIso


class TestConditionsAreNotBugs(PersonaIso):
    """The distinction charter already drew, now load-bearing. A Reporter's Ctrl-C must
    never become an upstream issue, and a slow child process is not charter's bug."""

    def test_a_keyboard_interrupt_records_nothing(self):
        with mock.patch("charter.commands.cmd_doctor", side_effect=KeyboardInterrupt):
            rc = cli.main(["doctor"])
        self.assertEqual(rc, 130)
        self.assertEqual(report.pending(), [])

    def test_a_process_timeout_records_nothing(self):
        with mock.patch("charter.commands.cmd_doctor",
                        side_effect=util.ProcTimeout(["git", "fetch"], 5)):
            rc = cli.main(["doctor"])
        self.assertEqual(rc, 1)
        self.assertEqual(report.pending(), [])


class TestUncaughtExceptionsAreRecorded(PersonaIso):
    def test_a_crash_records_exactly_one_report(self):
        with mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                cli.main(["doctor"])
        self.assertEqual(len(report.pending()), 1)

    def test_the_crash_still_reaches_the_developer(self):
        """Recording must not swallow the failure. A charter that quietly files a report
        and exits 0 would hide the very breakage it is reporting."""
        with mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                cli.main(["doctor"])

    def test_the_report_names_the_subcommand_that_failed(self):
        with mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                cli.main(["doctor"])
        self.assertEqual(report.pending()[0]["payload"]["subcommand"], "doctor")

    def test_the_report_carries_charters_own_frames(self):
        """Proves the traceback survived far enough to be useful — a report with no frames
        would tell the maintainer nothing about where it broke."""
        with mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                cli.main(["doctor"])
        self.assertTrue(report.pending()[0]["payload"]["frames"])

    def test_a_repeated_crash_collapses(self):
        for _ in range(3):
            with mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
                with self.assertRaises(ValueError):
                    cli.main(["doctor"])
        self.assertEqual(len(report.pending()), 1)
        self.assertEqual(report.pending()[0]["occurrences"], 3)


class TestDetectionNeverBreaksTheCli(PersonaIso):
    def test_a_failure_to_record_does_not_mask_the_original_crash(self):
        """If reporting itself breaks, the Reporter must still see the real error. A
        bug in the bug reporter is the worst possible thing to surface instead."""
        with mock.patch("charter.report.record_bug", side_effect=OSError("disk full")), \
             mock.patch("charter.commands.cmd_doctor", side_effect=ValueError("boom")):
            with self.assertRaises(ValueError):
                cli.main(["doctor"])


if __name__ == "__main__":
    unittest.main()
