"""Deterministic memory-curation engine (charter/curate.py): finds exact/near dups, stale,
index drift, and charter-worthy rule candidates; auto-applies ONLY tier-1 safe/reversible
ops (exact-dup collapse via archive + index repair) and returns the rest as proposals.
No LLM, fully deterministic — the semantic judgment stays in the steward agent layer."""

from __future__ import annotations

import datetime
import unittest

from charter import curate, memstore, persona
from tests._isolation import PersonaIso

TODAY = datetime.date(2026, 7, 24)


class CurateCase(PersonaIso):
    def setUp(self):
        # Inside a plane, because `memstore.files` refuses a store that resolves outside
        # the plane's data since #336 — see the header of `tests/test_memstore.py`.
        super().setUp()
        self.d = persona.memory_dir("curatetest")
        self.d.mkdir(parents=True, exist_ok=True)

    def _w(self, title, body, days_ago=0):
        stamp = datetime.datetime(2026, 7, 24, 12, 0) - datetime.timedelta(days=days_ago)
        return memstore.write(self.d, body, title, timestamped=True, stamp=stamp)

    # --- read-only must disclose what --apply would do ---
    def test_pending_auto_names_the_index_repair(self):
        """A read-only report that hides an auto-op is how --apply surprises you.

        Regression: an unindexed memory was silently linked by --apply while the
        read-only run said nothing about it, so there was no way to know the
        command would rewrite MEMORY.md.
        """
        p = self._w("Kept", "a fact")
        memstore.index_path(self.d).write_text("# Memory Index\n")   # drop the link
        rep = curate.report(self.d)
        self.assertEqual(rep["index"]["missing"], [p.name])
        pending = curate.pending_auto(rep)
        self.assertTrue(any("repair index" in x for x in pending), pending)
        self.assertTrue(any(p.name in x for x in pending), pending)

    def test_pending_auto_names_exact_dup_collapse(self):
        self._w("One", "identical body")
        self._w("Two", "identical body")
        rep = curate.report(self.d)
        pending = curate.pending_auto(rep)
        self.assertTrue(any("exact-duplicate" in x for x in pending), pending)

    def test_pending_auto_is_empty_on_a_tidy_corpus(self):
        self._w("Only", "a unique fact")
        self.assertEqual(curate.pending_auto(curate.report(self.d)), [])

    def test_pending_auto_mirrors_what_apply_safe_actually_does(self):
        """The two must not drift: anything announced has to really happen."""
        p = self._w("Kept", "a fact")
        memstore.index_path(self.d).write_text("# Memory Index\n")
        rep = curate.report(self.d)
        self.assertTrue(curate.pending_auto(rep), "announced nothing")
        actions = curate.apply_safe(self.d)
        self.assertTrue(actions, "announced an op that apply_safe did not perform")
        self.assertEqual(curate.report(self.d)["index"]["missing"], [])
        self.assertIn(p.name, memstore.index_path(self.d).read_text())

    def test_a_dangling_link_stays_a_proposal_not_an_auto_op(self):
        """Pruning is a judgment call — the fix may be to write the missing file."""
        self._w("Real", "a fact")
        idx = memstore.index_path(self.d)
        idx.write_text(idx.read_text() + "- [Gone](never-written.md)\n")
        rep = curate.report(self.d)
        self.assertEqual(rep["index"]["orphans"], ["never-written.md"])
        self.assertFalse(any("never-written" in x for x in curate.pending_auto(rep)))
        self.assertTrue(any("never-written" in x for x in curate.proposals(rep)))

    # --- body normalization (exact-dup basis) ---
    def test_body_strips_header_and_normalizes(self):
        a = "# Title A\n\n_2026-07-01 09:00 · persistent_\n\nthe   FACT here\n"
        b = "# Different Title\n\n_2026-07-20 15:30 · persistent_\n\nThe fact   here\n"
        self.assertEqual(memstore.body(a), memstore.body(b))  # same fact, diff title/stamp

    # --- exact duplicates ---
    def test_exact_dups_detected_and_grouped(self):
        self._w("first", "identical content", days_ago=5)
        self._w("second", "identical content", days_ago=1)
        self._w("unique", "something else")
        rep = curate.report(self.d, today=TODAY)
        self.assertEqual(len(rep["exact_dups"]), 1)
        self.assertEqual(len(rep["exact_dups"][0]), 2)

    def test_apply_safe_archives_redundant_keeps_one(self):
        self._w("first", "identical content", days_ago=5)
        self._w("second", "identical content", days_ago=1)
        before = len(memstore.files(self.d))
        actions = curate.apply_safe(self.d)
        self.assertTrue(any("archived exact-duplicate" in a for a in actions))
        # one active memory remains; the other moved to archive/ (reversible, still on disk)
        self.assertEqual(len(memstore.files(self.d)), before - 1)
        self.assertTrue((self.d / "archive").exists())
        self.assertEqual(len(list((self.d / "archive").glob("*.md"))), 1)

    def test_archived_memory_invisible_to_search(self):
        self._w("dup", "shared body text", days_ago=2)
        self._w("dup2", "shared body text", days_ago=1)
        curate.apply_safe(self.d)
        # search only sees the surviving copy, not the archived one
        hits = memstore.search([self.d], "shared body text")
        self.assertEqual(len(hits), 1)

    # --- near dups (proposal, not auto) ---
    def test_near_dups_are_proposed_not_applied(self):
        self._w("a", "the quick brown fox jumps over the lazy dog in the yard")
        self._w("b", "the quick brown fox jumps over the lazy dog in the field")
        rep = curate.report(self.d, today=TODAY)
        self.assertTrue(rep["near_dups"])
        # apply_safe must NOT touch near-dups
        n = len(memstore.files(self.d))
        curate.apply_safe(self.d)
        self.assertEqual(len(memstore.files(self.d)), n)

    # --- stale (proposal, age-based, never auto) ---
    def test_stale_flagged_but_not_archived(self):
        self._w("old", "an old fact", days_ago=200)
        self._w("fresh", "a new fact", days_ago=1)
        rep = curate.report(self.d, stale_days=90, today=TODAY)
        self.assertEqual(len(rep["stale"]), 1)
        self.assertEqual(rep["stale"][0][2], 200)  # age in days
        curate.apply_safe(self.d)  # must not archive by age
        self.assertEqual(len(memstore.files(self.d)), 2)

    # --- charter rule nomination ---
    def test_rule_candidate_scores_standing_rule_high(self):
        self._w("rule", "STANDING RULE: authz work is always backend; never trust the client")
        self._w("plain", "we deployed the service to dev today")
        rep = curate.report(self.d, today=TODAY)
        names = [r[0] for r in rep["rules"]]
        self.assertEqual(len(rep["rules"]), 1)   # only the rule-ish one
        self.assertIn("rule", names[0])

    def test_transient_snapshot_never_nominated(self):
        # deploy snapshots must NOT be charter candidates even with rule-ish words
        self._w("snap", "Deployed state as of today: the gate must always pass; never skip")
        rep = curate.report(self.d, today=TODAY)
        self.assertEqual(rep["rules"], [])

    # --- index health ---
    def test_index_missing_detected_and_repaired(self):
        p = self._w("indexed", "content")
        # write a memory file directly WITHOUT indexing it
        memstore.write(self.d, "orphan body", "unindexed", index=False)
        rep = curate.report(self.d, today=TODAY)
        self.assertEqual(len(rep["index"]["missing"]), 1)
        curate.apply_safe(self.d)
        self.assertEqual(curate.report(self.d, today=TODAY)["index"]["missing"], [])

    def test_index_orphan_ignores_urlish_titles(self):
        # a title containing `](…md` must not be mistaken for a listed file
        self._w("api", "GET /api/x returns ](weird.md fragment in the title text")
        rep = curate.report(self.d, today=TODAY)
        self.assertEqual(rep["index"]["orphans"], [])

    def test_proposals_lists_tier2_only(self):
        self._w("a", "the quick brown fox jumps over the lazy dog here now")
        self._w("b", "the quick brown fox jumps over the lazy dog here today")
        self._w("rule", "STANDING RULE: always verify before claiming done")
        props = curate.proposals(curate.report(self.d, today=TODAY))
        self.assertTrue(any("merge near-duplicates" in p for p in props))
        self.assertTrue(any("promote to charter" in p for p in props))


if __name__ == "__main__":
    unittest.main()
