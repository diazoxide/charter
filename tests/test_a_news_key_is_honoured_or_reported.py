"""#503: a field an author declared is honoured or reported. Never neither.

`news._read` looked its ordering fields up by an exact, case-sensitive key. `persona.parse`
keeps a key exactly as written, so ``Security: true`` put ``"Security"`` in the dict,
``meta.get("security")`` returned nothing, and the entry was **absent** — not wrong, not
reported, absent. It sorted below the ordinary entries, `ordering_errors` returned ``[]``,
and the release gate exited 0 with an empty stderr.

That is #486's own defect restored by the field added to prevent it, reached through the
KEY instead of the value. `_flag` was built so a value charter does not understand is
reported and never read as false; a capitalised key got no such treatment, because there
was no value to report — the lookup never found one. **The key half fails more quietly than
the value half**, and the release notes are the one document nobody re-derives, so an entry
that does not render is indistinguishable from an entry nobody wrote.

**Two answers were defensible and they are not the same.** Accept any case, or refuse
loudly. This is the second, and the reason is the first one's blast radius in both
directions:

* Case-folding answers ``Security:`` and nothing else. ``securiy:``, ``leads:``,
  ``security-fix:`` and ``sec urity:`` parse cleanly, are looked up by nothing, and sink an
  entry in exactly the same silence. Accepting more spellings is a guard written against a
  spelling — which is the shape this project keeps filing (#547, #558, #537, #498) — where
  the property is "was this declaration read by anything?".
* The dict a fold would have to change is `persona.parse`'s, and it is read by key at every
  caller: `persona.load`, `structural_errors`, `docsrc`, `news._read`, for ``role:``,
  ``vault:``, ``delegate-when:``, ``extends:``, ``tools:``. One of those decides which
  credentials a persona reaches. And a folding parser owes an answer to
  ``{"Security": "true", "security": "false"}``, which is two keys today (#509).
* Refusing loudly already has a caller ready to act on it: `charter news --for` exits
  non-zero for more than "nothing to render" (#486), and `release.yml`'s guard job is built
  on that exit code.

So `news` declares a CLOSED set of keys and reports anything outside it. Case stops being a
special case of anything, because there is no longer an unspoken key of any kind — and
charter never guesses what the author meant, which is the one thing a fold has to do.

The sibling with worse teeth is below: a miscased ``Version:`` does not sink an entry, it
**deletes** it. `_read` finds no version and returns ``None``, so the file never becomes an
`Entry` and no per-entry check exists to be asked about it — while `stamped()` answers the
release guard from FILENAMES and says the version has its entry. `news.unreadable` is that
half.

Entries here are named by slug in the prose and written into a throwaway directory, never
by the path of a staged entry: `charter news stamp` renames those during a release, and a
test that opens one by its staged name goes red in the middle of one.
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


def _entry(version: str = _V, headline: str = "important", **fields) -> str:
    lines = [f"version: {version}", f"headline: {headline}"]
    lines += [f"{k}: {v}" for k, v in fields.items()]
    return "---\n" + "\n".join(lines) + "\n---\n\nbody text\n"


class NewsDir(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.dir, ignore_errors=True)
        patch = mock.patch.object(news, "_PACKAGED", self.dir)
        patch.start()
        self.addCleanup(patch.stop)

    def write(self, name: str, text: str) -> None:
        (self.dir / name).write_text(text)

    def two(self, **fields) -> None:
        """The issue's own fixture: an ordinary entry named `a-…` and the one that
        matters named `z-…`, so an entry that fails to be recognised as a security fix
        stays where the filename put it. A fixture whose right answer is also the
        alphabetical one cannot fail."""
        self.write(f"{_V}-a-ordinary.md", _entry(headline="ordinary"))
        self.write(f"{_V}-z-fix.md", _entry(headline="the security fix", **fields))

    def gate(self, version: str = _V) -> tuple[int, str, str]:
        """`charter news --for`, which IS `release.yml`'s pre-publish guard."""
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=version, pending=False, since=None, until=None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news(args)
        return rc, out.getvalue(), err.getvalue()

    def headlines(self) -> list[str]:
        return [ln[4:] for ln in news.render_body(_V).splitlines()
                if ln.startswith("### ")]


class TheIssuesOwnReproduction(NewsDir):
    """Verbatim from #503, and every assertion in it was true before this change."""

    def test_the_declaration_is_reported_rather_than_returning_nothing(self):
        self.two(Security="true")
        why, = news.entry_errors(news.all())
        self.assertIn("z-fix", why)
        self.assertIn("Security", why)

    def test_and_the_release_gate_refuses_instead_of_publishing_it_second(self):
        """The whole point. Before this, `--for` exited 0, printed `### ordinary` first,
        and the security fix rendered below it — with an empty stderr."""
        self.two(Security="true")
        rc, out, err = self.gate()
        self.assertEqual(rc, 1)
        self.assertIn("z-fix", err)
        self.assertEqual(out, "", "a refusal must publish no partial body")

    def test_lead_in_another_case_is_the_same_answer(self):
        """Both ordering fields, not the one the repro used. A fix applied to `security`
        alone would be a fix nobody wrote, and a test that only ever names `security`
        would not know."""
        self.two(Lead="true")
        why, = news.entry_errors(news.all())
        self.assertIn("Lead", why)

    def test_and_so_is_a_shouted_one(self):
        self.two(SECURITY="true")
        why, = news.entry_errors(news.all())
        self.assertIn("SECURITY", why)

    def test_a_leading_space_was_never_the_failure(self):
        """`persona.parse` strips the key, so ` security:` really does arrive as
        `security` — the issue says so and it is worth pinning, because a fix aimed at
        whitespace instead of at the unrecognised key would pass every case above."""
        self.write(f"{_V}-a-ordinary.md", _entry(headline="ordinary"))
        self.write(f"{_V}-z-fix.md",
                   f"---\nversion: {_V}\nheadline: the security fix\n"
                   f" security: true\n---\n\nb\n")
        self.assertEqual(news.entry_errors(news.all()), [])
        self.assertEqual(self.headlines()[0], "security: the security fix")


class CharterReportsRatherThanGuessing(NewsDir):
    """The decision, asserted as behaviour rather than left in a docstring.

    Accepting any case would make `Security: true` a security fix. Refusing makes it a
    sentence. These are the assertions that tell the two apart, and they are the ones that
    would go red if somebody "fixed" this later by folding the lookup — which is a thing
    worth going red for, because the fold is not a superset of this: it decides for the
    author, and it has to decide again the moment two keys differ only in case (#509).
    """

    def test_the_entry_is_not_quietly_promoted_to_a_security_fix(self):
        self.two(Security="true")
        entry = next(e for e in news.for_version(_V) if e.slug == "z-fix")
        self.assertFalse(entry.security,
                         "charter guessed what the author meant instead of saying so")
        self.assertEqual(news.marker(entry), "")

    def test_the_lookup_that_finds_the_key_is_not_the_one_that_reads_it(self):
        """The subtler version of the same fold, and the one a later reader is most
        likely to write: leave the value lookup exact and make only the *unknown-key*
        check case-insensitive, so `Security:` is "recognised" and therefore not
        reported. That is silence again, arrived at from the other side — the key is
        accepted by the check and read by nothing.
        """
        self.two(Security="true")
        got = next(e for e in news.all() if e.slug == "z-fix")
        self.assertEqual(got.unknown, ("Security",),
                         "the key was treated as recognised and then read by nothing")
        self.assertFalse(got.security)

    def test_the_entry_still_ships_rather_than_vanishing(self):
        """Refusing to parse it would be worse than mis-sorting it: `stamped()` reads
        filenames, so the release guard would still pass while the notes lost an entry.
        The entry renders; the declaration is what gets reported."""
        self.two(Security="true")
        self.assertIn("the security fix", self.headlines())

    def test_declaring_the_key_correctly_is_still_silent_and_still_works(self):
        """The counterfactual, and it is not decorative: "every key is unrecognised"
        passes every assertion above while refusing all 105 shipped entries."""
        self.two(security="true")
        self.assertEqual(news.entry_errors(news.all()), [])
        self.assertEqual(self.headlines()[0], "security: the security fix")
        self.assertEqual(self.gate()[0], 0)


class ANearMissIsTheSameSilenceAndTheSameAnswer(NewsDir):
    """Why case is not the property. Every key below parses cleanly, is looked up by
    nothing, and sinks the entry exactly as `Security:` did — so a case-insensitive lookup
    would have fixed the issue's title and left the issue."""

    def test_each_unrecognised_key_is_reported(self):
        for key in ("securiy", "leads", "security-fix", "sec urity", "Sécurity",
                    "security_fix", "securitys"):
            with self.subTest(key=key):
                self.setUp()
                self.two(**{key: "true"})
                why, = news.entry_errors(news.all())
                self.assertIn(key, why)
                self.assertIn("z-fix", why)
                self.assertEqual(self.gate()[0], 1)

    def test_a_key_from_the_future_is_reported_rather_than_dropped(self):
        """A field a newer charter reads, in an entry an older charter is rendering. It
        is not a typo and it is still worth a sentence: this charter is about to publish
        notes that do not carry what the entry declared."""
        self.two(embargo="2030-01-01")
        why, = news.entry_errors(news.all())
        self.assertIn("embargo", why)

    def test_the_sentence_lists_what_charter_does_read(self):
        """A near miss is only actionable if the reader can see what they missed. The
        list comes from `_KNOWN_FIELDS` rather than from prose, so it cannot drift from
        the set that decides."""
        self.two(securiy="true")
        why, = news.entry_errors(news.all())
        for field in news._KNOWN_FIELDS:
            self.assertIn(f"`{field}:`", why)

    def test_a_key_that_is_only_a_case_away_gets_the_shorter_sentence(self):
        """The author has already written the right word. Handing them the whole list to
        hunt through, for a difference that is not in the letters, is the message failing
        at the one job it has."""
        self.setUp()
        self.two(Security="true")
        miscased, = news.entry_errors(news.all())
        self.setUp()
        self.two(securiy="true")
        typo, = news.entry_errors(news.all())
        self.assertIn("only in case", miscased)
        self.assertNotIn("only in case", typo)
        self.assertLess(len(miscased), len(typo))


class TheSetOfFieldsIsClosedAndEveryNameInItIsRead(NewsDir):
    """`_KNOWN_FIELDS` is what makes an unknown key answerable, so the set itself has to
    be honest in both directions.

    A name IN it that `_read` does not actually read is a key charter accepts and then
    ignores — which is the silence the set exists to remove, wearing a commit. A field
    `_read` reads that is NOT in it is reported on every entry that declares it, which is
    loud, wrong, and would stop a release; noisy is the safe direction of that drift, and
    it is still worth catching here.
    """

    def test_every_known_field_is_a_field_of_an_entry(self):
        self.assertTrue(set(news._KNOWN_FIELDS) <= set(news.Entry._fields),
                        f"{set(news._KNOWN_FIELDS) - set(news.Entry._fields)} is accepted "
                        f"in frontmatter and is not carried on an Entry")

    def test_every_known_field_declared_alone_changes_the_entry_it_is_declared_on(self):
        """Read, not merely tolerated. Each field is declared on its own and compared
        against an entry that declared nothing — so a name added to the set without a
        reader goes red here rather than being accepted in silence forever."""
        self.write(f"{_V}-a-bare.md", _entry())
        bare = news.for_version(_V)[0]
        values = {"version": "0.61.0", "headline": "another", "check": "doctor",
                  "adopt": "doctor", "lead": "true", "security": "true"}
        self.assertEqual(set(values), set(news._KNOWN_FIELDS),
                         "this test does not know a value for every declarable field")
        for field in news._KNOWN_FIELDS:
            with self.subTest(field=field):
                self.setUp()
                self.write(f"{_V}-a-bare.md", _entry(**{field: values[field]}))
                got, = news.all()
                self.assertNotEqual(getattr(got, field), getattr(bare, field),
                                    f"`{field}:` is accepted and read by nothing")

    def test_no_known_field_is_ever_reported_as_unknown(self):
        self.write(f"{_V}-a-all.md",
                   _entry(check="doctor", adopt="doctor", lead="true", security="false"))
        got, = news.all()
        self.assertEqual(got.unknown, ())
        self.assertEqual(news.entry_errors([got]), [])

    def test_an_entry_that_declares_nothing_extra_carries_no_unknown_keys(self):
        self.two()
        for e in news.all():
            self.assertEqual(e.unknown, ())
        self.assertEqual(news.entry_errors(news.all()), [])


class AMiscasedVersionDeletesTheEntryAndCharterSaysSo(NewsDir):
    """The sibling with worse teeth, and the reason `unreadable` exists.

    `Security:` sinks an entry. `Version:` removes it: `_read` finds no version and returns
    ``None``, so there is no `Entry` for `entry_errors` to have an opinion about and the
    file is dropped before `all`'s consumers see it. The release guard does not notice
    either — `stamped()` answers from filenames, so a file named `<version>-<slug>.md`
    satisfies "every published version ships an entry" while rendering into neither the
    Release body nor `charter news`.
    """

    def _lost(self) -> None:
        self.write(f"{_V}-a-ordinary.md", _entry(headline="ordinary"))
        self.write(f"{_V}-z-fix.md",
                   f"---\nVersion: {_V}\nheadline: the one that vanished\n---\n\nb\n")

    def test_the_file_really_does_fall_out_of_every_view(self):
        """The premise. If `_read` ever started returning an entry for this, the class
        would be testing a shape that no longer exists and should say so here."""
        self._lost()
        self.assertEqual([e.slug for e in news.all()], ["a-ordinary"])
        self.assertNotIn("the one that vanished", news.render_body(_V))

    def test_and_the_filename_still_satisfies_the_stamped_check(self):
        """Which is why nothing else catches it: the guard that asks "does this version
        have an entry?" is answered by a name on the disk."""
        self._lost()
        with mock.patch.object(news, "_CHECKOUT", self.dir), \
             mock.patch.object(news, "_is_checkout", return_value=True):
            self.assertEqual(len(news.stamped(_V)), 2)

    def test_charter_names_the_file_and_the_key(self):
        self._lost()
        why, = news.unreadable()
        self.assertIn("z-fix", why)
        self.assertIn("Version", why)
        self.assertIn("version:", why)

    def test_and_the_release_gate_refuses(self):
        self._lost()
        rc, out, err = self.gate()
        self.assertEqual(rc, 1)
        self.assertIn("z-fix", err)
        self.assertEqual(out, "")

    def test_a_directory_of_ordinary_entries_reports_nothing(self):
        """The counterfactual. Without it every assertion above passes on an
        `unreadable()` that refuses everything."""
        self.two()
        self.assertEqual(news.unreadable(), [])
        self.assertEqual(self.gate()[0], 0)


class AFileThatIsNotAnEntryIsAlsoSaid(NewsDir):
    """`unreadable` is asked over the directory, so the other ways a file lands there and
    renders as nothing get a sentence too. Each names the edit, because "not an entry"
    alone sends a release engineer to open a file and guess."""

    def test_a_file_with_no_frontmatter(self):
        self.two()
        self.write(f"{_V}-z-notes.md", "just some prose, no frontmatter at all\n")
        why, = news.unreadable()
        self.assertIn("z-notes", why)
        self.assertIn("frontmatter", why)

    def test_a_file_whose_frontmatter_declares_no_version(self):
        self.two()
        self.write(f"{_V}-z-nover.md", "---\nheadline: h\n---\n\nb\n")
        why, = news.unreadable()
        self.assertIn("z-nover", why)
        self.assertIn("version:", why)

    def test_a_version_declared_with_nothing_after_the_colon(self):
        """The same shape `_flag` answers for an ordering field, one field up. The
        author put the value on the continuation line; charter's frontmatter is flat."""
        self.two()
        self.write(f"{_V}-z-empty.md", f"---\nversion:\n  {_V}\nheadline: h\n---\n\nb\n")
        why, = news.unreadable()
        self.assertIn("z-empty", why)

    def test_every_reason_is_one_sentence_that_names_the_file(self):
        """The set of files reported comes from `_read` returning None; only the WORDING
        is `_not_an_entry`'s. A sixth way for `_read` to decline gets the fallthrough
        sentence rather than no sentence — which is the property, not the prose."""
        for name, text in ((f"{_V}-z-a.md", "prose\n"),
                           (f"{_V}-z-b.md", "---\nheadline: h\n---\n\nb\n"),
                           (f"{_V}-z-c.md", f"---\nVersion: {_V}\n---\n\nb\n")):
            with self.subTest(name=name):
                self.setUp()
                self.write(name, text)
                why, = news.unreadable()
                self.assertEqual(len(why.splitlines()), 1, why)
                self.assertIn(name.split("-", 1)[1].removesuffix(".md"), why)


class TheRangeViewWarnsRatherThanWithholding(NewsDir):
    """A reader catching up is not a release. Losing them nineteen entries because a
    twentieth spelled a key wrong would be the wrong trade — `--for` is the call that
    becomes something permanent, and it is the one that refuses."""

    def _range(self) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(for_version=None, pending=False, since="0.59.0", until=_V)
        with mock.patch.object(news, "probe", return_value=(news.INFORMATIONAL, "")), \
             redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_news(args)
        return rc, out.getvalue(), err.getvalue()

    def test_a_miscased_key_is_warned_about_and_every_entry_still_prints(self):
        self.two(Security="true")
        rc, out, err = self._range()
        self.assertEqual(rc, 0)
        self.assertIn("Security", err)
        self.assertEqual(len([ln for ln in out.splitlines() if ln.startswith(_V)]), 2)

    def test_a_file_that_is_not_an_entry_is_warned_about_too(self):
        self.two()
        self.write(f"{_V}-z-notes.md", "prose\n")
        rc, _out, err = self._range()
        self.assertEqual(rc, 0)
        self.assertIn("z-notes", err)


class WhatShippedDeclaresNothingCharterDoesNotRead(unittest.TestCase):
    """Over the real news directory, not a fixture. Every entry in the tree declares only
    keys charter reads, and every file in the directory is an entry.

    This is the assertion that makes the closed set a decision rather than a hope: it is
    what would have gone red on the commit that added a seventh key without teaching
    `_read` about it, and it is what says the change refuses nothing that already ships.
    """

    def test_no_shipped_entry_declares_a_key_charter_does_not_read(self):
        entries = news.all()
        self.assertTrue(entries, "no news entries found — this test proves nothing")
        self.assertEqual([(e.slug, e.unknown) for e in entries if e.unknown], [])

    def test_and_every_file_in_the_directory_is_an_entry(self):
        self.assertEqual(news.unreadable(), [])

    def test_which_together_is_what_the_release_gate_asks(self):
        self.assertEqual(news.entry_errors(news.all()), [])
