"""#502: a news entry owns its text, not the structure of charter's report about it.

`news.entry_errors` said this, in a message built out of two spans::

    f"{e.path.name}: `{field}: {contain.one_line(raw)}` is not a value charter reads …"

The ordering VALUE was contained. The committed FILENAME three inches to its left was
interpolated raw — so an entry named ``0.60.0-a\\nEVIL: charter says nothing is wrong.md``
printed **two** lines where charter emitted one, and the second was the author's sentence
sitting in a CI log in charter's own voice, above the release a human is about to publish.

**A filename is committed data.** Whoever writes the commit chooses it, exactly as whoever
writes it chooses `security:`'s value, and #453 is the same mechanism one surface over: a
value crossing into a format that has structure without being escaped for that structure.
The value was guarded because the value was what that commit was about; the filename was
not judged safe, it was not judged.

**So the property is not "the filename is contained".** It is *every untrusted span in a
report line is contained, including the ones nobody was thinking about* — and the way to
have that is to contain the sentence as it is ASSEMBLED rather than at the spans somebody
enumerated. `news._report` does that, and this module is what says so about spans that
were not enumerated: the `{version}` in the duplicate-`lead:` message is frontmatter, the
`{check}` in a probe's reason is frontmatter, a slug is a filename with its prefix cut off,
and the two filenames in `news stamp`'s SUCCESS line are the same pair its refusal carries.

Every fixture below plants the same payload in a different span and asserts the same two
things: charter's report is the number of lines charter meant to write, and the payload is
still *shown* rather than dropped. Escaped, never swallowed — a report that silently loses
the value it is complaining about is a report about nothing.

The line counts are never hard-coded. Each surface is run twice, once with an ordinary
fixture and once with the hostile one, and the two are compared: a test that pinned "4
lines" would go red on a wording change and green on a forged line inside a longer message.
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

#: A payload with one of every way to end a line, plus one that ends no line but repaints
#: the terminal. Named by what each one DOES rather than by codepoint, because the reason
#: they are all here is that "newline" is four different characters depending on who is
#: reading: `str.splitlines` splits on all of the first four, a YAML 1.1 reader and a
#: pre-ES2019 JavaScript parser split on U+2028, and `\\x1b` is what makes the forged line
#: look like it came from a different program.
_LF = "\n"
_CR = "\r"
_NEL = "\x85"                # NEXT LINE, a line break to `str.splitlines`
_LINE_SEP = " "         # LINE SEPARATOR, a line break to YAML and to `splitlines`
_ESC = "\x1b[31m"            # an ANSI sequence: no line break, a repainted terminal

#: The word a forged line would carry, chosen so a test can tell "escaped and shown" from
#: "dropped" — the two outcomes that look identical if you assert only the line count.
_MARK = "EVIL"

#: The payload as it appears in a span that can hold a line break: a FILENAME, which the
#: filesystem is happy to let hold a ``\\n``, and any field of an `Entry` built directly.
_HOSTILE = f"a{_LF}{_MARK}: charter says nothing is wrong{_CR}{_NEL}{_LINE_SEP}{_ESC}b"

#: The payload as it can appear in a frontmatter VALUE, which is a narrower thing — see
#: :class:`AFrontmatterValueCannotHoldALineBreakInTheFirstPlace`. No line break survives
#: the parser, so what a value can actually carry into a report is the half of the payload
#: that ends no line and still owns the terminal.
_HOSTILE_VALUE = f"a{_ESC}{_MARK}: charter says nothing is wrong‍b"

#: The same shape with nothing dangerous in it, for the run the hostile one is compared to.
_BENIGN = f"a{_MARK} charter says nothing is wrong b"


def _entry(version: str = _V, headline: str = "important", **fields) -> str:
    lines = [f"version: {version}", f"headline: {headline}"]
    lines += [f"{k}: {v}" for k, v in fields.items()]
    return "---\n" + "\n".join(lines) + "\n---\n\nbody text\n"


class OneLine(unittest.TestCase):
    """The two assertions every case in this file makes."""

    def assertOneLine(self, said: str, where: str = "") -> None:
        """*said* occupies exactly one physical line, by every definition of one.

        `splitlines` is the check rather than ``"\\n" not in said`` because the codepoints
        that break a line are not the codepoint an author would think to look for: it
        splits on ``\\r``, ``\\x0b``, ``\\x0c``, ``\\x1c``-``\\x1e``, ``\\x85``, ``\\u2028``
        and ``\\u2029`` as well, and a guard written against ``\\n`` alone passes every one
        of them straight through.
        """
        self.assertEqual(len(said.splitlines()), 1,
                         f"{where or 'this report line'} is {len(said.splitlines())} "
                         f"physical lines: {said!r}")
        self.assertNotIn("\x1b", said,
                         f"{where or 'this report line'} carries a raw escape sequence")

    def assertShown(self, said: str, where: str = "") -> None:
        """The payload is escaped INTO the line, not dropped out of it. A report that
        quietly loses the value it is complaining about names a problem the reader then
        cannot find."""
        self.assertIn(_MARK, said,
                      f"{where or 'this report line'} dropped the value it is about")


class NewsDir(OneLine):
    """Entries read from a throwaway directory. The hostile filenames below really are
    created on disk — a POSIX filename may hold a newline, which is the whole point — so
    they never go anywhere near the repository."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> Path:
        p = self.dir / name
        p.write_text(text)
        return p


class AFrontmatterValueCannotHoldALineBreakInTheFirstPlace(NewsDir):
    """The premise the fixtures above rest on, asserted rather than assumed.

    `persona.parse` splits the block with `str.splitlines()` before it partitions a line on
    its colon, so a line break inside a *value* is not a value that contains a line break —
    it is a second frontmatter line, and usually a second KEY. That is why the file-borne
    payload goes in the FILENAME, which the filesystem is glad to let hold a ``\\n``, and
    the value-borne payload is the half that ends no line.

    It is also why this is a class and not a comment. If `persona.parse` ever grew
    multi-line values, every fixture in this file that plants a value payload would be
    testing a shape that no longer exists — and would keep passing while doing it. This
    goes red instead, and names the reason.

    None of it makes the value guard optional: an `Entry` is also built by `_replace`, by a
    caller in another module, and by whatever reads frontmatter after `persona.parse` is
    changed. `contain` is not a second parser and the parser is not a second `contain`.
    """

    def test_a_line_break_in_a_value_becomes_another_frontmatter_line(self):
        from charter import persona

        meta, _body = persona.parse(f"---\nversion: {_V}\n"
                                    f"headline: a{_LF}{_MARK}: forged\n---\n\nb\n")
        self.assertEqual(meta.get("headline"), "a")
        self.assertIn(_MARK, meta, "the payload did not become its own key after all")

    def test_which_is_why_the_value_payload_ends_no_line(self):
        self.assertEqual(len(_HOSTILE_VALUE.splitlines()), 1)
        self.assertGreater(len(_HOSTILE.splitlines()), 1)

    def test_and_the_filename_payload_really_reaches_the_disk_intact(self):
        """A POSIX filename may contain anything but ``/`` and NUL. If a filesystem under
        this suite ever refused one, every filename case here would be vacuous."""
        p = self.write(f"{_V}-{_HOSTILE}.md", _entry())
        self.assertIn(_LF, p.name)
        entry, = news.all()
        self.assertEqual(entry.path.name, p.name)


class AFilenameCannotWriteASecondLine(NewsDir):
    """#502's own reproduction, at the layer the issue reported it."""

    def _named(self, payload: str) -> list[str]:
        self.write(f"{_V}-{payload}.md", _entry(security="yes"))
        return news.entry_errors(news.all())

    def test_the_reproduction_from_the_issue_is_one_line(self):
        why, = self._named(_HOSTILE)
        self.assertOneLine(why, "the unreadable-value sentence")
        self.assertShown(why)

    def test_and_an_ordinary_filename_still_reads_as_it_did(self):
        """The counterfactual. Containing everything is only a fix if the ordinary case
        comes out unchanged — a guard that mangled every filename would pass the
        assertion above and make every real message harder to act on."""
        why, = self._named("z-important")
        self.assertIn("z-important.md", why)
        self.assertOneLine(why)

    def test_the_hostile_and_ordinary_reports_are_the_same_number_of_lines(self):
        """The property, stated without a hard-coded count: what an entry may not do is
        change how many lines charter's report has."""
        hostile = self._named(_HOSTILE)
        self.setUp()
        ordinary = self._named(_BENIGN)
        self.assertEqual([len(w.splitlines()) for w in hostile],
                         [len(w.splitlines()) for w in ordinary])


class EverySpanInEverySentence(NewsDir):
    """Not the filename alone. Each case below plants the payload in a DIFFERENT span of a
    sentence `news` assembles, and three of them are spans #502 predicted would be missed
    by a fix that wrapped the two it named: the version in the duplicate-`lead:` message,
    the `check:` in a probe's reason, and the second filename in a stamp refusal."""

    def test_the_ordering_value(self):
        self.write(f"{_V}-z.md", _entry())
        e = news.for_version(_V)[0]._replace(bad=(("security", _HOSTILE),))
        why, = news.entry_errors([e])
        self.assertOneLine(why, "the unreadable-value sentence")
        self.assertShown(why)

    def test_the_version_in_the_duplicate_lead_message(self):
        """Frontmatter, and the span the issue named as "the next spelling". Two entries
        have to claim the lead for this sentence to exist at all, and they have to agree
        on the version — so the payload IS the key the claimants were grouped by.

        Planted on the `Entry` rather than in the file, for the reason
        `test_news_ordering.py` gives about the same manoeuvre: the guard has to hold on
        the argument it is handed, not on a parser upstream that happens to filter the
        input today. `version:` reaches this sentence through `for_version` and through
        `all`, and neither of those is `persona.parse`.
        """
        self.write(f"{_V}-a.md", _entry(lead="true"))
        self.write(f"{_V}-z.md", _entry(lead="true"))
        entries = [e._replace(version=_HOSTILE) for e in news.all()]
        self.assertEqual(len(entries), 2, "the fixture did not stage two leads")
        why, = news.entry_errors(entries)
        self.assertOneLine(why, "the duplicate-`lead:` sentence")
        self.assertShown(why)

    def test_the_joined_filenames_in_the_duplicate_lead_message(self):
        """A list of spans, not one span. Each element is contained on its own and the
        comma between them is charter's, so an entry cannot supply the separator either —
        and a fix that contained the JOINED string would clip the third claimant out of a
        sentence whose whole job is to name all of them."""
        for slug in ("a", "z"):
            self.write(f"{_V}-{slug}{_HOSTILE}.md", _entry(lead="true"))
        why, = news.entry_errors(news.all())
        self.assertOneLine(why, "the duplicate-`lead:` sentence")
        self.assertShown(why)

    def test_the_check_a_file_really_can_carry(self):
        """`check:` is frontmatter and the reason is printed as a line of `charter news
        --pending`'s report. `_tokens` refuses a `\\n` as shell syntax, which decides
        whether the probe RUNS and decides nothing about what the report PRINTS — and it
        has no opinion at all about an escape sequence, which is what a file can actually
        deliver here."""
        self.write(f"{_V}-z.md", _entry(check=f"frobnicate{_HOSTILE_VALUE}"))
        entry, = news.released()
        status, why = news.probe(entry)
        self.assertEqual(status, news.UNKNOWN, "the fixture's probe was not refused")
        self.assertOneLine(why, "the probe's reason")
        self.assertShown(why)

    def test_the_check_of_an_entry_handed_to_probe_directly(self):
        """And the line-breaking half, at the layer the sentence is built from. `probe`
        takes an `Entry`; whether one can be spelled in today's frontmatter is a fact
        about `persona.parse`, not about this guard."""
        self.write(f"{_V}-z.md", _entry(check="frobnicate"))
        entry, = news.released()
        _status, why = news.probe(entry._replace(check=f"frobnicate{_HOSTILE}"))
        self.assertOneLine(why, "the probe's reason")
        self.assertShown(why)

    def test_the_unrecognised_key_of_a_frontmatter_line(self):
        """#503's sentence is #502's problem too: the key it quotes back is written by
        the same author, in the same file, and lands in the same report."""
        self.write(f"{_V}-z.md", "---\n"
                                 f"version: {_V}\nheadline: h\n"
                                 f"{_ESC}{_MARK}: true\n---\n\nb\n")
        why, = news.entry_errors(news.all())
        self.assertOneLine(why, "the unrecognised-key sentence")
        self.assertShown(why)

    def test_the_reason_a_file_is_not_an_entry(self):
        """`unreadable()` names files, and a file that is not an entry is the one whose
        name nobody has ever looked at."""
        self.write(f"{_V}-{_HOSTILE}.md", "---\nheadline: h\n---\n\nb\n")
        why, = news.unreadable()
        self.assertOneLine(why, "the not-an-entry sentence")
        self.assertShown(why)


class StampDir(OneLine):
    """A throwaway checkout, so nothing here renames a file that really ships."""

    def setUp(self):
        self.repo = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.repo, ignore_errors=True)
        (self.repo / "pyproject.toml").write_text("[project]\nname = 'x'\n")
        self.dir = self.repo / "docs" / "news"
        self.dir.mkdir(parents=True)
        for attr, value in (("_CHECKOUT", self.dir), ("_PACKAGED", self.repo / "absent")):
            patch = mock.patch.object(news, attr, value)
            patch.start()
            self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> Path:
        p = self.dir / name
        p.write_text(text)
        return p


class AStampSaysWhichFilesInOneLineEach(StampDir):
    """`stamp`'s two refusals are the half of #502 that predates the ordering fields
    entirely — they have been interpolating `src.name` since news entries existed — and
    the SUCCESS line is a third, on the path that runs at every release."""

    def test_a_name_already_taken_is_reported_in_one_line(self):
        self.write(f"{news.UNRELEASED}-{_HOSTILE}.md", _entry(version=news.UNRELEASED))
        self.write(f"{_V}-{_HOSTILE}.md", _entry())
        _renamed, blocked = news.stamp(_V)
        why, = blocked
        self.assertOneLine(why, "the name-already-taken refusal")
        self.assertShown(why)

    def test_an_entry_with_no_version_line_is_reported_in_one_line(self):
        self.write(f"{news.UNRELEASED}-{_HOSTILE}.md", "no frontmatter here\n")
        _renamed, blocked = news.stamp(_V)
        why, = blocked
        self.assertOneLine(why, "the nothing-to-stamp refusal")
        self.assertShown(why)

    def test_a_version_charter_refused_is_reported_in_one_line(self):
        """The value came from argv rather than from a commit, and it is bounded anyway:
        a string charter has just refused is by definition one nothing has vouched for."""
        _renamed, blocked = news.stamp(f"not-a-version{_HOSTILE}")
        why, = blocked
        self.assertOneLine(why, "the not-a-version refusal")
        self.assertShown(why)

    def test_the_success_line_is_one_line_per_entry_renamed(self):
        """The path that always runs. Containing the refusal and not the success would be
        this whole issue in miniature — one spelling of a message guarded and its
        neighbour, three lines away, not."""
        self.write(f"{news.UNRELEASED}-{_HOSTILE}.md", _entry(version=news.UNRELEASED))
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news_stamp(SimpleNamespace(version=_V))
        self.assertEqual(rc, 0, err.getvalue())
        said = out.getvalue() + err.getvalue()
        self.assertEqual(len(said.strip().splitlines()), 1,
                         f"one entry was renamed and charter wrote: {said!r}")
        self.assertShown(said, "the stamp's success line")


class TheCommandsPrintWhatTheModuleAssembled(NewsDir):
    """The report a person actually sees. Each view is rendered twice — once from an
    ordinary fixture, once from a hostile one whose spans are otherwise identical — and
    the two are required to have the same number of lines. That is the property stated
    without a count in it: an entry may fill charter's lines, and may not add one.
    """

    def _stage(self, name_payload: str, value_payload: str) -> None:
        """The filename takes the payload that can end a line; the frontmatter values take
        the one that can survive `persona.parse`. Both fixtures declare the same KEYS, so
        the two runs differ in the text of the report and in nothing else — a payload that
        added a frontmatter key would add a finding, and a count that moved for that
        reason would say nothing about forged lines."""
        self.write(f"{_V}-a{name_payload}.md",
                   _entry(headline=f"h{value_payload}", adopt=f"doctor{value_payload}",
                          security="yes"))
        self.write(f"{_V}-z-plain.md", _entry(headline="plain"))

    def _lines(self, name_payload: str, value_payload: str,
               args: SimpleNamespace) -> tuple[int, str]:
        self.setUp()
        self._stage(name_payload, value_payload)
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(news, "probe", return_value=(news.PENDING, "")), \
             redirect_stdout(out), redirect_stderr(err):
            commands.cmd_news(args)
        said = out.getvalue() + err.getvalue()
        return len(said.splitlines()), said

    def _both(self, args: SimpleNamespace) -> None:
        hostile_lines, hostile = self._lines(_HOSTILE, _HOSTILE_VALUE, args)
        benign_lines, _benign = self._lines(_BENIGN, _BENIGN, args)
        self.assertEqual(hostile_lines, benign_lines,
                         f"the hostile fixture changed the shape of the report: "
                         f"{hostile!r}")
        self.assertNotIn("\x1b", hostile)
        self.assertIn(_MARK, hostile, "the report dropped the value it is about")

    def test_the_release_gate_refusal(self):
        self._both(SimpleNamespace(for_version=_V, pending=False, since=None, until=None))

    def test_the_range_view(self):
        self._both(SimpleNamespace(for_version=None, pending=False,
                                   since="0.59.0", until=_V))

    def test_the_pending_view(self):
        with mock.patch.object(commands.config, "HAS_CONTROL_PLANE", True):
            self._both(SimpleNamespace(for_version=None, pending=True,
                                       since=None, until=None))


class TheGuardIsAtTheAssemblyNotAtTheSpans(unittest.TestCase):
    """What separates this fix from the one #502 argued against.

    Wrapping each span individually fixes the spans somebody enumerated. `_report`
    contains what it is HANDED, so the next field added to one of these templates is
    contained by having been passed through it — which is a property a reviewer can check
    by reading the call site, rather than one they have to remember.
    """

    def test_every_field_handed_to_report_is_contained(self):
        said = news._report("{a} {b}", a=f"x{_LF}y", b=f"p{_LINE_SEP}q")
        self.assertEqual(len(said.splitlines()), 1, said)

    def test_a_sequence_is_contained_element_by_element(self):
        """Not by containing the joined string: the comma is charter's structure, and
        containing the join would clip a long list's last element out of a sentence whose
        purpose is to name every one of them.

        `assertEqual` on the whole result rather than "is it one line", because the
        obvious wrong implementation — drop the branch and let `one_line` stringify the
        list — also produces one line. It produces ``['a\\x0ab', 'c\\x0ad']``: Python's
        repr, with the brackets and quotes of a data structure, in a sentence a person
        reads.
        """
        said = news._report("{names}", names=[f"a{_LF}b", f"c{_LF}d"])
        self.assertEqual(said, "a\\x0ab, c\\x0ad")

    def test_a_sequence_is_joined_by_charter_and_not_by_python(self):
        said = news._report("{names}", names=["one.md", "two.md"])
        self.assertEqual(said, "one.md, two.md")

    def test_the_templates_are_charters_own_text(self):
        """`str.format` reads the TEMPLATE for `{}` slots and never the values, so a
        committed value holding braces is data. Asserted rather than assumed, because it
        is the reason this helper can take a template at all."""
        said = news._report("{a}", a="{b} {0} {}")
        self.assertEqual(said, "{b} {0} {}")
