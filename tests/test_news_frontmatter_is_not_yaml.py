"""#902: `docs/news/` frontmatter looks exactly like YAML and is not, and the quotes an
author adds because of that get published.

It opens with ``---`` and closes with ``---``. Everything a reader knows about frontmatter
says the next thing is YAML. `persona.parse` is flat ``key: value`` — it takes everything
after the first colon verbatim and unquotes nothing — so::

    headline: '`charter workspace reinit --all` counts repairs and workspaces apart'

renders into the published GitHub Release as::

    ### '`charter workspace reinit --all` counts repairs and workspaces apart'

**The author is not being careless.** A headline in this repo usually *starts with a
backtick*, and a backtick is a reserved indicator in real YAML: anyone who believes this is
YAML is correct to quote it. The format punishes knowing YAML, and it punishes quietly —
the file is well-formed, `charter news --for` exits 0, and the only person who ever sees
the quotes is a reader of the published notes.

**Which is how six of them shipped.** 0.56.0 published six quoted headlines and nobody
noticed until 0.57.0 was being cut, when one entry was caught by hand, by someone who
happened to look. Neither this suite nor the release gate had an opinion.

Refusing rather than unquoting, and the module's own reasoning is what decided it.
`news._flag` already answers a *different* YAML habit — a value indented onto the
continuation line — and it answers it with a sentence naming the habit rather than by
learning to read it, because "which spellings do we accept" is a guard against a spelling
and the property is whether the value was understood. `news._KNOWN_FIELDS` refuses to fold
keys for the same reason, and `persona.misspelled_key` says in as many words that charter
never guesses which field an author meant. Teaching the reader to strip one pair of quotes
would honour exactly one YAML habit while going on dropping continuation lines, anchors and
backslash escapes — half a spec, with nothing to tell an author which half — and would make
a headline that really does end in a quote unwritable *silently*. Refusing says so.

Three surfaces, and the earliest is the one that matters. This file asks the whole
committed corpus on every PR, which is before merge. `charter news --for` asks the version
being cut, which is the release gate that was silent for 0.56.0. `news.quoted_values` is
where the answer lives, so the two ask the same question of the same parser.

**And the six are reported, not rewritten.** They are the record of a Release GitHub
already holds; editing them would fork the repo's copy from the published notes, so that
the thing you read in `docs/news/` and the thing readers of the Release see would no longer
be the same document. :data:`_SHIPPED_QUOTED` pins them by name and
:class:`WhatShippedIsReportedRatherThanRewritten` asserts both directions of it.
"""

from __future__ import annotations

import io
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, news

_V = "0.60.0"

#: The six headlines 0.56.0 published with their quotes, named rather than described.
#:
#: Frozen on purpose, and in the test rather than in `charter/news.py`. In the module it
#: would be an exemption — `quoted_values` would stop naming them, and the defect would
#: become invisible in the surface built to make it visible. Here it is a *pin*: the
#: function still reports all six to anyone who asks, and this file says that reporting
#: exactly these six is the expected state of the tree.
#:
#: Both directions have teeth. A seventh quoted entry makes this set too small and the
#: assertion names the new file — which is the check #902 asked for. Correcting one of the
#: six makes it too big, and that is deliberate: these files are what the 0.56.0 Release
#: says, and a repo whose copy of a stamped release's notes differs from the notes
#: themselves has two answers to one question. Correcting them is a decision about the
#: published Release, not about this tree, and it edits this set in the same commit.
_SHIPPED_QUOTED = frozenset({
    "0.56.0-a-green-doctor-row-keeps-nothing-back.md",
    "0.56.0-a-release-body-spends-what-github-allows.md",
    "0.56.0-charter-save-refuses-from-a-workspace-clone-that-is-a-plane.md",
    "0.56.0-doctor-answers-for-the-directory-it-is-running-in.md",
    "0.56.0-the-plane-init-creates-is-one-init-can-see.md",
    "0.56.0-the-session-root-row-stops-contradicting-the-row-below-it.md",
})


def _entry(headline: str, version: str = _V, **fields) -> str:
    lines = [f"version: {version}", f"headline: {headline}"]
    lines += [f"{k}: {v}" for k, v in fields.items()]
    return "---\n" + "\n".join(lines) + "\n---\n\nbody text\n"


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so a test never depends on what shipped —
    the same isolation `tests/test_news_ordering.py` establishes, for the same reason."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, text: str, name: str = f"{_V}-z-important.md") -> None:
        (self.dir / name).write_text(text)

    def said(self, text: str) -> list[str]:
        self.write(text)
        return news.quoted_values(news.all())


class AMatchedPairIsFound(unittest.TestCase):
    """`news.quoted` on values, with nothing else in the way.

    The rule is "starts and ends with the same ``'`` or ``\"``" and nothing narrower. Every
    case below is either one of the shapes the corpus actually holds or one of the shapes a
    narrower rule would have got wrong.
    """

    def test_the_shape_the_issue_was_filed_about(self):
        self.assertEqual(news.quoted("'`charter workspace reinit --all` counts'"), "'")

    def test_double_quotes_too(self):
        """The issue is about the six single-quoted ones because those are what shipped.
        A YAML author reaches for either, and a rule that knew only one would leave the
        other exactly as silent as before."""
        self.assertEqual(news.quoted('"a headline"'), '"')

    def test_an_inner_quote_of_the_same_kind_does_not_excuse_the_pair(self):
        """The refinement that looks more precise and is wrong. YAML writes an inner ``'``
        as ``''`` inside a single-quoted scalar, so "the same quote appears inside, this is
        prose rather than a wrapper" is a rule with a real YAML pedigree — and one of the
        six 0.56.0 headlines is exactly that shape:

            '`charter doctor`''s `session root` row stops telling you…'

        The narrower rule drops it. A guard that misses a sixth of the corpus it was
        written for, while reading as though it understood the format better, is the
        trade this test exists to refuse.
        """
        self.assertEqual(news.quoted("'`charter doctor`''s `session root` row'"), "'")

    def test_a_backtick_is_not_a_quote(self):
        """The commonest headline in this repo is a code span, and a headline that IS one
        opens and closes with a backtick. It renders as markdown in the Release body
        rather than as itself, which is the whole difference: the pair disappears into
        formatting instead of into the heading."""
        self.assertIsNone(news.quoted("`charter workspace reinit --all`"))

    def test_the_dominant_convention_is_left_alone(self):
        """251 of 257 entries. A check that had an opinion about these would be a check
        nobody could keep."""
        self.assertIsNone(news.quoted(
            "`charter save` from a workspace clone refuses, instead of committing"))

    def test_a_quote_at_one_end_only(self):
        """Three shipped headlines start or end with a `"` and none does both — a quoted
        phrase at the start of a sentence, or at the end of one. Flagging those would fail
        CI on prose that renders exactly as written."""
        self.assertIsNone(news.quoted('charter trace answers "which command got the token"'))
        self.assertIsNone(news.quoted('"gathering" is what the frame says now'))

    def test_two_different_quote_characters_are_not_a_pair(self):
        self.assertIsNone(news.quoted("\"mixed'"))

    def test_one_quote_is_not_two(self):
        """``len >= 2``, or a value of a single ``'`` starts and ends with the same
        character and reads as its own wrapper."""
        self.assertIsNone(news.quoted("'"))
        self.assertIsNone(news.quoted('"'))

    def test_nothing_at_all(self):
        self.assertIsNone(news.quoted(""))
        self.assertIsNone(news.quoted("   "))

    def test_surrounding_whitespace_does_not_hide_the_pair(self):
        """`persona.parse` strips, so this is belt to braces — but `quoted` is public and
        the next caller may not hand it a parsed value."""
        self.assertEqual(news.quoted("  'a headline'  "), "'")


class EveryTextFieldIsAsked(NewsDir):
    """Not the headline alone.

    `headline:` is what shipped wrong and what the issue is named for, so it is also the
    field a fix written from the issue alone would check by itself. `check:` and `adopt:`
    are read the same way by the same parser: a quoted `check:` is a probe naming a
    command that does not exist, and a quoted `adopt:` prints `adopt: charter 'workspace
    reinit'` into the suggestion a reader is meant to paste.

    The set is derived from `news._KNOWN_FIELDS` rather than listed, so a seventh field
    is asked about without anyone remembering to add it here.
    """

    def test_the_headline(self):
        why, = self.said(_entry("'important'"))
        self.assertIn("headline", why)

    def test_the_probe(self):
        why, = self.said(_entry("important", check="'workspace reinit'"))
        self.assertIn("check", why)

    def test_the_adopt_line(self):
        why, = self.said(_entry("important", adopt="'workspace reinit'"))
        self.assertIn("adopt", why)

    def test_the_version(self):
        """A quoted `version:` is the loudest of them — the entry lands in a release
        called `'0.60.0'` that no other view of the tree agrees exists."""
        why, = self.said(_entry("important", version=f"'{_V}'"))
        self.assertIn("version", why)

    def test_the_ordering_fields_are_not_asked_twice(self):
        """`lead: 'true'` is already a value `_flag` cannot read, and `entry_errors`
        already names it with the vocabulary of ordering fields. Reporting it here as well
        would send an author looking for two mistakes where they made one."""
        self.assertEqual(self.said(_entry("important", security="'true'")), [])
        self.assertEqual(
            [f for f in news._TEXT_FIELDS if f in news._ORDERING_FIELDS], [])

    def test_every_text_field_is_a_field_of_an_entry(self):
        """`quoted_values` reaches them with `getattr`, so a name in `_KNOWN_FIELDS` that
        is not an `Entry` field would be an `AttributeError` at the release gate — the one
        call where an exception is worst."""
        for field in news._TEXT_FIELDS:
            with self.subTest(field=field):
                self.assertIn(field, news.Entry._fields)

    def test_an_unquoted_entry_says_nothing(self):
        """The counterfactual. Without it every assertion above passes on a function that
        always reports."""
        self.assertEqual(self.said(_entry("important", check="doctor")), [])


class TheSentenceSaysWhatToDo(NewsDir):
    """A refusal an author cannot act on is a failed CI run and nothing else."""

    def _why(self, headline: str) -> str:
        why, = self.said(_entry(headline))
        return why

    def test_it_names_the_file(self):
        self.assertIn("0.60.0-z-important.md", self._why("'important'"))

    def test_and_the_field(self):
        self.assertIn("`headline:`", self._why("'important'"))

    def test_and_the_character_the_author_typed(self):
        """Not "a quote". The author typed one of two characters and the sentence shows
        which, so the line is greppable and the fix is unambiguous."""
        self.assertIn("'", self._why("'important'"))
        self.assertIn('"', self._why('"important"'))

    def test_and_says_the_format_is_not_yaml(self):
        """The sentence has to correct the belief, not just the file. An author told only
        "remove the quotes" learns a rule; an author told the frontmatter is flat `key:
        value` stops reaching for the next YAML habit too."""
        why = self._why("'important'")
        self.assertIn("not YAML", why)
        self.assertIn("key: value", why)

    def test_and_that_a_backtick_needs_no_quoting(self):
        """The specific thing the author was working around. A headline starting with a
        backtick is why the quotes went on, so a sentence that does not answer that sends
        them back to the same fix."""
        self.assertIn("backtick", self._why("'`charter doctor` does a thing'"))

    def test_and_names_the_one_value_it_cannot_represent(self):
        """The cost of refusing rather than stripping, said out loud rather than
        discovered. A headline that must really begin and end with a quote has to be
        reworded, and the author is told that instead of being left to guess whether they
        have found a bug."""
        self.assertIn("reworded", self._why("'important'"))

    def test_it_is_one_line(self):
        """`contain.sentence`, like every other sentence this module assembles: the
        filename is committed data and a `\\n` in it would forge a second line of
        charter's own report (#502)."""
        self.assertEqual(len(self._why("'important'").splitlines()), 1)


class TheReleaseGateRefusesIt(NewsDir):
    """`charter news --for <version>` is `release.yml`'s pre-publish guard and the same
    call whose stdout `announce` pipes into `gh release create`. It was the last place
    0.56.0 could have been stopped, and it exited 0 six times."""

    def _for(self, version: str = _V) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=version, pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news(args)
        return rc, out.getvalue(), err.getvalue()

    def test_a_quoted_headline_stops_the_release(self):
        self.write(_entry("'important'"))
        rc, _out, err = self._for()
        self.assertEqual(rc, 1)
        self.assertIn("0.60.0-z-important.md", err)

    def test_and_the_notes_are_not_printed(self):
        """stdout is what `announce` redirects into `--notes-file`. A gate that refused
        and printed the body anyway would have the exit code and the artefact disagree,
        and only one of them is read by a human."""
        self.write(_entry("'important'"))
        _rc, out, _err = self._for()
        self.assertNotIn("### ", out)

    def test_an_unquoted_release_publishes(self):
        """The counterfactual, and the one this gate cannot afford to get wrong: it runs
        on every release, and a check that refused them all would be found by the first
        one rather than by a test."""
        self.write(_entry("important"))
        rc, out, _err = self._for()
        self.assertEqual(rc, 0)
        self.assertIn("### important", out)


class TheRangeViewSaysNothingAboutIt(NewsDir):
    """A reader catching up is not an author, and the six cannot be corrected without
    forking the repo's copy of 0.56.0's notes from the Release published from them.

    So this deliberately does NOT go through `entry_errors`, which `cmd_news` prints as a
    warning in the range view: every user whose upgrade spans 0.56.0 would be warned, on
    every run, forever, about six files nobody is going to change. The consequence belongs
    at the release gate, where a fix is still possible, and in the suite, where it is
    cheap.
    """

    def test_a_reader_catching_up_is_not_warned(self):
        self.write(_entry("'important'"))
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=None, pending=False, since="0.59.0", until=_V)
        with mock.patch.object(news, "probe", return_value=(news.INFORMATIONAL, "")), \
             redirect_stdout(out), redirect_stderr(err):
            self.assertEqual(commands.cmd_news(args), 0)
        self.assertIn("important", out.getvalue())
        self.assertNotIn("unquote", err.getvalue())

    def test_and_entry_errors_keeps_its_own_vocabulary(self):
        """The other half: a quoted value is not something charter *cannot honour*. It
        reads it, renders it and ships exactly the bytes the file holds. Folding the two
        reports together would make that sentence false for a sixth of what it covers."""
        self.write(_entry("'important'"))
        self.assertEqual(news.entry_errors(news.all()), [])


class WhatShippedIsReportedRatherThanRewritten(unittest.TestCase):
    """Over the real `docs/news`, not a fixture.

    This is the check that would have caught 0.56.0 on the PR that wrote the entries, and
    it is the one that keeps the seventh from happening. It is also what says the six are
    *reported* without being *changed*: `news.quoted_values` names all six every time it is
    asked, and this asserts that the set it names is exactly the six that shipped.
    """

    def setUp(self):
        self.entries = news.all()
        self.assertTrue(self.entries, "no news entries found — this test proves nothing")

    def _flagged(self) -> set[str]:
        return {e.path.name for e in self.entries
                if any(news.quoted(getattr(e, f)) for f in news._TEXT_FIELDS)}

    def test_no_entry_carries_a_quoted_value_except_the_six_that_shipped(self):
        self.assertEqual(
            self._flagged(), set(_SHIPPED_QUOTED),
            "an entry in docs/news wraps a frontmatter value in quotes. This frontmatter "
            "is flat `key: value`, not YAML — nothing unquotes it, so both quotes are part "
            "of the value and both render into the published GitHub Release heading. "
            "Remove them; a backtick needs no quoting here. If a file went the other way "
            "and one of the six 0.56.0 entries was corrected, that is a change to what a "
            "stamped release's notes say and it belongs in the same commit as the edit to "
            "`_SHIPPED_QUOTED` and to the published Release.")

    def test_the_six_are_reported_by_name(self):
        """Not merely counted. The release engineer's memory note says to grep for them by
        hand before tagging; the point of putting the question in code is that the answer
        is specific enough to replace that grep."""
        named = " ".join(news.quoted_values(self.entries))
        for name in sorted(_SHIPPED_QUOTED):
            with self.subTest(entry=name):
                self.assertIn(name, named)

    def test_and_they_still_say_on_disk_what_the_release_says(self):
        """The other half of "reported, not rewritten", asserted against the bytes. The
        published 0.56.0 Release has those quotes in its headings; a repo whose copy
        quietly lost them would leave two documents claiming to be the same notes."""
        for name in sorted(_SHIPPED_QUOTED):
            path = next(e.path for e in self.entries if e.path.name == name)
            with self.subTest(entry=name):
                self.assertIsNotNone(
                    news.quoted(next(e.headline for e in self.entries
                                     if e.path.name == name)),
                    f"{name} no longer carries the quotes 0.56.0 published")
                self.assertTrue(path.is_file())

    def test_and_the_version_being_prepared_would_publish(self):
        """The staged entries, through the real gate. `--for unreleased` is not a release,
        but it renders through the same `quoted_values` call `--for 0.58.0` will make
        after `news stamp` — so a staged entry that would stop the next release stops this
        PR instead."""
        staged = news.for_version(news.UNRELEASED)
        self.assertEqual(news.quoted_values(staged), [])

    def test_and_0_56_0_is_the_release_that_would_not_publish_today(self):
        """Said as a fact rather than left implicit. The gate now refuses the version that
        shipped these, which is the proof that it would have refused them in September —
        and the reason the fix is a report rather than an edit: charter cannot un-publish
        a Release, so the honest thing is to keep saying so.
        """
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version="0.56.0", pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            self.assertEqual(commands.cmd_news(args), 1)
        self.assertIn("unquote", err.getvalue())


if __name__ == "__main__":
    unittest.main()
