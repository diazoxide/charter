"""Where a **report** lives between being detected and being sent.

Detection is automatic and local; publishing never is (docs/adr/0003). So a report drafted
when charter crashed at 2pm has to still be there when the Reporter approves it at 4pm —
and a Reporter who walks away from a crash loop must not come back to a full disk.
"""
from __future__ import annotations

import unittest

from charter import config, report
from tests._isolation import PersonaIso


def _caught(exc: Exception) -> Exception:
    try:
        raise exc
    except Exception as e:  # noqa: BLE001
        return e


def _distinct(i: int) -> Exception:
    """A genuinely different bug.

    Note what does NOT make one: a different message or a different subcommand. The
    fingerprint is exception type plus failure site, so `clone` and `doctor` both dying
    with ValueError at the same charter line are ONE bug — correctly, since one fix closes
    both. Varying the exception type is what actually produces distinct reports.
    """
    return _caught(type(f"SynthError{i}", (Exception,), {})("boom"))


class TestRecordingABug(PersonaIso):
    def test_a_recorded_bug_can_be_read_back(self):
        rid = report.record_bug(_caught(ValueError("boom")), subcommand="clone")
        loaded = report.load(rid)
        self.assertEqual(loaded["payload"]["exception_type"], "ValueError")
        self.assertEqual(loaded["payload"]["subcommand"], "clone")

    def test_a_new_report_starts_unsent(self):
        """`sent` is what separates a draft from something already public — and what the
        Reporter's approval flips. It must never start true."""
        loaded = report.load(report.record_bug(_caught(ValueError("boom")), subcommand="clone"))
        self.assertIsNone(loaded["issue_url"])
        self.assertEqual(loaded["occurrences"], 1)

    def test_reports_are_written_under_the_state_dir(self):
        """Gitignored, per-developer, beside the other per-session state — never inside a
        workspace and never anywhere committed."""
        report.record_bug(_caught(ValueError("boom")), subcommand="clone")
        self.assertTrue(config.REPORTS_DIR.is_relative_to(config.STATE_DIR))
        self.assertTrue(any(config.REPORTS_DIR.iterdir()))


class TestCrashLoopCollapse(PersonaIso):
    """A broken config makes every invocation fail identically. That is one report with a
    count, not a hundred files — and the count is useful signal upstream ('hit 47 times')."""

    def _same_bug(self):
        return report.record_bug(_caught(ValueError("boom")), subcommand="clone")

    def test_the_same_bug_twice_is_one_report(self):
        first, second = self._same_bug(), self._same_bug()
        self.assertEqual(first, second)
        self.assertEqual(len(report.pending()), 1)

    def test_repeats_increment_the_occurrence_counter(self):
        for _ in range(5):
            rid = self._same_bug()
        self.assertEqual(report.load(rid)["occurrences"], 5)

    def test_a_different_bug_is_its_own_report(self):
        self._same_bug()
        report.record_bug(_caught(KeyError("other")), subcommand="doctor")
        self.assertEqual(len(report.pending()), 2)

    def test_the_subcommand_does_not_split_one_bug_into_two(self):
        """`clone` and `doctor` failing the same way at the same place is one bug — one
        fix closes both. Pinned because it is the surprising half of the fingerprint rule."""
        report.record_bug(_caught(ValueError("boom")), subcommand="clone")
        report.record_bug(_caught(ValueError("boom")), subcommand="doctor")
        self.assertEqual(len(report.pending()), 1)

    def test_distinct_pending_reports_are_capped(self):
        """Past the cap, recording stops rather than filling the disk. Returns None so a
        caller can tell it was refused instead of silently believing it recorded."""
        for i in range(report.MAX_PENDING):
            report.record_bug(_distinct(i), subcommand=f"cmd{i}")
        self.assertEqual(len(report.pending()), report.MAX_PENDING)

        refused = report.record_bug(_distinct(9999), subcommand="zzz")
        self.assertIsNone(refused)
        self.assertEqual(len(report.pending()), report.MAX_PENDING)

    def test_the_cap_still_lets_an_existing_report_collapse(self):
        """The cap bounds DISTINCT reports. A repeat of one already on disk must still
        count, or a crash loop at the cap would stop counting the very thing looping."""
        first = self._same_bug()
        for i in range(report.MAX_PENDING):
            report.record_bug(_distinct(i), subcommand=f"cmd{i}")
        self.assertEqual(report.load(self._same_bug())["occurrences"], 2)
        self.assertEqual(first, self._same_bug())


class TestPendingListing(PersonaIso):
    def test_nothing_recorded_means_nothing_pending(self):
        self.assertEqual(report.pending(), [])

    def test_a_sent_report_is_no_longer_pending(self):
        """Sent reports are kept (they carry the issue URL a later repeat points at), but
        they are not drafts awaiting approval."""
        rid = report.record_bug(_caught(ValueError("boom")), subcommand="clone")
        report.mark_sent(rid, "https://github.com/diazoxide/charter/issues/7")
        self.assertEqual(report.pending(), [])
        self.assertEqual(report.load(rid)["issue_url"],
                         "https://github.com/diazoxide/charter/issues/7")


if __name__ == "__main__":
    unittest.main()
