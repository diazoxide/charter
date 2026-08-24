"""Documents that assert the masked line is uncheckable must also carry what it misses.

`charter secret get`'s masked line is `HMAC(plane key, value)` plus a size band, so it is
not a function of the value alone and **cannot be checked against a guess offline**. True,
tested in `test_secret_get_masked.py`, and it is the whole of #436's fix.

It leaves a residual, and the residual is not hypothetical — it was executed on this
branch. Someone who can run `charter secret set` here stores a guess in a vault of their
own and compares its masked line to a target vault's; equal lines confirm the guess. No
guard denies either half. `fingerprint.py` and `docs/secrets.md` said so; the version of
`skills/secrets/SKILL.md` that shipped in round one did not, and told the model instead
that the fingerprint "cannot be checked against a guess" with no qualifier at all and
that there is "nothing to be gained by recomputing it".

**That file is the one document here the model acts on rather than merely reads**, which
is why its omission is the blocking one: a false assurance in a skill is not a doc bug, it
is an instruction to stop being careful about the exact operation that still works.

Round one also shipped a news entry claiming `.charter/vaults/` "no longer lists every
vault name you have to every account on the machine", which was false for every plane that
already existed — a pre-existing 0755 directory is 0755 after `secret set` — and false
even for a fresh one, because `mkdir(parents=True, mode=0o700)` sets the leaf only.
`test_vault_dir_mode.py` fixed the second half in code; the first half is a residual
charter deliberately does not fix, so the entry has to say so.

**What this test is, honestly.** It is a spelling check, and spelling checks are what this
codebase keeps getting bypassed on. It cannot tell a real qualifier from a sentence that
merely contains the right words, and someone determined to reintroduce an overclaim can
write around it. What it does catch is the way the defect actually arrived both times:
somebody edits the confident half of a paragraph and drops the qualifying half, or writes
a fresh assurance and never adds one. The *behavioural* claims are pinned by
`test_secret_get_masked.py` and `test_vault_dir_mode.py`, and one of them — the in-plane
oracle itself — is executed below, so the sentence and the behaviour cannot drift apart
without something here going red.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

from charter import config
from charter.secrets.fingerprint import masked
from charter.secrets.plain_file import PlainFileProvider

from tests._isolation import PersonaIso

DOCS = Path(__file__).resolve().parent.parent


def entries(slug: str) -> tuple[str, ...]:
    """Every news entry carrying *slug*, whatever version prefix it wears today.

    An entry is staged as ``unreleased-<slug>.md`` and renamed to ``<version>-<slug>.md``
    by `charter news stamp` during the bump, so a path spelled here with the
    ``unreleased-`` prefix is a path that **stops existing on the release that ships it**.
    It cost 0.52.0 four red tests, on the one branch that has to be green before a tag,
    for a document whose text never changed.

    The slug is the half that survives, and it is split out the way `charter` itself
    splits it (`news._read`): everything after the first ``-``, because a version prefix
    contains dots and never a dash.

    Raising on no match is the point. These constants feed ``for rel in …`` loops, and a
    loop over an empty tuple is a green test that checked nothing — the exact bypass this
    whole file exists to complain about.
    """
    found = tuple(str(p.relative_to(DOCS)) for p in sorted((DOCS / "docs" / "news").glob("*.md"))
                  if "-" in p.stem and p.stem.split("-", 1)[1] == slug)
    if not found:
        raise AssertionError(
            f"no docs/news entry has the slug {slug!r}. If the entry was renamed, rename "
            f"it here; if it was deleted, the claim it qualifies went with it and this "
            f"test should be moved to whatever document makes the claim now — do not "
            f"drop the reference and leave the loop empty.")
    return found


#: Where the "not checkable against a guess" assurance is made. Each must carry the
#: residual alongside it.
CLAIM_DOCS = (
    "skills/secrets/SKILL.md",
    "docs/secrets.md",
    "charter/secrets/fingerprint.py",
    *entries("a-fingerprint-you-cannot-check-a-guess-against"),
)

#: Where the directory-mode claims are made — "every level is 0700" and "and charter
#: reports the ones it did not create". Both overreached in round two: the first swallowed
#: `.charter/` itself (#470), the second put the report in `doctor` (#471). Both are closed
#: now, and the entry that closes them makes the same two claims, so it is held to the same
#: residual: charter still does not touch a directory it did not create, and still has to
#: say so where it cannot fix it.
DIR_CLAIM_ENTRIES = (*entries("a-vault-charter-cannot-make-private"),
                     *entries("the-state-directory-is-charters-to-choose"))
DIR_CLAIM_DOCS = ("docs/secrets.md", *DIR_CLAIM_ENTRIES)

#: The claim: some form of "this line cannot be checked against a guessed value".
_CLAIM = re.compile(r"against a guess|checkable|not computable|not reproducible", re.I)

#: The residual: some form of "inside this plane, `secret set` a guess and compare".
_RESIDUAL = re.compile(r"equality oracle|set` a guess|store a guess|guess (?:in a vault|"
                       r"and compare)|compare fingerprints", re.I)


def flat(rel: str) -> str:
    """A document with its line wrapping collapsed.

    Every pattern here spans a sentence, and a sentence in these files spans a line break
    wherever the wrap happens to fall. Matching the raw text would make this test fail on
    a reflow and — much worse — pass on a document whose qualifier was deleted and whose
    remaining text simply rewrapped past the pattern.
    """
    return re.sub(r"\s+", " ", (DOCS / rel).read_text())


class EveryAssuranceCarriesItsResidual(unittest.TestCase):

    def test_the_fingerprint_claim_never_stands_alone(self) -> None:
        for rel in CLAIM_DOCS:
            with self.subTest(doc=rel):
                text = flat(rel)
                self.assertTrue(
                    _CLAIM.search(text),
                    f"{rel} no longer makes the claim this test exists to qualify — if "
                    f"the wording moved, move this test with it rather than deleting it")
                self.assertTrue(
                    _RESIDUAL.search(text),
                    f"{rel} tells the reader the masked line cannot be checked against a "
                    f"guess and never says that inside one plane it still can. That is "
                    f"the #436 attack, intact, and it was demonstrated on this branch.")

    def test_the_skill_says_what_to_do_about_it(self) -> None:
        """A residual stated to the model is not enough; the model needs the instruction.

        `docs/secrets.md` can describe a limit and leave the reader to judge. A skill is
        loaded *as behaviour*, so the sentence that matters there is the imperative, not
        the description.
        """
        text = flat("skills/secrets/SKILL.md")
        self.assertRegex(
            text, r"never store a (?:candidate |guess )?value in\s+order to compare",
            "SKILL.md describes the in-plane oracle but never tells the model not to use "
            "it — the one document here that is acted on rather than believed")

    def test_the_directory_claim_is_scoped_to_what_charter_creates(self) -> None:
        for rel in DIR_CLAIM_ENTRIES:
            with self.subTest(doc=rel):
                text = flat(rel)
                self.assertNotRegex(
                    text, r"`\.charter/vaults/` no longer lists every vault name",
                    "the unscoped claim is false for every plane that already exists")
                self.assertRegex(
                    text, r"[Aa] directory that was already there keeps its mode",
                    "the entry must say what happens to a directory charter did not create")
                self.assertIn(
                    "listed by other accounts", text,
                    "and must name the report that replaces the fix it cannot make")

    def test_the_directory_claim_names_the_level_it_did_not_used_to_cover(self) -> None:
        """"Every directory charter creates" was false one level up, and shipped anyway.

        `make_private_dir` was reached from the three secrets writers and from nowhere
        else, so on the default flow `.charter/` was the umask's to decide (#470). It is
        charter's now — every state writer goes through the same walk — and these
        documents have to say which of the two they are describing, because a reader on
        0.52.0 and a reader on the next release are looking at different behaviour.

        The behaviour is pinned in `test_vault_dir_mode.TheOrderTheCliActuallyUses` and in
        `test_the_state_directory_is_charters_to_choose.py`. This case only stops the
        sentence from drifting off it.
        """
        for rel in DIR_CLAIM_DOCS:
            with self.subTest(doc=rel):
                text = flat(rel)
                self.assertIsNotNone(
                    re.search(r"issues/470", text),
                    f"{rel} must name the level the walk did not used to cover, and where "
                    f"that was tracked, rather than leaving the reader to measure it")
                self.assertRegex(
                    text, r"`?\.charter/?`?",
                    f"{rel} makes a claim about directory modes without naming the "
                    f"directory the claim is about")

    def test_every_document_says_which_surfaces_carry_the_note(self) -> None:
        """The note is the whole remedy for a directory charter will not chmod, so where it
        appears is not a detail — it is the sentence the reader acts on.

        It reached `charter vault list` and nothing else, while two documents said
        otherwise (#471). Both surfaces carry it now, from one rendering of one structured
        answer, and the documents name both. Pinned behaviourally by
        `test_doctor_names_a_loose_state_directory.py`.
        """
        for rel in DIR_CLAIM_DOCS:
            with self.subTest(doc=rel):
                text = flat(rel)
                self.assertIsNotNone(
                    re.search(r"issues/471", text),
                    f"{rel} must say where the note appears and where the gap that kept "
                    f"it off `doctor` was tracked")
                self.assertRegex(
                    text, r"vault list",
                    f"{rel} names the loose-directory note without naming the command "
                    f"whose STATUS column prints it")
                self.assertRegex(
                    text, r"doctor",
                    f"{rel} names the loose-directory note without saying anything about "
                    f"`charter doctor`, which is the command an operator runs to find "
                    f"exactly this class of thing")


class TheResidualIsRealAndStillOpen(PersonaIso):
    """The oracle the documents now admit to, executed.

    This is the test that stops the paragraphs above from being a matter of opinion. If
    somebody closes the in-plane oracle later — per-vault salting, a guard on `secret
    set` — this goes RED and the documents saying "still open" become the thing to fix.
    A doc test alone could never notice that, and a stale *reassurance* is the failure
    mode this whole file is about, in the other direction.
    """

    def test_a_fabricated_guess_is_confirmed_by_comparing_masked_lines(self) -> None:
        vd = Path(config.VAULTS_DIR)
        # Not a real credential anywhere: a fabricated string, and the assertions below
        # compare fingerprints rather than reporting either value.
        secret = "fabricated-value-not-a-credential"
        PlainFileProvider("victim", {"file": str(vd / "victim.json")}).set("P", secret)

        attacker = PlainFileProvider("scratch", {"file": str(vd / "scratch.json")})
        target = masked(secret)

        attacker.set("G", "a-wrong-guess")
        self.assertNotEqual(masked(attacker.get("G")), target,
                            "a wrong guess must not match, or the oracle is a constant "
                            "and this test proves nothing")

        attacker.set("G", secret)
        self.assertEqual(
            masked(attacker.get("G")), target,
            "the in-plane equality oracle is what SKILL.md, docs/secrets.md and "
            "fingerprint.py now tell the reader is still open. It just closed — which is "
            "good news and makes those three documents wrong; update them.")


if __name__ == "__main__":       # pragma: no cover
    unittest.main()
