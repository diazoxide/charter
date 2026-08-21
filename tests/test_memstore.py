"""The shared per-file memory engine: one file per fact + MEMORY.md index, optional
timestamp-prefixed filenames for chronological ordering, keyword search, near-dup
detection, and forget-by-slug (matching the timestamp prefix too).

**The store lives inside a plane, and that is not incidental.** These cases used to run
against a bare `mkdtemp`, which no production caller ever produces — every one of them
derives its directory from `persona.memory_dir`, `workspace.memory_dir`, `todos_dir` or
`refs_dir`. Since #336 `memstore.files` refuses a directory that resolves outside the
plane's data (a linked `memory/` is how a read of memory becomes a read of a vault), so a
store in an unrelated temp directory now lists nothing. Keep the fixture inside
`config.PERSONAS_DIR`: reverting it to `mkdtemp` makes every case here fail on an empty
listing, which reads like a search bug and is not one.
"""

from __future__ import annotations

import datetime
import unittest

from charter import memstore, persona
from tests._isolation import PersonaIso


class MemstoreCase(PersonaIso):
    def setUp(self):
        super().setUp()
        self.d = persona.memory_dir("memtest")
        self.d.mkdir(parents=True, exist_ok=True)

    def _stamp(self, s):
        return datetime.datetime.strptime(s, "%Y-%m-%d %H:%M:%S")

    def test_write_slug_only(self):
        p = memstore.write(self.d, "a cluster fact", "Cluster")
        self.assertEqual(p.name, "cluster.md")
        self.assertIn("# Cluster", p.read_text())
        self.assertIn("[Cluster](cluster.md)", memstore.index_path(self.d).read_text())

    def test_write_timestamped_prefix_orders(self):
        a = memstore.write(self.d, "first", "aaa", timestamped=True,
                           stamp=self._stamp("2026-07-22 09:00:00"))
        b = memstore.write(self.d, "second", "bbb", timestamped=True,
                           stamp=self._stamp("2026-07-22 10:00:00"))
        self.assertTrue(a.name.startswith("20260722-090000-"))
        self.assertTrue(b.name.startswith("20260722-100000-"))
        # sorted() on the store lists chronologically because the prefix sorts
        self.assertEqual([p.name for p in memstore.files(self.d)], [a.name, b.name])

    def test_dedup_same_title(self):
        a = memstore.write(self.d, "one", "same")
        b = memstore.write(self.d, "two", "same")
        self.assertNotEqual(a.name, b.name)
        self.assertEqual(len(memstore.files(self.d)), 2)

    def test_no_index_when_disabled(self):
        memstore.write(self.d, "x", "t", index=False)
        self.assertFalse(memstore.index_path(self.d).exists())

    def test_empty_rejected(self):
        with self.assertRaises(ValueError):
            memstore.write(self.d, "   ")

    def test_search_ranks_title_higher(self):
        memstore.write(self.d, "mentions keycloak once in body", "unrelated")
        memstore.write(self.d, "body", "keycloak token")
        res = memstore.search([self.d], "keycloak")
        self.assertEqual(res[0][1], "keycloak token")   # title hit ranks first

    def test_search_empty_query(self):
        memstore.write(self.d, "x", "t")
        self.assertEqual(memstore.search([self.d], "   "), [])

    def test_duplicates_flags_overlap(self):
        memstore.write(self.d, "the quick brown fox jumps over lazy dogs", "a")
        memstore.write(self.d, "the quick brown fox jumps over lazy dogs again", "b")
        dupes = memstore.duplicates([self.d], threshold=0.5)
        self.assertTrue(dupes and dupes[0][0] >= 0.5)

    def test_forget_by_slug_and_by_prefixed_name(self):
        memstore.write(self.d, "x", "plain")
        memstore.write(self.d, "y", "stamped", timestamped=True,
                       stamp=self._stamp("2026-07-22 12:00:00"))
        self.assertTrue(memstore.forget(self.d, "plain"))            # slug → plain.md
        self.assertTrue(memstore.forget(self.d, "stamped"))         # slug → <ts>-stamped.md
        self.assertEqual(memstore.files(self.d), [])
        idx = memstore.index_path(self.d).read_text()
        self.assertNotIn("plain.md", idx)
        self.assertNotIn("stamped.md", idx)

    def test_forget_missing(self):
        self.assertFalse(memstore.forget(self.d, "nope"))


class SearchFindsShortAndDistinctiveTerms(PersonaIso):
    """`_terms` discarded every token of two characters or fewer.

    So `recall "S3"`, `"CI"`, `"db"`, `"PR"`, `"TZ"`, `"v2"` each returned a confident
    "No memories match" against a corpus that contained them. That matters more than it
    looks: the session briefing lists ten titles and tells the agent to *search* for the
    rest, so search is the only route to everything else, and a false negative there is
    indistinguishable from the fact not existing.
    """

    def setUp(self) -> None:
        super().setUp()
        self.d = persona.memory_dir("memsearch")
        self.d.mkdir(parents=True, exist_ok=True)

    def _write(self, name: str, title: str, body: str = "") -> None:
        (self.d / f"{name}.md").write_text(f"# {title}\n\n{body}\n")

    def test_a_two_character_term_is_searchable(self):
        self._write("a", "artifacts live in S3", "the bucket is versioned")
        self._write("b", "unrelated note", "nothing to see")
        hits = memstore.search([self.d], "S3")
        self.assertEqual([t for _p, t, _s in hits], ["artifacts live in S3"])

    def test_the_other_short_terms_the_report_named(self):
        self._write("ci", "CI runs on push", "")
        self._write("db", "db migrations are manual", "")
        self._write("pr", "PR titles carry the issue id", "")
        for q, expected in (("CI", "CI runs on push"),
                            ("db", "db migrations are manual"),
                            ("PR", "PR titles carry the issue id")):
            with self.subTest(q=q):
                hits = memstore.search([self.d], q)
                self.assertTrue(hits, f"{q!r} found nothing")
                self.assertEqual(hits[0][1], expected)

    def test_a_single_character_is_still_dropped(self):
        """It matches nearly everything and ranks nothing."""
        self.assertEqual(memstore._terms("a x"), [])

    def test_stopwords_cannot_outvote_the_word_that_mattered(self):
        """Scoring is raw count with no IDF, so before this a long memory full of "for"
        could beat a single exact hit on the term actually searched for."""
        self._write("noise", "planning notes", "for " * 60)
        self._write("real", "the kubeconfig lives in the devops vault", "")
        hits = memstore.search([self.d], "for kubeconfig")
        self.assertEqual(hits[0][1], "the kubeconfig lives in the devops vault")

    def test_terms_match_on_a_word_boundary(self):
        """"version" no longer scores inside "conversion" — the cheap half of stemming."""
        self._write("con", "conversion funnel metrics", "")
        self._write("ver", "version pinning policy", "")
        hits = memstore.search([self.d], "version")
        self.assertEqual([t for _p, t, _s in hits], ["version pinning policy"])

    def test_a_prefix_still_matches(self):
        self._write("v", "the schema is versioned per release", "")
        self.assertTrue(memstore.search([self.d], "version"))

    def test_dropped_terms_distinguishes_nothing_matched_from_nothing_searched(self):
        self.assertEqual(memstore._terms("in the of"), [])
        self.assertEqual(set(memstore.dropped_terms("in the of")), {"in", "the", "of"})
        self.assertEqual(memstore.dropped_terms("in the kubeconfig"), ["in", "the"])


if __name__ == "__main__":
    unittest.main()
