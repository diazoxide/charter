"""Backfill reconciles by identity, so a window the live hook missed is still reachable
(issue #83).

`PostToolUse(Task|Agent)` does not fire for every dispatch — background sub-agents in
particular return immediately and complete later, and charter never sees them. On the
reporting plane, three real dispatches on 2026-08-10 were absent from a trace store whose
last record was 2026-08-07, and `charter persona stats` showed `DISP 0 / never dispatched`
for personas that had just done the work.

Backfill existed and could have found them: transcripts record every dispatch, and
`scan_transcripts` already reads them. It was the double-count guard that put them out of
reach — everything at or after `_earliest_live()` was skipped, on one global timestamp.
The guard's job is "do not count the same dispatch twice"; expressing that as a cutoff
also asserted that live recording was complete from the first live record onward, which is
exactly what this issue disproves. One early record disabled reconciliation for good.

Identity — (timestamp, agent) — is the same guarantee without the assumption, and it
self-heals: whenever backfill next runs it picks up whatever the hook missed, without
anyone having to work out which window that was.

Verified on this plane while diagnosing: six dispatches in the transcripts, zero in the
tally, and `personas/_dispatch` never created at all.
"""
from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import dispatch


class BackfillCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-backfill-"))
        self.d = self.tmp / "_dispatch"
        self.d.mkdir(parents=True)
        p = mock.patch.object(dispatch, "_dir", lambda: self.d)
        p.start()
        self.addCleanup(p.stop)
        self.addCleanup(lambda: shutil.rmtree(self.tmp, ignore_errors=True))

    def live(self, *rows):
        """Dispatches the live hook did record."""
        (self.d / "2026-08.host.jsonl").write_text(
            "".join(json.dumps({"ts": ts, "agent": a}) + "\n" for ts, a in rows))

    def transcripts(self, *rows):
        return mock.patch.object(dispatch, "scan_transcripts",
                                 lambda: [{"ts": ts, "agent": a} for ts, a in rows])

    def rows(self) -> list[tuple[str, str]]:
        """Timestamps are normalised through `_ts` on both the live and backfill sides,
        so compare the way the code does rather than against a literal."""
        out = []
        for f in sorted(self.d.glob("*.jsonl")):
            for ln in f.read_text().splitlines():
                o = json.loads(ln)
                out.append((dispatch._ts(o).isoformat(), o["agent"]))
        return sorted(out)


class TestTheGapTheCutoffCouldNotReach(BackfillCase):
    """The report, reduced: live records from the 7th, a hook-missed dispatch on the
    10th. Under the cutoff this was skipped forever."""

    def test_a_dispatch_after_the_earliest_live_record_is_imported(self):
        self.live(("2026-08-07T16:45:04", "datadog-master"))
        with self.transcripts(("2026-08-07T16:45:04", "datadog-master"),
                              ("2026-08-10T09:00:00", "datadog-master")):
            imported, skipped = dispatch.backfill()
        self.assertEqual((imported, skipped), (1, 1))

    def test_the_missed_dispatch_reaches_the_tally(self):
        self.live(("2026-08-07T16:45:04", "datadog-master"))
        with self.transcripts(("2026-08-10T09:00:00", "datadog-master")):
            dispatch.backfill()
        self.assertIn(("2026-08-10T09:00:00+00:00", "datadog-master"), self.rows())

    def test_a_persona_stops_reading_as_never_dispatched(self):
        self.live(("2026-08-07T16:45:04", "someone-else"))
        with self.transcripts(("2026-08-10T09:00:00", "datadog-master"),
                              ("2026-08-10T09:05:00", "psa-integrations-master")):
            dispatch.backfill()
        self.assertEqual(dispatch.tally().get("datadog-master"), 1)


class TestNothingIsCountedTwice(BackfillCase):
    """The guard still has to do its actual job."""

    def test_a_dispatch_the_hook_already_recorded_is_skipped(self):
        self.live(("2026-08-07T16:45:04", "datadog-master"))
        with self.transcripts(("2026-08-07T16:45:04", "datadog-master")):
            imported, skipped = dispatch.backfill()
        self.assertEqual((imported, skipped), (0, 1))
        self.assertEqual(len(self.rows()), 1)

    def test_the_same_dispatch_in_two_transcripts_lands_once(self):
        with self.transcripts(("2026-08-10T09:00:00", "a"), ("2026-08-10T09:00:00", "a")):
            imported, _ = dispatch.backfill()
        self.assertEqual(imported, 1)

    def test_two_agents_at_the_same_instant_are_both_kept(self):
        """A fan-out dispatches several sub-agents in the same second. Deduping on the
        timestamp alone would silently drop every one but the first."""
        with self.transcripts(("2026-08-10T09:00:00", "a"), ("2026-08-10T09:00:00", "b")):
            imported, _ = dispatch.backfill()
        self.assertEqual(imported, 2)

    def test_rerunning_is_idempotent(self):
        with self.transcripts(("2026-08-10T09:00:00", "a")):
            dispatch.backfill()
            dispatch.backfill()
        self.assertEqual(len(self.rows()), 1)

    def test_an_empty_tally_imports_everything(self):
        """This plane, when the gap was found: six in the transcripts, nothing recorded,
        `_dispatch` not even created."""
        with self.transcripts(*[(f"2026-08-12T09:0{i}:00", "general-purpose")
                                for i in range(6)]):
            imported, skipped = dispatch.backfill()
        self.assertEqual((imported, skipped), (6, 0))


class TestReportingHowCompleteTheTallyIs(BackfillCase):
    def test_never_reconciled_when_nothing_was_backfilled(self):
        self.assertIsNone(dispatch.last_backfill())

    def test_a_reconciliation_is_dated(self):
        with self.transcripts(("2026-08-10T09:00:00", "a")):
            dispatch.backfill()
        self.assertIsNotNone(dispatch.last_backfill())

    def test_live_records_alone_do_not_count_as_a_reconciliation(self):
        """The live store being non-empty says nothing about whether transcripts were
        ever checked — which is the whole failure."""
        self.live(("2026-08-07T16:45:04", "a"))
        self.assertIsNone(dispatch.last_backfill())


if __name__ == "__main__":
    unittest.main()
