"""A **gap** — charter cannot do something it should — and the scrubbing it needs.

A bug payload is structured, so it can be allowlisted field by field. A gap is free prose
composed by the reporting agent while it holds the Reporter's private repo names, workspace
names, and whatever they were actually trying to do. Mechanical scrubbing catches the
careless half; the human read (docs/adr/0003) catches the rest, because nothing mechanical
catches "we need this because our billing service can't do X".
"""
from __future__ import annotations

import unittest
from pathlib import Path

from charter import config, report, workspace
from tests._isolation import PersonaIso


class TestScrubbingProse(PersonaIso):
    def test_an_absolute_home_path_does_not_survive(self):
        """Absolute paths carry the Reporter's username in the second segment."""
        out = report.scrub(f"it broke at {Path.home()}/code/billing/app.py")
        self.assertNotIn(str(Path.home()), out)

    def test_a_workspace_name_is_replaced(self):
        """charter knows its own workspace names, which a generic scrubber never would.
        A workspace is usually named after the client or the project."""
        workspace.ensure("acme-migration")
        out = report.scrub("charter clone fails inside acme-migration every time")
        self.assertNotIn("acme-migration", out)

    def test_a_persona_name_is_replaced(self):
        self.make_persona("client-billing-reviewer")
        out = report.scrub("the client-billing-reviewer persona cannot do this")
        self.assertNotIn("client-billing-reviewer", out)

    def test_the_surrounding_prose_survives(self):
        """Scrubbing must not shred the report — what remains has to still describe the
        gap, or the Reporter is left approving something useless."""
        workspace.ensure("acme")
        out = report.scrub("charter clone fails inside acme every time")
        self.assertIn("charter clone fails inside", out)
        self.assertIn("every time", out)

    def test_scrubbing_is_visible_not_silent(self):
        """A redaction the Reporter cannot see reads as charter having sent the original.
        The placeholder is the signal that something was removed."""
        workspace.ensure("acme-migration")
        self.assertIn(report.REDACTED, report.scrub("broken in acme-migration"))

    def test_text_with_nothing_sensitive_is_returned_unchanged(self):
        text = "charter has no way to archive a workspace"
        self.assertEqual(report.scrub(text), text)

    def test_the_default_workspace_name_is_not_scrubbed(self):
        """`default` exists on every install, so it identifies nobody — and redacting it
        would mangle ordinary English in most reports."""
        workspace.ensure(config.DEFAULT_WORKSPACE)
        text = f"the {config.DEFAULT_WORKSPACE} workspace behaves differently"
        self.assertEqual(report.scrub(text), text)


class TestRecordingAGap(PersonaIso):
    def test_a_gap_is_recorded_and_readable(self):
        rid = report.record_gap("charter has no way to archive a workspace")
        rec = report.load(rid)
        self.assertEqual(rec["kind"], "gap")
        self.assertEqual(rec["issue_url"], None)

    def test_a_gaps_text_is_scrubbed_on_the_way_in(self):
        """Scrubbed at record time, not at send time. A draft sitting on disk unredacted
        is a draft someone can publish without the scrub ever having run."""
        workspace.ensure("acme-migration")
        rec = report.load(report.record_gap("no archive command for acme-migration"))
        self.assertNotIn("acme-migration", rec["payload"]["text"])

    def test_two_different_gaps_are_two_reports(self):
        report.record_gap("no way to archive a workspace")
        report.record_gap("no way to rename a persona")
        self.assertEqual(len(report.pending()), 2)

    def test_the_same_gap_text_twice_is_one_report(self):
        """Gaps have no stack frame to fingerprint, but identical text is still the same
        request — an agent re-reporting on every session should not accumulate."""
        first = report.record_gap("no way to archive a workspace")
        second = report.record_gap("no way to archive a workspace")
        self.assertEqual(first, second)
        self.assertEqual(len(report.pending()), 1)

    def test_a_gap_carries_the_version_but_no_traceback_fields(self):
        """A gap is not a crash: there is no exception type and no frame to report."""
        rec = report.load(report.record_gap("no way to archive a workspace"))
        self.assertIn("charter_version", rec["payload"])
        self.assertNotIn("exception_type", rec["payload"])
        self.assertNotIn("frames", rec["payload"])


if __name__ == "__main__":
    unittest.main()
