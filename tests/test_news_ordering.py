"""#486: a release leads with the entry that matters, and both consumers agree on which.

`charter news --for <version>` rendered entries in `sorted(d.glob("*.md"))` order, which
for a stamped release is alphabetical by **slug** — a name an author picked for a reason
that had nothing to do with importance. `release.yml`'s announce job pipes that straight
into `gh release create --notes-file`, so slug order *was* the published order: 0.52.0's
vault-spending fix (#453) rendered eighth, under a 1Password docs correction.

**The property is not "security sorts first".** It is that *ordering is declared in the
entry and honoured by every consumer of it*. That distinction is the whole test module.
The obvious fix — teach `render_body` to sort — would pass a test asserting the Release
body leads correctly while `charter news` kept printing slug order, and the two are
deliberately the same answer printed twice (the release charter: "the shipped entry is
the single source for both … hand-editing forks them"). So the assertions below compare
the two consumers' orders **to each other**, from one fixture, rather than each to a
hand-written expectation that could be satisfied by two independently-sorted lists.

The sort therefore lives in `news.all()`, which both reach through, and these tests would
still fail if someone re-sorted at either call site — because the fixture's declared order
disagrees with its filename order in both directions.

Two fields, and they are not two spellings of one:

* `security: true` is a **class**, knowable by the author alone, and any number of a
  version's entries may carry it.
* `lead: true` is a **position**, which no author can know — 24 entries were staged for
  0.52.0 and none of their authors could see the other 23 — so it belongs to the release
  and only one entry per version may claim it.

Fixture slugs below are chosen so filename order and declared order **disagree**: the
entry that should lead is named `z-…` and the ordinary one `a-…`. A fixture where the
right answer is also the alphabetical one is a fixture that cannot fail.
"""

from __future__ import annotations

import io
import re
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, news, persona

_V = "0.60.0"


def _entry(version: str, headline: str, **fields) -> str:
    lines = [f"version: {version}", f"headline: {headline}"]
    lines += [f"{k}: {v}" for k, v in fields.items()]
    return "---\n" + "\n".join(lines) + "\n---\n\nbody text\n"


class NewsDir(unittest.TestCase):
    """Entries read from a throwaway directory, so a test never depends on what shipped —
    the same isolation `tests/test_news.py` establishes, for the same reason."""

    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> None:
        (self.dir / name).write_text(text)

    def three(self, **decls: dict) -> None:
        """Three entries in one version whose filename order is a-, m-, z-. Any declared
        field is passed through, so a caller states only what it is testing."""
        for slug, headline in (("a-ordinary", "ordinary"),
                               ("m-middle", "middle"),
                               ("z-important", "important")):
            self.write(f"{_V}-{slug}.md",
                       _entry(_V, headline, **decls.get(slug, {})))

    # -- the two consumers, each reduced to the sequence of headlines it shows -- #
    def release_body_order(self) -> list[str]:
        """What GitHub publishes: the `### ` headings of `charter news --for`, in order.
        Read off the rendered text rather than off `for_version()`, so a sort applied
        after the entries are fetched would still be caught."""
        return [ln[4:] for ln in news.render_body(_V).splitlines()
                if ln.startswith("### ")]

    def offline_order(self) -> list[str]:
        """What `charter news` shows a reader with no network: the range view's own
        stdout, parsed back. Again the printed text, not the list behind it."""
        out = io.StringIO()
        args = SimpleNamespace(for_version=None, pending=False,
                               since="0.59.0", until=_V)
        with mock.patch.object(news, "probe",
                               return_value=(news.INFORMATIONAL, "")), \
             redirect_stdout(out), redirect_stderr(io.StringIO()):
            self.assertEqual(commands.cmd_news(args), 0)
        return [ln.split("  ", 1)[1] for ln in out.getvalue().splitlines()
                if ln.startswith(_V + "  ")]


class DeclaringNothingChangesNothing(NewsDir):
    """Opt-in, and that is what let 24 shipped entries stay untouched."""

    def test_entries_that_declare_no_order_still_sort_by_filename(self):
        self.three()
        self.assertEqual(self.release_body_order(), ["ordinary", "middle", "important"])

    def test_and_the_offline_view_agrees_with_it(self):
        self.three()
        self.assertEqual(self.offline_order(), self.release_body_order())

    def test_an_undeclared_entry_carries_neither_flag_and_no_complaint(self):
        self.three()
        for e in news.for_version(_V):
            self.assertFalse(e.lead)
            self.assertFalse(e.security)
            self.assertEqual(e.bad, ())
        self.assertEqual(news.entry_errors(news.for_version(_V)), [])


class ADeclaredOrderBeatsTheFilename(NewsDir):
    def test_lead_puts_an_entry_first_however_its_file_is_named(self):
        self.three(**{"z-important": {"lead": "true"}})
        self.assertEqual(self.release_body_order(),
                         ["important", "ordinary", "middle"])

    def test_security_sorts_above_the_ordinary_entries(self):
        self.three(**{"z-important": {"security": "true"}})
        self.assertEqual(self.release_body_order(),
                         ["security: important", "ordinary", "middle"])

    def test_lead_outranks_security_because_a_position_beats_a_class(self):
        """Both declared, on different entries. `security:` says what an entry IS and any
        number may say it; `lead:` says where it GOES and one may. So the one claiming the
        position gets it, and the security entry keeps its place above the rest."""
        self.three(**{"z-important": {"security": "true"},
                      "m-middle": {"lead": "true"}})
        self.assertEqual(self.release_body_order(),
                         ["middle", "security: important", "ordinary"])

    def test_several_security_entries_all_rise_and_keep_filename_order_among_themselves(self):
        """A class may have many members. Within a rank nothing is re-decided, so the
        order is the one it always was — `sorted` is stable and the key ends at the rank."""
        self.three(**{"z-important": {"security": "true"},
                      "a-ordinary": {"security": "true"}})
        self.assertEqual(self.release_body_order(),
                         ["security: ordinary", "security: important", "middle"])


class BothConsumersHonourIt(NewsDir):
    """The anti-fork property, and the reason #486 was not fixed by editing one Release.

    Compared to EACH OTHER rather than to a written-out expectation: a fix that sorted
    inside `render_body` would satisfy a hand-written list for the Release body and leave
    `charter news` printing slug order, which is precisely the fork the release charter
    forbids. Two lists that must be equal cannot both be satisfied by one of them being
    right.
    """

    def test_the_release_body_and_the_offline_view_render_one_order(self):
        self.three(**{"z-important": {"lead": "true"},
                      "m-middle": {"security": "true"}})
        body = self.release_body_order()
        self.assertEqual(body,
                         ["important", "security: middle", "ordinary"])  # not vacuous
        self.assertEqual(self.offline_order(), body)

    def test_and_they_render_one_label(self):
        """`security:` is visible offline too — the half of #486 slug order could never
        give `charter news`, which has no colours, no badges and no rendered markdown."""
        self.three(**{"m-middle": {"security": "true"}})
        self.assertIn("security: middle", news.render_body(_V))
        self.assertIn("security: middle", self.offline_order())

    def test_an_ordinary_entry_is_not_labelled(self):
        self.three(**{"m-middle": {"security": "true"}})
        self.assertNotIn("security: ordinary", news.render_body(_V))

    def test_lead_alone_adds_no_label(self):
        """A position is not a kind. "This was listed first" is already legible from
        being listed first, and a `lead:` badge would let an entry that is not a security
        fix wear a word suggesting it is."""
        self.three(**{"z-important": {"lead": "true"}})
        self.assertEqual(news.marker(news.for_version(_V)[0]), "")
        self.assertIn("### important", news.render_body(_V))


class AValueCharterCannotReadIsSaid(NewsDir):
    """The failure `_flag` exists to refuse: a value the author meant as yes, read as no,
    and the entry sinking to the bottom in silence — #486's own defect, restored by the
    field added to prevent it."""

    def test_true_and_false_are_the_two_values(self):
        self.three(**{"z-important": {"security": "true"},
                      "m-middle": {"security": "false"}})
        self.assertEqual(news.entry_errors(news.for_version(_V)), [])
        self.assertEqual(self.release_body_order(),
                         ["security: important", "ordinary", "middle"])

    def test_case_is_not_a_spelling_difference(self):
        self.three(**{"z-important": {"security": "TRUE"}})
        self.assertEqual(news.entry_errors(news.for_version(_V)), [])
        self.assertEqual(self.release_body_order()[0], "security: important")

    def test_a_value_outside_the_pair_is_reported_rather_than_read_as_false(self):
        """`yes` is the obvious next thing an author types. A truthy-set implementation
        would accept it and be walked past by the one after that; the property here is
        not which words mean yes but whether the value was UNDERSTOOD."""
        self.three(**{"z-important": {"security": "yes"}})
        why, = news.entry_errors(news.for_version(_V))
        self.assertIn("z-important", why)
        self.assertIn("yes", why)
        self.assertIn("`true` or `false`", why)

    def test_a_lookalike_true_is_reported_too(self):
        """Full-width `ｔｒｕｅ` case-folds to itself — it is not `true`, and charter says
        so rather than deciding for the author. This is the codepoint that walks past
        every membership test written against a list of words."""
        self.three(**{"z-important": {"security": "ｔｒｕｅ"}})
        self.assertTrue(news.entry_errors(news.for_version(_V)))

    def test_the_unreadable_value_cannot_forge_a_second_line_of_output(self):
        """A news entry is a committed file, so its frontmatter is untrusted input, and
        this message is a line of charter's own report. `contain.one_line` is what stops a
        newline in the value writing a second one (#453's mechanism, one surface over)."""
        self.three(**{"z-important": {"security": "yes"}})
        # A newline cannot reach the value through flat frontmatter, so plant it directly
        # at the layer the message is built from — the guard has to hold on the argument
        # it is given, not on the parser upstream of it.
        e = news.for_version(_V)[0]._replace(bad=(("security", "yes\nEVIL: line"),))
        why, = news.entry_errors([e])
        self.assertNotIn("\n", why)
        self.assertIn("EVIL", why)  # shown, escaped — not dropped

    def test_the_entry_still_ships_rather_than_vanishing(self):
        """Refusing to parse it would be worse than mis-sorting it: `stamped()` reads
        filenames, so the release guard would still pass while the notes lost an entry
        entirely. The entry renders; the ordering claim is what gets reported."""
        self.three(**{"z-important": {"security": "yes"}})
        self.assertIn("important", self.release_body_order())


class ADeclaredFieldWithNoValueIsUnreadRatherThanUnmade(NewsDir):
    """The first cut of this feature reintroduced #486 through the field added to prevent
    it, and by the same mechanism every bypass in this codebase has used: it matched a
    SPELLING of "missing" instead of asking the property.

    `_read` asked ``(meta.get(field) or "").strip()``. That expression is true of a key
    that is absent AND of a key that is present with nothing after the colon, so the two
    arrived at `_flag` as the same ``""`` and took the "absent is False" path. The
    property is not "is the value empty" but **"was the field declared, and if so was its
    value understood?"** — two facts, and `.get(…) or ""` is exactly the line that makes
    them one. The declaration lives in the dict's KEYS; only the value lives in its values.

    The authoring shape is not exotic, which is what makes it the dangerous one: it is
    the YAML habit of putting the value on the continuation line. `persona.parse` is flat
    ``key: value`` and drops any line without a colon, so the ``  true`` never arrives.
    An author who did opt in got an entry that sorted as though they had not, a release
    that published it eighth, and `charter news --for` exiting 0 with an empty stderr.

    **The next spelling is the KEY, not the value.** ``Security: true`` is a different
    dict key and is genuinely absent here (#503), and two ``security:`` lines in one
    entry silently keep the last (#509). Both are reached through `persona.parse`'s dict
    rather than through `_flag`, and both are filed rather than folded in, because fixing
    either changes how every caller of that parser reads its result.
    """

    #: The value on the continuation line, as an author would actually type it. Written
    #: as raw text rather than through `_entry`, because the helper joins `key: value`
    #: pairs and cannot express the shape under test.
    _CONTINUATION = ("---\n"
                     f"version: {_V}\n"
                     "headline: important\n"
                     "security:\n"
                     "  true\n"
                     "---\n\nbody text\n")

    def _important(self, text: str) -> None:
        """Three entries, with `z-important` replaced by *text*. `z-` sorts last, so an
        entry that fails to be recognised as a security fix stays where the filename put
        it — the fixture cannot pass by coincidence."""
        self.three()
        self.write(f"{_V}-z-important.md", text)

    def test_the_continuation_line_really_does_leave_the_key_present_and_empty(self):
        """The premise, asserted rather than assumed. If `persona.parse` ever grew
        multi-line values this class would be testing a shape that no longer exists, and
        it should say so here rather than keep passing on the rest."""
        meta, _body = persona.parse(self._CONTINUATION)
        self.assertIn("security", meta)
        self.assertEqual(meta["security"], "")

    def test_a_value_on_the_continuation_line_is_reported_not_read_as_false(self):
        self._important(self._CONTINUATION)
        why, = news.entry_errors(news.for_version(_V))
        self.assertIn("z-important", why)
        self.assertIn("security", why)

    def test_and_the_release_gate_refuses_rather_than_publishing_it_eighth(self):
        """The whole point. Before the fix this rendered `ordinary`, `middle`,
        `important` and exited 0 — #486's published order, from an entry that declared
        otherwise."""
        self._important(self._CONTINUATION)
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=_V, pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news(args)
        self.assertEqual(rc, 1)
        self.assertIn("z-important", err.getvalue())
        self.assertEqual(out.getvalue(), "")

    def test_a_declaration_with_nothing_after_the_colon_is_the_same_answer(self):
        """`security:` typed and the value forgotten, and `security: ` with a trailing
        space, reach `_read` as the same empty string the continuation line does. One
        answer for all three, because they are one fact: the field was declared."""
        for text in (f"---\nversion: {_V}\nheadline: important\nsecurity:\n---\n\nb\n",
                     f"---\nversion: {_V}\nheadline: important\nsecurity: \n---\n\nb\n"):
            with self.subTest(text=text):
                self._important(text)
                self.assertTrue(news.entry_errors(news.for_version(_V)))

    def test_lead_declared_empty_is_reported_too(self):
        """Both ordering fields, not just the one the repro used. `_ORDERING_FIELDS` is
        walked in one loop, so a fix applied to `security` alone would be a fix nobody
        wrote — but a test that only ever names `security` would not know."""
        self._important(f"---\nversion: {_V}\nheadline: important\nlead:\n  true\n"
                        f"---\n\nb\n")
        why, = news.entry_errors(news.for_version(_V))
        self.assertIn("lead", why)

    def test_the_message_names_the_shape_instead_of_quoting_an_empty_value(self):
        """There is no value to correct here, so echoing one back — "`security: `" with
        nothing after it — reads as a rendering bug and tells the author nothing. The
        sentence has to name the line the value went onto, because that is the edit."""
        self._important(self._CONTINUATION)
        why, = news.entry_errors(news.for_version(_V))
        self.assertNotIn("`security: `", why)
        self.assertIn("`true` or `false`", why)
        self.assertIn("next line", why.casefold())

    def test_declaring_nothing_at_all_is_still_silent_and_still_false(self):
        """The counterfactual, and it is not decorative: "every field is unreadable"
        passes every assertion above while refusing all 24 shipped entries and breaking
        opt-in outright. Absence is the one input that must stay False."""
        self.three()
        for e in news.for_version(_V):
            self.assertFalse(e.security)
            self.assertFalse(e.lead)
            self.assertEqual(e.bad, ())
        self.assertEqual(news.entry_errors(news.for_version(_V)), [])
        self.assertEqual(self.release_body_order(),
                         ["ordinary", "middle", "important"])

    def test_absent_and_empty_are_two_different_calls_to_the_reader(self):
        """`_flag`'s contract, at the layer the docs test also reaches it through: absence
        is spelled as the absence of a value, and every string — `""` included — is
        something an author typed. Collapsing them at the call site is the defect; a
        reader that cannot tell them apart is where it would come back."""
        self.assertIs(news._flag(None), False)
        self.assertIsNone(news._flag(""))
        self.assertIsNone(news._flag("   "))
        self.assertIs(news._flag("true"), True)
        self.assertIs(news._flag("false"), False)


class TheReleaseGateRefusesAContradiction(NewsDir):
    """`charter news --for <version>` is not just a view — `release.yml` runs it as the
    pre-publish guard and pipes it into `gh release create --notes-file`. So this is where
    an ordering charter cannot honour has to stop, before there is a Release to be wrong.
    """

    def _for(self, version: str = _V) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=version, pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news(args)
        return rc, out.getvalue(), err.getvalue()

    def test_two_entries_claiming_the_lead_stop_the_release(self):
        self.three(**{"z-important": {"lead": "true"}, "a-ordinary": {"lead": "true"}})
        rc, out, err = self._for()
        self.assertEqual(rc, 1)
        self.assertIn("lead", err)
        self.assertIn("z-important", err)
        self.assertIn("a-ordinary", err)

    def test_the_claimants_are_named_in_a_stable_order(self):
        """`sorted`, doing work — found by `tools/sweep.py`, which could delete it with the
        suite green because the row above asserts only that both names appear.

        **Asked of `entry_errors` directly, and that is the point.** Through `cmd_news` the
        entries arrive already in name order, so the sentence comes out alphabetical whether
        this sort is there or not — the first version of this test asserted the right thing
        and was masked by a sort three functions upstream. Handed the claimants in the other
        order, the sentence a release engineer reads out of CI names the same two files
        either way round unless this sort holds, and two runs of one tree print two
        different refusals.
        """
        entries = [self.entry("z-important"), self.entry("a-ordinary")]
        said = news.entry_errors(entries)
        self.assertEqual(len(said), 1, said)
        self.assertLess(said[0].index("a-ordinary"), said[0].index("z-important"), said[0])

    def entry(self, slug: str) -> news.Entry:
        """A claimant, named by its file the way a real entry is."""
        return news.Entry(version=_V, slug=slug, headline="h", check="", adopt="",
                          body="", path=Path(f"{_V}-{slug}.md"), lead=True)

    def test_and_publishes_no_partial_body_while_refusing(self):
        """The announce job redirects this stdout into the notes file. A refusal that
        still printed the entries would put un-ordered notes on the Release anyway."""
        self.three(**{"z-important": {"lead": "true"}, "a-ordinary": {"lead": "true"}})
        _rc, out, _err = self._for()
        self.assertEqual(out, "")

    def test_a_value_it_cannot_read_stops_the_release_too(self):
        self.three(**{"z-important": {"security": "yes"}})
        rc, _out, err = self._for()
        self.assertEqual(rc, 1)
        self.assertIn("z-important", err)

    def test_one_lead_and_many_security_entries_publish_normally(self):
        """The counterfactual: the gate must not refuse the ordering it exists to allow.
        Without this the three tests above pass on a `--for` that always returns 1."""
        self.three(**{"z-important": {"lead": "true"},
                      "m-middle": {"security": "true"},
                      "a-ordinary": {"security": "true"}})
        rc, out, _err = self._for()
        self.assertEqual(rc, 0)
        self.assertIn("### important", out)

    def test_leads_in_DIFFERENT_versions_are_not_a_contradiction(self):
        """One entry per version, not one entry ever. `released()` spans every version
        charter has shipped, so a check written over the whole list rather than per
        version would refuse the second release that ever used the field."""
        self.write(f"{_V}-only.md", _entry(_V, "this one", lead="true"))
        self.write("0.61.0-only.md", _entry("0.61.0", "that one", lead="true"))
        self.assertEqual(news.entry_errors(news.all()), [])
        self.assertEqual(self._for(_V)[0], 0)
        self.assertEqual(self._for("0.61.0")[0], 0)


class TheRangeViewWarnsRatherThanWithholding(NewsDir):
    """A reader catching up is not a release. Losing them nineteen entries because a
    twentieth has a malformed `security:` line would be the wrong trade — `--for` is the
    call that becomes something permanent, and it is the one that refuses."""

    def test_it_still_prints_every_entry(self):
        self.three(**{"z-important": {"security": "yes"}})
        self.assertEqual(len(self.offline_order()), 3)

    def test_and_says_what_it_could_not_read(self):
        self.three(**{"z-important": {"security": "yes"}})
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=None, pending=False, since="0.59.0", until=_V)
        with mock.patch.object(news, "probe", return_value=(news.INFORMATIONAL, "")), \
             redirect_stdout(out), redirect_stderr(err):
            self.assertEqual(commands.cmd_news(args), 0)
        self.assertIn("z-important", err.getvalue())


class WhatShippedStillParses(unittest.TestCase):
    """Over the real `docs/news`, not a fixture: every entry in the tree reads cleanly,
    and no version has two leads. This is what would have caught a hand-edit to a shipped
    entry's frontmatter, and it is the check `release.yml` runs per version, asked once
    across all of them."""

    def test_no_shipped_entry_declares_an_order_charter_cannot_honour(self):
        entries = news.all()
        self.assertTrue(entries, "no news entries found — this test proves nothing")
        self.assertEqual(news.entry_errors(entries), [])

    def test_the_0_52_0_release_leads_with_its_security_fix(self):
        """#486's own case, on real data. 0.52.0 shipped 24 entries and the vault-spending
        fix rendered eighth, below a docs correction; the entry now declares the lead, so
        the offline view of that release is right even though its published Release body
        was rendered before this existed."""
        first, *rest = news.for_version("0.52.0")
        self.assertTrue(rest, "0.52.0 should have many entries")
        self.assertIn("server name", first.headline)
        self.assertTrue(first.lead)


class ADocumentedExampleIsParsedTheWayCharterParsesIt(unittest.TestCase):
    """Every frontmatter block the docs print as copyable must survive `persona.parse`.

    The first draft of this feature's own news entry taught the failure it exists to
    prevent. It showed::

        security: true      # this entry is a security fix

    and `persona.parse` has **no comment syntax** — it is `key, _, value =
    line.partition(":")` and keeps the rest verbatim — so a reader who copied that block
    got the value ``true      # this entry is a security fix``, which `_flag` cannot read.
    Loud rather than silent, and the release gate catches it; but the example still taught
    a syntax charter does not have, and the reader who was taught it is the one with no
    other source.

    The property is not "no `#` in the docs". It is that a block presented as frontmatter
    is run through the real parser and the real field reader — so the test cannot be
    satisfied by an example that merely avoids the one character this draft used. A
    trailing `# …`, an inline YAML `{}`, a quoted `"true"`, a value indented onto the next
    line: each yields something `_flag` does not understand, and each fails here.

    **The fourth of those was a claim this test could not make until `_flag` learned to
    tell absent from empty.** An indented continuation leaves the key present with ``""``,
    and `_flag` used to read that as False — understood, and false — so a doc teaching the
    multi-line spelling passed the test written to stop exactly it, and the reader who
    copied it shipped an entry that sank in silence. The gate above (`if field not in
    meta: continue`) was always structured to catch it; the reader it called was not. That
    is why this class asserts `assertIsNotNone` rather than a value: the question is
    whether charter UNDERSTOOD the block, and "absent" is the only understanding a doc
    example must not reach by accident.

    **The next spelling** is a comment on a key that is *not* typed — `headline: x  # y`
    parses to a headline ending in `# y`, which no reader of this test would learn about
    because free text has no wrong value to detect. It is also the harmless half: a
    mangled headline is visible in the first line of the notes, where a mangled
    `security:` is visible only as an entry sitting lower than it should. If a third typed
    field is ever added, it belongs in `news._ORDERING_FIELDS` and this test picks it up
    with no edit.
    """

    _ROOT = Path(__file__).resolve().parent.parent
    _FENCE = re.compile(r"```[a-z]*\n(.*?)```", re.S)

    def _blocks(self):
        roots = ["docs", "skills", "personas"]
        files = [p for r in roots for p in (self._ROOT / r).rglob("*.md")]
        files += [self._ROOT / "CONTRIBUTING.md", self._ROOT / "README.md"]
        for path in sorted(p for p in files if p.is_file()):
            for m in self._FENCE.finditer(path.read_text()):
                block = m.group(1)
                if block.lstrip().startswith("---") and "version:" in block:
                    yield path, block

    def test_every_documented_news_frontmatter_declares_a_value_charter_reads(self):
        found = list(self._blocks())
        self.assertTrue(found, "no documented frontmatter examples found — vacuous")
        seen_typed = False
        for path, block in found:
            meta, _body = persona.parse(block.lstrip())
            for field in news._ORDERING_FIELDS:
                if field not in meta:
                    continue
                seen_typed = True
                raw = meta[field]
                with self.subTest(path=path.name, field=field):
                    shown = (f"`{field}:` with nothing after it" if raw.strip() == ""
                             else f"`{field}: {raw}`")
                    self.assertIsNotNone(
                        news._flag(raw),
                        f"{path.relative_to(self._ROOT)} shows {shown} in a "
                        f"frontmatter example, and charter reads that as a value it does "
                        f"not understand. The frontmatter parser is flat `key: value` "
                        f"with no comment syntax and no quoting — whatever follows the "
                        f"colon IS the value, and a line without a colon is dropped, so "
                        f"a value on the next line never arrives. Anyone who copies this "
                        f"block gets a refused release.")
        self.assertTrue(seen_typed,
                        "no example declares an ordering field, so this test proves "
                        "nothing about them")


class TheGateAnnotationNamesWhatTheGateNowCatches(NewsDir):
    """The CI sentence a release engineer reads at 2am, with the run and nothing else.

    `release.yml`'s pre-publish guard *is* `charter news --for $version`, and until #486
    that call had exactly one way to exit 1 — no entry — so its `::error::` could state
    that as fact and prescribe `charter news stamp $version`. #486 gave it two more: an
    ordering value charter cannot read, and two entries both claiming `lead: true`; #503
    gave it two after that, a frontmatter key charter does not read (`Security:`,
    `securiy:`) and a file in `docs/news` that is not an entry at all. #902 gave it one
    more, and that one is not a "cannot honour" at all: a value the author wrapped in
    quotes, which charter reads perfectly and publishes with the quotes inside the
    heading. Against any of them, an annotation naming only the missing entry names a
    cause that is not the cause and prescribes a command that changes nothing — and
    stamping a version that is already stamped is a no-op, so the operator's next move
    produces no new information either.

    Which is also why the vocabulary the two texts share is no longer the word "ordering".
    Five of the six causes are not orderings, and a shared word that is true of one cause
    is the same narrowing that put #486 back through the field added to prevent it.

    Three properties, in ascending order of teeth.

    1. The annotation names both causes rather than one, and defers to the command's own
       output for which.
    2. The words it defers with are words the command actually prints. A reader who
       greps the run for the annotation's vocabulary has to land on the line that
       explains it, so the two texts are asserted against each other rather than each
       against a hand-written expectation.
    3. **The stream it points at survives the redirect.** `>/dev/null` drops stdout
       alone, deliberately — that stdout is the announce job's notes file. `2>&1` or
       `&>` would drop the half the sentence points at, turning "see the error above"
       into a lie that nothing else in the run would notice.

    And the recurring failure here is a *fourth* exit-1 path added to `--for` without
    anyone remembering this annotation exists. `test_a_new_refusal_is_a_new_sentence`
    below counts them at the source, so adding one goes red on the PR that adds it.
    """

    _WORKFLOW = Path(__file__).resolve().parent.parent / ".github/workflows/release.yml"

    def setUp(self):
        super().setUp()
        text = self._WORKFLOW.read_text()
        self.assertIn("news --for", text, "the guard no longer runs `charter news --for`")
        self.gate = next(ln for ln in text.splitlines() if "news --for" in ln
                         and "python" in ln)
        self.annotation = next(ln for ln in text.splitlines() if "::error::" in ln
                               and "news" in ln)

    def _stderr_of(self, version: str) -> str:
        err = io.StringIO()
        args = SimpleNamespace(for_version=version, pending=False, since=None, until=None)
        with redirect_stdout(io.StringIO()), redirect_stderr(err):
            self.assertEqual(commands.cmd_news(args), 1,
                             f"--for {version} was expected to refuse")
        return err.getvalue()

    def test_the_annotation_names_the_declared_cause_as_well_as_the_missing_entry(self):
        self.assertIn("docs/news/", self.annotation)
        self.assertIn("cannot honour", self.annotation)

    def test_and_the_cause_that_is_not_about_the_entries_at_all(self):
        """#665's shape, and the reason it needs its own clause rather than a widening of
        "cannot honour": nothing is wrong with the entries. Each of them is well-formed,
        declares nothing charter cannot read, and renders — and the release still cannot be
        announced, because the body they render to is longer than GitHub's create-release
        API accepts. An operator sent looking for a malformed entry would find none.

        Coupled on "too long" for the reason every other pair in this class is coupled:
        that is the phrase `cmd_news` prints and the phrase GitHub's own refusal uses, so a
        reader grepping the run for the annotation's vocabulary lands on the line that
        explains it. `tests/test_release_notes_fit_the_release.py` holds the other end.
        """
        self.assertIn("too long", self.annotation)

    def test_and_the_cause_where_the_entry_is_right_about_everything_but_the_format(self):
        """#902's shape, and it needs its own clause for the mirror image of #665's
        reason. There, nothing was wrong with the entries; here, nothing is wrong with
        them *as charter reads them* — the value parses, renders, and ships. What is wrong
        is that the author believed the frontmatter was YAML, which it looks exactly like,
        and the quotes they added in good faith end up inside the published `###` heading.

        An operator sent to look for "something charter cannot honour" would read six
        well-formed entries and find nothing, which is how 0.56.0 published six of these.
        So the annotation carries the word the command prints — `unquote` — and the pair
        is asserted here rather than each against a hand-written string.
        """
        self.assertIn("unquote", self.annotation)

    def test_a_quoted_value_prints_the_word_the_annotation_sends_a_reader_to_find(self):
        self.three()
        self.write(f"{_V}-z-important.md", _entry(_V, "'important'"))
        self.assertIn("unquote", self._stderr_of(_V))

    def test_and_defers_to_the_command_for_which_of_them_it_was(self):
        """Rather than diagnosing. Two causes named in one line is only an improvement if
        the line also says where the answer is."""
        self.assertIn("the error above", self.annotation.casefold())

    def test_a_missing_entry_prints_the_words_the_annotation_sends_a_reader_to_find(self):
        self.assertIn("news entry", self._stderr_of("9.9.9"))

    def test_a_declaration_charter_cannot_honour_prints_them_too(self):
        """The coupling that makes property 1 more than a word in a YAML file: reword
        `cmd_news`'s refusal to drop "cannot honour" and this goes red, because the
        annotation is still sending the reader to look for it.

        Every shape that refuses, not one of them. The sentence the annotation sends the
        reader to find is `cmd_news`'s wrapper line, and a refusal added later that
        printed only its own per-entry sentence would leave that reader grepping the log
        for a word the run never says.

        The last three shapes are #503's, and they are the reason the coupled word is no
        longer "ordering": `securiy: true` is not an ordering claim, and a file whose key
        reads `Version:` is not an entry at all — but each of them stops a release, and
        each has to print the vocabulary the annotation promises.
        """
        shapes = {
            "two entries claim the lead":
                lambda: self.three(**{"z-important": {"lead": "true"},
                                      "a-ordinary": {"lead": "true"}}),
            "a value charter cannot read":
                lambda: self.three(**{"z-important": {"security": "yes"}}),
            "a field declared with no value":
                lambda: (self.three(),
                         self.write(f"{_V}-z-important.md",
                                    f"---\nversion: {_V}\nheadline: important\n"
                                    f"security:\n  true\n---\n\nb\n")),
            "an ordering field spelled in another case":
                lambda: self.three(**{"z-important": {"Security": "true"}}),
            "a key that is a near miss for one charter reads":
                lambda: self.three(**{"z-important": {"securiy": "true"}}),
            "a file in the directory that is not an entry":
                lambda: (self.three(),
                         self.write(f"{_V}-z-important.md",
                                    f"---\nVersion: {_V}\nheadline: important\n"
                                    f"---\n\nb\n")),
        }
        for name, stage in shapes.items():
            with self.subTest(shape=name):
                stage()
                self.assertIn("cannot honour", self._stderr_of(_V))

    def test_the_gate_drops_only_the_stream_the_annotation_does_not_point_at(self):
        """stdout is discarded on purpose — it is the Release body, and the guard wants
        the exit code, not the notes. stderr is the whole of what the annotation promises
        is "above", so a redirect that swallowed it would break the sentence silently."""
        self.assertIn(">/dev/null", self.gate)
        for swallow in ("2>&1", "2>/dev/null", "&>"):
            self.assertNotIn(swallow, self.gate,
                             f"the guard's `{swallow}` discards the stderr its own "
                             f"annotation tells the operator to read")

    def test_a_new_refusal_is_a_new_sentence(self):
        """The next spelling, made checkable.

        Nothing about adding a third `return 1` to this branch would have told its author
        that a workflow annotation two directories away enumerates them. So the count is
        pinned here: raise it and this test names the file to update, in the same PR that
        creates the obligation.

        It has already paid for itself once. #902 added the fourth — a value the entry
        quoted — and this is the test that said the annotation had to grow a clause for it
        in the same commit.
        """
        import inspect

        src = inspect.getsource(commands.cmd_news)
        branch = src.split("if version:", 1)[1].split("if getattr(args, \"pending\"", 1)[0]
        self.assertEqual(
            branch.count("return 1"), 4,
            "`charter news --for` has a number of ways to refuse that this test no "
            "longer expects. That call is release.yml's pre-publish guard, and its "
            "`::error::` annotation enumerates the causes for an operator who has only "
            "the CI log — update `.github/workflows/release.yml` and this count together.")


if __name__ == "__main__":
    unittest.main()
