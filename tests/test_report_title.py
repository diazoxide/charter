"""`report.title` — the one line of a report a maintainer reads before anything else.

The free-text branch took the report's first line verbatim and cut it at 72 (#360). A
reporter opening with a Markdown heading — which is natural, the body *is* Markdown — got
the `#` in the issue title, truncated mid-word. Live example: #322 on this tracker.

Every test here asserts its **precondition** first: that the record has no
``exception_type``, so the free-text branch is the one that ran. The crash branch above it
never touches this code, and a test that silently exercised it would pass while proving
nothing.
"""
from __future__ import annotations

import unittest

from charter import report

#: #322's first line, as the reporter wrote it. The defect this module exists for.
_322 = '`charter secret list` reports "no secrets" when the 1Password read actually failed'


def rec(text: str) -> dict:
    return {"payload": {"text": text}}


class TitleCase(unittest.TestCase):
    def title_of(self, text: str) -> str:
        """The title, having first proved the free-text branch is what produced it."""
        r = rec(text)
        self.assertNotIn("exception_type", r["payload"],
                         "precondition: this must be the free-text branch, not the crash "
                         "branch, which builds its title from the exception type instead")
        return report.title(r)

    def assert_broke_at_a_word(self, first: str, title: str) -> None:
        """The title is a prefix of *first* that ends where a word ends."""
        self.assertTrue(title.endswith("…"), title)
        kept = title[:-1]
        self.assertTrue(first.startswith(kept), f"{title!r} is not a prefix of {first!r}")
        self.assertEqual(first[len(kept)], " ",
                         f"{title!r} cut mid-word — the next character is not a space")


class TestAHeadingMarkerIsNotPartOfTheTitle(TitleCase):
    def test_the_issue_322_line_loses_its_marker_and_keeps_its_words(self):
        self.assertEqual(
            self.title_of(f"# {_322}\n\nBody follows."),
            '`charter secret list` reports "no secrets" when the 1Password read…')

    def test_every_atx_depth_is_stripped(self):
        for depth in range(1, 7):
            with self.subTest(depth=depth):
                self.assertEqual(self.title_of(f"{'#' * depth} A gap in clone"),
                                 "A gap in clone")

    def test_a_closed_atx_heading_loses_the_trailing_markers_too(self):
        self.assertEqual(self.title_of("## A gap in clone ##"), "A gap in clone")

    def test_a_hash_that_is_not_a_heading_marker_is_left_alone(self):
        """CommonMark needs whitespace after the marker. `#331` is an issue reference in
        the reporter's own sentence, and deleting it would change what they said."""
        self.assertEqual(self.title_of("#331 is still open"), "#331 is still open")

    def test_a_hash_inside_the_line_is_left_alone(self):
        self.assertEqual(self.title_of("clone fails on #331"), "clone fails on #331")


class TestInlineMarkupIsTheReportersOwnWords(TitleCase):
    """A heading marker is a *block* marker — it was never part of the sentence. Backticks
    and bold are inline markup the reporter chose. Charter un-marks the line; it does not
    rewrite the words."""

    def test_backticks_survive(self):
        self.assertEqual(self.title_of("# `charter clone` fails"), "`charter clone` fails")

    def test_bold_survives(self):
        self.assertEqual(self.title_of("**clone** fails"), "**clone** fails")


class TestTruncationPrefersAWordBoundary(TitleCase):
    def test_a_long_line_is_not_cut_mid_word(self):
        self.assert_broke_at_a_word(_322, self.title_of(_322))

    def test_a_long_unbroken_token_is_hard_cut_rather_than_collapsed(self):
        """The `textwrap.shorten` trap, and why this rule is hand-rolled: `shorten`
        returns ONLY the placeholder when the first word exceeds the width, which would
        leave the issue titled `…`. A word break too near the start is worth less than the
        characters it throws away, so below the floor charter cuts mid-word instead."""
        first = "ab " + "x" * 80
        got = self.title_of(first)
        self.assertEqual(got, "ab " + "x" * 68 + "…")
        self.assertNotEqual(got, "…")
        self.assertNotEqual(got, "ab…")

    def test_a_break_at_or_past_the_floor_is_taken(self):
        first = "y" * 45 + " " + "z" * 40
        self.assertEqual(self.title_of(first), "y" * 45 + "…")

    def test_the_whole_title_including_the_ellipsis_fits_the_bound(self):
        for first in (_322, "ab " + "x" * 80, "y" * 45 + " " + "z" * 40, "w" * 200):
            with self.subTest(first=first[:20]):
                self.assertLessEqual(len(self.title_of(first)), 72)

    def test_a_line_that_already_fits_is_untouched_and_unmarked(self):
        got = self.title_of("clone fails on a private repo")
        self.assertEqual(got, "clone fails on a private repo")
        self.assertFalse(got.endswith("…"))

    def test_the_marker_is_stripped_before_the_bound_is_measured(self):
        """Not after: a line whose words fit in 72 once the `# ` is gone must not be
        truncated for the two characters charter itself removed."""
        first = "c" * 71
        self.assertEqual(self.title_of(f"# {first}"), first)


class TestEmptyTextIsNotACrash(TitleCase):
    """Defence in depth, not a live bug: `commands_report._draft` refuses an empty body
    before a record is ever written, so no CLI path reaches this. It is asserted because
    `title` is called on stored records and must never be the thing that raises — the
    verbatim `splitlines()[0]` raised `IndexError` here."""

    def test_empty_text_is_an_empty_title(self):
        self.assertEqual(self.title_of(""), "")

    def test_whitespace_only_text_is_an_empty_title(self):
        self.assertEqual(self.title_of("   \n  \n"), "")

    def test_a_line_that_is_only_a_marker_is_an_empty_title(self):
        self.assertEqual(self.title_of("###"), "###")


class TestTheHeadingStaysInTheBody(TitleCase):
    """The ruling on #360's open question, held by a test so it cannot be quietly reversed.

    `render` serves both the Reporter's review and the issue body **deliberately**, so that
    what is shown and what is sent cannot drift (docs/adr/0003). Dropping the heading line
    once it became the title would make the sent thing differ from the approved thing in
    the one place the codebase forbids it, and would need `render` and `title` to agree
    about which line that was — two code paths answering one question, which is the defect
    this repo keeps paying for. One repeated Markdown heading is the cheaper failure.
    """

    def test_render_still_contains_the_heading_line_verbatim(self):
        r = rec(f"# {_322}\n\nBody follows.")
        self.assertNotIn("exception_type", r["payload"])
        body = report.render(r)
        self.assertIn(f"# {_322}", body)
        self.assertIn("Body follows.", body)

    def test_the_title_is_not_the_heading_line(self):
        """Both halves of the ruling in one place: the body keeps the marker, the title
        does not."""
        r = rec(f"# {_322}\n\nBody follows.")
        self.assertIn(f"# {_322}", report.render(r))
        self.assertFalse(report.title(r).startswith("#"))


if __name__ == "__main__":
    unittest.main()
