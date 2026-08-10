"""`charter report send` — the only command here that touches the network.

Every test stubs :func:`charter.report.gh`, the single seam through which reporting reaches
a forge (docs/adr/0002). Nothing in this module may reach a real tracker.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_report, report
from tests._isolation import ReportIso

_ISSUE = "https://github.com/diazoxide/charter/issues/7"


class SendCase(ReportIso):
    def setUp(self) -> None:
        super().setUp()
        report.grant_consent()

    def draft(self, text="charter cannot archive a workspace") -> str:
        commands_report.cmd_report_gap(SimpleNamespace(text=text))
        return report.pending()[0]["id"]

    def send(self, rid, *, new=False, dry_run=False, dups=None, gh_result=_ISSUE):
        buf = io.StringIO()
        with mock.patch("charter.report.gh", return_value=gh_result) as gh, \
             mock.patch("charter.report.search_duplicates", return_value=dups or []):
            with redirect_stdout(buf):
                rc = commands_report.cmd_report_send(
                    SimpleNamespace(id=rid, new=new, dry_run=dry_run))
        return rc, buf.getvalue(), gh


class TestASuccessfulSend(SendCase):
    def test_it_records_the_issue_url_against_the_report(self):
        rid = self.draft()
        rc, _, _ = self.send(rid)
        self.assertEqual(rc, 0)
        self.assertEqual(report.load(rid)["issue_url"], _ISSUE)

    def test_a_sent_report_is_kept_not_deleted(self):
        """Kept so the next identical failure can point at the existing upstream issue
        instead of drafting again — local dedupe at zero API cost."""
        rid = self.draft()
        self.send(rid)
        self.assertIsNotNone(report.load(rid))

    def test_sending_twice_does_not_open_a_second_issue(self):
        rid = self.draft()
        self.send(rid)
        rc, out, gh = self.send(rid)
        self.assertEqual(rc, 0)
        gh.assert_not_called()

    def test_the_issue_is_labelled_at_creation(self):
        """`via-charter-report` is what separates machine-drafted reports from
        hand-written ones — and how you measure whether this feature works at all."""
        rid = self.draft()
        _, _, gh = self.send(rid)
        args = gh.call_args[0][0]
        self.assertIn("via-charter-report", args)
        self.assertIn("gap", args)

    def test_the_body_sent_is_the_body_shown(self):
        """One renderer for both. If they could drift, the Reporter's approval would stop
        meaning anything."""
        rid = self.draft()
        _, _, gh = self.send(rid)
        args = gh.call_args[0][0]
        self.assertIn(report.issue_body(report.load(rid)), args)


class TestDuplicateHandling(SendCase):
    _DUP = [{"number": 12, "title": "cannot archive a workspace",
             "url": "https://github.com/diazoxide/charter/issues/12"}]

    def test_a_candidate_duplicate_stops_the_send(self):
        rid = self.draft()
        rc, out, gh = self.send(rid, dups=self._DUP)
        self.assertEqual(rc, 2)
        gh.assert_not_called()
        self.assertIsNone(report.load(rid)["issue_url"])

    def test_the_candidates_are_shown_so_someone_can_judge_them(self):
        """Candidates, not a verdict — a keyword score cannot tell 'clone fails on a
        private repo' from 'clone fails on a submodule'."""
        rid = self.draft()
        _, out, _ = self.send(rid, dups=self._DUP)
        self.assertIn("12", out)
        self.assertIn("https://github.com/diazoxide/charter/issues/12", out)

    def test_new_overrides_the_duplicate_check(self):
        """The Reporter believing their case is different must be able to say so — an
        over-eager duplicate check cannot be allowed to silence a real bug."""
        rid = self.draft()
        rc, _, gh = self.send(rid, new=True, dups=self._DUP)
        self.assertEqual(rc, 0)
        self.assertTrue(gh.called)


class TestDryRun(SendCase):
    def test_dry_run_sends_nothing(self):
        rid = self.draft()
        rc, _, gh = self.send(rid, dry_run=True)
        self.assertEqual(rc, 0)
        gh.assert_not_called()
        self.assertIsNone(report.load(rid)["issue_url"])

    def test_dry_run_shows_what_would_be_sent(self):
        rid = self.draft()
        _, out, _ = self.send(rid, dry_run=True)
        self.assertIn("charter cannot archive a workspace", out)

    def test_dry_run_touches_no_network_at_all(self):
        """Not even the duplicate search or the `gh auth` probe. This is the offline
        affordance the feature was developed against — it must not need a forge."""
        rid = self.draft()
        buf = io.StringIO()
        with mock.patch("charter.report.gh") as gh, \
             mock.patch("charter.report.gh_available") as avail, \
             mock.patch("charter.report.search_duplicates") as search:
            with redirect_stdout(buf):
                rc = commands_report.cmd_report_send(
                    SimpleNamespace(id=rid, new=False, dry_run=True))
        self.assertEqual(rc, 0)
        gh.assert_not_called()
        avail.assert_not_called()
        search.assert_not_called()

    def test_a_duplicate_does_not_short_circuit_a_dry_run(self):
        """A dry run asked to see the payload. Returning 'this may be a duplicate' without
        ever showing it would answer a question the Reporter did not ask."""
        rid = self.draft()
        _, out, _ = self.send(rid, dry_run=True, dups=TestDuplicateHandling._DUP)
        self.assertIn("charter cannot archive a workspace", out)


class TestFailureIsLoud(SendCase):
    def test_a_forge_failure_does_not_report_success(self):
        """The `_api` contract — swallow everything, return None — would mean the
        Reporter believes they reported something they did not (docs/adr/0002)."""
        rid = self.draft()
        buf = io.StringIO()
        with mock.patch("charter.report.gh",
                        side_effect=report.ReportingError("label not found")), \
             mock.patch("charter.report.search_duplicates", return_value=[]):
            with redirect_stdout(buf):
                rc = commands_report.cmd_report_send(
                    SimpleNamespace(id=rid, new=False, dry_run=False))
        self.assertEqual(rc, 1)

    def test_the_draft_survives_a_failed_send(self):
        rid = self.draft()
        with mock.patch("charter.report.gh", side_effect=report.ReportingError("boom")), \
             mock.patch("charter.report.search_duplicates", return_value=[]):
            commands_report.cmd_report_send(SimpleNamespace(id=rid, new=False, dry_run=False))
        self.assertIsNone(report.load(rid)["issue_url"])
        self.assertEqual(len(report.pending()), 1)


class TestNoGithubIdentity(SendCase):
    """charter supports GitLab forges, so a Reporter with no `gh` is a real population —
    and this is also the escape hatch when `gh` is broken or rate-limited."""

    def test_it_offers_a_prefilled_url_instead_of_failing(self):
        rid = self.draft()
        buf = io.StringIO()
        with mock.patch("charter.report.gh_available", return_value=False), \
             mock.patch("charter.report.gh") as gh:
            with redirect_stdout(buf):
                rc = commands_report.cmd_report_send(
                    SimpleNamespace(id=rid, new=False, dry_run=False))
        self.assertEqual(rc, 0)
        gh.assert_not_called()
        self.assertIn("/issues/new", buf.getvalue())

    def test_the_prefilled_url_is_encoded(self):
        rid = self.draft("clone fails & breaks")
        url = report.fallback_url(report.load(rid))
        self.assertNotIn(" ", url)
        self.assertIn("title=", url)
        self.assertIn("body=", url)


class TestUnknownReport(SendCase):
    def test_sending_an_unknown_id_fails_cleanly(self):
        rc, _, gh = self.send("nosuchid")
        self.assertEqual(rc, 1)
        gh.assert_not_called()


class TestCrashTitles(ReportIso):
    def test_a_crash_title_leads_with_exception_type_and_subcommand(self):
        """So duplicates collide visually in the maintainer's issue list."""
        try:
            raise ValueError("boom")
        except ValueError as e:
            rid = report.record_bug(e, subcommand="clone")
        self.assertEqual(report.title(report.load(rid)), "ValueError in `charter clone`")


if __name__ == "__main__":
    unittest.main()
