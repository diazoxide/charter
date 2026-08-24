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

#: Where the "not checkable against a guess" assurance is made. Each must carry the
#: residual alongside it.
CLAIM_DOCS = (
    "skills/secrets/SKILL.md",
    "docs/secrets.md",
    "charter/secrets/fingerprint.py",
    "docs/news/unreleased-a-fingerprint-you-cannot-check-a-guess-against.md",
)

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
        text = flat("docs/news/unreleased-a-vault-charter-cannot-make-private.md")
        self.assertNotRegex(
            text, r"`\.charter/vaults/` no longer lists every vault name",
            "the unscoped claim is false for every plane that already exists")
        self.assertRegex(
            text, r"[Aa] directory that was already there keeps its mode",
            "the entry must say what happens to a directory charter did not create")
        self.assertIn(
            "listed by other accounts", text,
            "and must name the report that replaces the fix it cannot make")


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
