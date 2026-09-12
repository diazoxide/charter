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


class TheTitleAWriteDerivesIsAlsoWhatAReaderComputes(MemstoreCase):
    """`title_of` is `write`'s own rule, lifted out so `todos.duplicate_of` can ask what a
    stored title WILL be instead of respelling "the first line, stripped, capped" — two
    spellings of it are how the writer and the reader come to disagree about which todos
    are the same one."""

    def test_a_title_is_the_first_line_and_nothing_after_it(self):
        self.assertEqual(memstore.title_of("Fix the widget\nIt breaks on resize.\n"),
                         "Fix the widget")

    def test_the_blank_lines_and_the_padding_around_it_are_not_part_of_it(self):
        self.assertEqual(memstore.title_of("\n\n  Fix the widget  \nmore\n"),
                         "Fix the widget")

    def test_a_body_with_nothing_in_it_has_no_title(self):
        self.assertEqual(memstore.title_of("  \n\n "), "")

    def test_a_long_first_line_is_capped_at_seventy_two_characters(self):
        """**The number is spelled out here rather than read off the constant.** A test whose
        expectation is computed from the value it is checking moves with that value and pins
        nothing: widening `TITLE_MAX` to 4096, and retuning it by one, both stayed green
        against `"x" * memstore.TITLE_MAX`.

        72 is what a `# ` heading, an index row and a `slug` filename carry, and it is the
        same cap on both sides of `todos.duplicate_of`'s comparison — a reader that stopped
        one character later would compare 73 characters against the 72 `write` stored."""
        self.assertEqual(memstore.TITLE_MAX, 72)
        self.assertEqual(memstore.title_of("x" * 100), "x" * 72)

    def test_a_title_handed_to_write_is_capped_and_stripped_the_same_way(self):
        """A caller's own title goes through the same two rules as a derived one, so a
        `# ` heading and an index row look the same whichever way the title arrived."""
        for given, stored_as in (("y" * 100, "y" * 72), ("  Padded  ", "Padded")):
            with self.subTest(title=given[:12]):
                p = memstore.write(self.d, "a body", given)
                [(_p, stored, _t)] = [e for e in memstore.entries(self.d) if e[0] == p]
                self.assertEqual(stored, stored_as)
                # The INDEX row too, and it is the reading that can see the strip:
                # `entries` takes the title off the `# ` heading and strips it again on the
                # way out, so padding left in by the writer is invisible there and visible
                # here — in the file a person reads.
                self.assertIn(f"- [{stored_as}]({p.name})\n",
                              memstore.index_path(self.d).read_text())
                p.unlink()

    def test_what_a_write_stores_is_what_title_of_answers(self):
        for text in ("Fix the widget\nIt breaks.\n", "  padded  \nmore\n",
                     "y" * (memstore.TITLE_MAX + 5)):
            with self.subTest(text=text[:20]):
                p = memstore.write(self.d, text)
                [(_p, stored, _t)] = [e for e in memstore.entries(self.d) if e[0] == p]
                self.assertEqual(stored, memstore.title_of(text))
                p.unlink()


class TheIndexIsExactlyTheLinesThatSurvive(MemstoreCase):
    """Two one-line conditionals in `memstore`, neither of which had a case.

    Both sit on lines #505 rewrote (`write_text` → `config.write_for`), so the sweep
    charged them to that change and found them unpinned. The logic predates it; the cases
    do not, and "the line was already like that" is not a reason to leave a guard nobody
    can delete safely.
    """

    def test_the_header_gets_exactly_one_trailing_newline(self) -> None:
        """`ensure_index` adds one only when the caller did not. Collapsed either way the
        index starts with a doubled blank line or with none, and every later append lands
        against a header that is not the shape `index_append` writes after."""
        for label, header in (("caller ended it", "# Memory Index\n"),
                              ("caller did not", "# Memory Index")):
            with self.subTest(label):
                d = persona.memory_dir(f"hdr-{len(header)}")
                d.mkdir(parents=True, exist_ok=True)
                idx = memstore.ensure_index(d, header)
                self.assertEqual(idx.read_text(), "# Memory Index\n",
                                 "the header did not come out with exactly one newline")

    def test_dropping_the_only_line_leaves_an_empty_index_not_a_blank_one(self) -> None:
        """`_drop_index_line` appends a trailing newline only when something is left.

        Reaching "nothing is left" takes an index with no header — a hand-written one, or
        one an older charter wrote — because otherwise the header lines survive every drop
        and the conditional never takes its other branch. That is also what makes it worth
        a case: the branch is only ever exercised by a file charter did not write, which is
        the class of input nobody runs by hand. A lone ``"\n"`` is a file with one blank
        line in it, and `index_drift` reads a blank line back as an entry that does not
        resolve.
        """
        memstore.write(self.d, "only fact", "Solo")
        idx = memstore.index_path(self.d)
        idx.write_text("- [Solo](solo.md)\n")      # no header: the drop empties it
        memstore.forget(self.d, "solo")
        self.assertEqual(idx.read_text(), "",
                         "the index kept a blank line where its last entry had been")

    def test_dropping_one_of_several_keeps_the_rest_newline_terminated(self) -> None:
        """The other side of the same conditional — a file that does not end in a newline
        is one every later append runs onto the end of."""
        memstore.write(self.d, "first", "Alpha")
        memstore.write(self.d, "second", "Beta")
        memstore.forget(self.d, "alpha")
        text = memstore.index_path(self.d).read_text()
        self.assertNotIn("(alpha.md)", text)
        self.assertIn("(beta.md)", text)
        self.assertTrue(text.endswith("\n"), f"index does not end in a newline: {text!r}")


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
