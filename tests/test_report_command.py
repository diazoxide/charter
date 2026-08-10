"""The `charter report` surface — the drafting half.

Two commands exist because the second one *is* the consent (docs/adr/0003): `report
bug|gap` drafts and shows, `report send` publishes. This module covers drafting, and its
central claim is a negative one — **drafting touches no network**. That is what lets
detection default to on without charter reading as telemetry.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_report, report, workspace
from tests._isolation import PersonaIso


class TestDraftingABug(PersonaIso):
    def _run(self, text):
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = commands_report.cmd_report_bug(SimpleNamespace(text=text))
        return rc, buf.getvalue()

    def test_it_records_a_report_and_succeeds(self):
        rc, _ = self._run("charter clone crashes on a private repo")
        self.assertEqual(rc, 0)
        self.assertEqual(len(report.pending()), 1)

    def test_it_prints_the_id_the_reporter_needs_to_send(self):
        """The id is the handle for the second command. Without it printed, the two-step
        flow is unusable and the Reporter cannot approve anything."""
        _, out = self._run("charter clone crashes on a private repo")
        self.assertIn(report.pending()[0]["id"], out)

    def test_it_prints_the_exact_text_that_would_be_published(self):
        """Shown, not summarised. The Reporter is being asked to approve a payload, so
        approving something they were never shown would make the approval meaningless."""
        _, out = self._run("charter clone crashes on a private repo")
        self.assertIn("charter clone crashes on a private repo", out)

    def test_it_shows_the_scrubbed_text_not_the_original(self):
        """What is displayed must be what would be sent. Showing the original while
        sending the scrubbed version is a confusing lie; the reverse is a dangerous one."""
        workspace.ensure("acme-migration")
        _, out = self._run("clone fails in acme-migration")
        self.assertNotIn("acme-migration", out)
        self.assertIn(report.REDACTED, out)


class TestDraftingTouchesNoNetwork(PersonaIso):
    """The load-bearing test of the whole phase. Drafting must not reach the network under
    any circumstance — that is the difference between a local draft and telemetry."""

    def test_drafting_a_bug_runs_no_subprocess(self):
        with mock.patch("charter.util.run") as run:
            commands_report.cmd_report_bug(SimpleNamespace(text="something broke"))
        run.assert_not_called()

    def test_drafting_a_gap_runs_no_subprocess(self):
        with mock.patch("charter.util.run") as run:
            commands_report.cmd_report_gap(SimpleNamespace(text="no archive command"))
        run.assert_not_called()


class TestDraftingAGap(PersonaIso):
    def test_it_records_a_gap_not_a_bug(self):
        rc = commands_report.cmd_report_gap(SimpleNamespace(text="no way to archive"))
        self.assertEqual(rc, 0)
        self.assertEqual(report.pending()[0]["kind"], "gap")


class TestNothingIsSentByDrafting(PersonaIso):
    def test_a_drafted_report_is_not_marked_sent(self):
        commands_report.cmd_report_bug(SimpleNamespace(text="something broke"))
        self.assertIsNone(report.pending()[0]["issue_url"])


if __name__ == "__main__":
    unittest.main()
