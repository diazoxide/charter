"""Two spellings of one path got two answers out of the vault guards.

`cat .charter/vaults/db.json` was denied. `cat .charter//vaults/db.json` was allowed — one
extra separator, the identical file to the kernel, no wrapper, no interpreter, no program
off the allowlist. `.charter/./vaults/db.json` likewise. This is not the documented
`_READERS` ceiling (a program the guard does not know) and not the documented path-spelling
limit (a path *you* chose, outside the plane): it is the guard failing on the one path it
claims to cover, which is what `SECURITY.md` and `docs/hooks.md` now promise it catches.

**The property, stated no wider than what is tested here.** The guards answer on the TEXT
OF THE OPERAND AS WRITTEN, and this file pins that they are complete over four ways that
text can vary while still naming the same path *without a shell's help*: redundant `/`
separators, dot segments, LETTER CASE, and — for the vault DIRECTORY — the presence or
absence of a trailing slash. So the tests generate those spellings mechanically and assert
every one lands where the canonical form lands: denied for a vault file and for the
directory that holds every vault file, allowed for the registry and for ordinary work. A
new separator or case trick is a new element of `_RESPELLINGS`, not a new bypass.

**And that the two routes agree.** The fourth item above was missing for a review round
precisely because every test here asked ONE guard about the operands IT was written for.
`grep -rn TOKEN .charter/vaults` printed a fabricated password through the Bash route while
`Grep(path=".charter/vaults")` refused the identical target, because `pretooluse_read`
carried a private "retry with a `/` appended" step and `_leak_reason` did not. Both this
file and `tests/test_vault_read_guard.py` were green throughout. `TestTheTwoGuardsCannotDisagree`
is the answer to that shape: one corpus, both routes, fails in either direction, so a repair
made in one caller reddens it instead of hiding in it.

**What is deliberately NOT closed, pinned as behaviour rather than left to be rediscovered.**
`TestWhatStillWalksPastBothGuards` — an operand that merely CONTAINS the vault directory,
e.g. `grep -rn TOKEN .` (#474). `TestSeparatorMeansTheForwardSlash` — a Windows-style
backslash spelling (#476). Both are stated as limits in `SECURITY.md`, `docs/secrets.md` and
`skills/secrets/SKILL.md`, and if either ever starts being denied these tests fail next to
the paragraph that has to move with it.

**What this file does NOT claim, because the first version of this docstring did and it was
false.** It said the decision "depends on the path an operand names, not on how it is
spelled". `cat .charter/vault?/db.json` names the same path and is allowed; so do
`V=.charter/vaults/db.json; cat $V` and `cd .charter/vaults && cat db.json`. Those are not
respellings of a string — they are a SHELL rewriting the operand after the guard has already
answered, on text the guard never sees. That whole family is the sixth documented limit and
is pinned, with proof that each one really reads the file, by
`tests/test_documented_limits.py::TestAShellExpandsAfterTheGuardHasAnswered`. The line
between the two files is the line the guard can actually hold: case and separators are
properties of a string it already has; glob and `$VAR` are properties of a shell it is not.

Case joined this file rather than that one because it is the first kind: on macOS/APFS and
on Windows the filesystem folds case, so `.CHARTER/vaults/db.json` was the SAME INODE as the
denied form and was allowed — a guard whose answer depended on which filesystem it ran on.
`_VAULT_PATH_RE` now carries `re.IGNORECASE`, which closes every case spelling rather than a
list of them.

Deliberately NOT `realpath`: resolving would follow symlinks and stat every operand of every
Bash tool call. A symlink planted at a path the caller chose is the limit `SECURITY.md`
states and `tests/test_documented_limits.py` pins, not a case this closes.
"""

from __future__ import annotations

import itertools
import re
import unittest

from charter import hooks
from tests._isolation import PersonaIso, run_hook

#: Spellings of one path that name the same file. The first group holds on every POSIX
#: filesystem and is collapsed by `normpath`; the second holds wherever the filesystem folds
#: case — macOS/APFS and Windows by default — and is covered by `re.IGNORECASE`.
#:
#: `swapcase` and `upper` are applied to the WHOLE operand deliberately: an entry that only
#: upper-cased `.charter` would be a literal bad input dressed up as a transform, and the
#: next bypass would be the segment it did not name. The exhaustive test below goes further
#: and enumerates every case permutation of the guarded prefix.
_RESPELLINGS = (
    ("as written", lambda p: p),
    ("doubled separator", lambda p: p.replace("/", "//", 1)),
    ("every separator doubled", lambda p: p.replace("/", "//")),
    ("a dot segment", lambda p: p.replace("/", "/./", 1)),
    ("a leading dot segment", lambda p: "./" + p),
    ("an up-and-back segment", lambda p: p.replace("/", "/x/../", 1)),
    ("tripled separator", lambda p: p.replace("/", "///", 1)),
    ("upper-cased", lambda p: p.upper()),
    ("swapcased", lambda p: p.swapcase()),
    ("title-cased", lambda p: p.title()),
    ("upper-cased with a dot segment", lambda p: p.upper().replace("/", "/./", 1)),
)

#: A real vault file, the registry beside it (config and paths, never values — an ordinary
#: read, and the false positive that a wider pattern would reintroduce), and an unrelated
#: file. Values are fabricated names only; nothing here reads a real plane.
VAULT = ".charter/vaults/db.json"
REGISTRY = ".charter/vaults.json"
ORDINARY = "docs/secrets.md"
#: The vault DIRECTORY named without a trailing slash — the operand that walks EVERY vault
#: file, and the one the two guards disagreed about for a whole review round (#462). It is a
#: constant rather than a literal in one test because both routes and the differential all
#: have to answer for it.
DIRECTORY = ".charter/vaults"


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestTheBashGuardAnswersThePathNotTheSpelling(unittest.TestCase):
    def test_every_spelling_of_a_vault_file_is_denied(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                path = respell(VAULT)
                reason = hooks._leak_reason(f"cat {path}")
                self.assertIsNotNone(reason, f"allowed as {name}")
                self.assertIn("reads a vault/secret file directly", reason)

    def test_every_spelling_of_the_registry_stays_allowed(self):
        """The registry holds provider config and file paths, never a value. Denying it is
        the false positive #443's predecessor fixed, and a wider pattern brings it back."""
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(hooks._leak_reason(f"grep -rn vaults {respell(REGISTRY)}"),
                                  f"denied as {name}")

    def test_every_spelling_of_an_ordinary_file_stays_allowed(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(hooks._leak_reason(f"cat {respell(ORDINARY)}"),
                                  f"denied as {name}")

    def test_the_raw_operand_is_still_tested_so_no_denial_was_traded_away(self):
        """The union is raw OR normalised, and this pins the raw half.

        `cat .charter/vaults/../../elsewhere` matches the pattern as written and normalises
        to a path outside the plane. Semantically the normalised answer is the right one —
        that command reads no vault — so this denial is a false positive charter keeps on
        purpose: a fix for a *spelling* hole may not quietly hand back a denial that existed
        before it, and fail-closed is the correct direction for a guard under review.
        Without this test, deleting the raw arm passes the whole file."""
        self.assertIsNotNone(hooks._leak_reason("cat .charter/vaults/../../elsewhere"))

    def test_a_directory_operand_is_denied_with_or_without_the_slash(self):
        """The recursive grep that walks EVERY vault file, in both of its spellings.

        `normpath` strips a trailing slash, so an earlier version put one back before
        matching — and the pattern it was feeding demanded a literal `vaults/`, which is why
        the slash mattered at all. Anchoring `vaults` to a path segment answers both
        spellings from the pattern itself; the restore step is gone, and neither of these
        may start passing for a reason other than the pattern."""
        for cmd in ("grep -r . .charter/vaults/", "grep -r . .charter//vaults//",
                    "grep -r . .charter/vaults", "grep -r . .charter//vaults"):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(hooks._leak_reason(cmd))


class TestTheReadGuardAnswersThePathNotTheSpelling(PersonaIso):
    """The `Read`/`Grep` guard shares the pattern, and shared a hole with it: the Bash
    denial names the path it refused, so the agent's next move is the same path through a
    tool — the exact sequence #90 was filed for."""

    def read(self, path: str, tool: str = "Read", **extra):
        ti = {"file_path": path} if tool == "Read" else {"path": path, **extra}
        return run_hook(hooks.pretooluse_read,
                        {"tool_name": tool, "tool_input": ti, "session_id": "s", "cwd": "/tmp"})

    def test_every_spelling_of_a_vault_file_is_denied(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertEqual("deny", _decision(self.read(respell(VAULT))), name)

    def test_every_spelling_of_the_registry_stays_allowed(self):
        for name, respell in _RESPELLINGS:
            with self.subTest(spelling=name):
                self.assertIsNone(_decision(self.read(respell(REGISTRY))), name)

    def test_a_grep_rooted_at_the_state_directory_is_denied_in_every_spelling(self):
        for path in (".charter", ".charter/", ".charter//", "./.charter",
                     ".CHARTER", ".Charter/", "./.CHARTER"):
            with self.subTest(path=path):
                self.assertEqual("deny",
                                 _decision(self.read(path, tool="Grep", pattern="token")))

    def test_a_grep_rooted_at_the_vault_directory_is_denied_in_every_case(self):
        """The composed decision, not the shared helper — the exact operand a `Grep` names
        when an agent points it at the directory holding every vault file, crossed with the
        case fold. This route once landed via a slash-appending retry private to
        `pretooluse_read`; it now lands via the segment anchor in the shared pattern, which
        is what makes the Bash route answer the same way."""
        for path in (".charter/vaults", ".CHARTER/VAULTS", ".Charter/Vaults",
                     ".charter/VAULTS", ".CHARTER/vaults"):
            with self.subTest(path=path):
                self.assertEqual("deny",
                                 _decision(self.read(path, tool="Grep", pattern="token")),
                                 f"{path} walks past the read guard")

    def test_the_registry_stays_readable_through_the_read_guard_in_every_case(self):
        """Boundary control for the pair above: `vaults` in `vaults.json` is a prefix of a
        path segment, not a segment, so the anchor does not reach it. If this ever denies,
        #443's false positive is back and the widening that did it is right above."""
        for path in (".charter/vaults.json", ".CHARTER/VAULTS.JSON", ".Charter/Vaults.Json"):
            with self.subTest(path=path):
                self.assertIsNone(_decision(self.read(path)), f"{path} is the registry")


class TestEveryCaseSpellingOfTheGuardedPrefix(unittest.TestCase):
    """Case, exhaustively, because a sample is a list of bad inputs with extra steps.

    `.charter/vaults/` is thirteen letters; this enumerates all 2^13 upper/lower spellings of
    them and asserts the guard denies every one. On a case-folding filesystem — macOS/APFS,
    Windows, and therefore this repo's own development machine — each of those names the same
    inode as the canonical form, and before `re.IGNORECASE` the guard denied exactly one of
    the 8192 and allowed the other 8191.

    Run against `_names_a_vault_path` rather than through `_leak_reason`, so 8192 cases cost
    a regex search each instead of a `shlex` parse each; the two `_leak_reason` case entries
    in `_RESPELLINGS` cover the wiring."""

    PREFIX = ".charter/vaults/"
    TAIL = "db.json"

    def _spellings(self):
        letters = [i for i, ch in enumerate(self.PREFIX) if ch.isalpha()]
        for bits in itertools.product((False, True), repeat=len(letters)):
            chars = list(self.PREFIX)
            for i, up in zip(letters, bits):
                chars[i] = chars[i].upper() if up else chars[i].lower()
            yield "".join(chars) + self.TAIL

    def test_the_corpus_is_the_size_it_claims(self):
        """A generator bug that yielded one item would make the next test pass in silence."""
        self.assertEqual(2 ** 13, sum(1 for _ in self._spellings()))

    def test_every_case_spelling_is_denied(self):
        missed = [s for s in self._spellings() if not hooks._names_a_vault_path(s)]
        self.assertEqual([], missed[:5],
                         f"{len(missed)} case spellings of one vault path walk past the guard")

    def test_the_registry_beside_it_survives_every_case_spelling(self):
        """The carve-out is `vaults.json` vs `vaults/`, and case must not blur it: a wider
        pattern that folded the dot into the slash would re-deny the registry, which is
        #443's false positive coming back."""
        for spelling in (".charter/vaults.json", ".CHARTER/VAULTS.JSON",
                         ".Charter/Vaults.Json", ".chARTer/vAULts.json"):
            with self.subTest(spelling=spelling):
                self.assertFalse(hooks._names_a_vault_path(spelling))


#: `origin/main`'s predicate, transcribed. The vault guards on this branch must deny
#: everything main denies — a security branch that denies LESS on any input is a regression
#: wearing a fix's commit message, and this round shipped two of those elsewhere. Kept as a
#: literal rather than imported, because the point is to compare against a FROZEN reference:
#: if someone changes `_VAULT_PATH_RE`, this constant must not change with it.
_MAIN_VAULT_PATH_RE = re.compile(r"\.(?:charter|edm)(?:/(?:vaults/|browser|active-)|/?$)")


class TestThisBranchNeverDeniesLessThanMain(unittest.TestCase):
    """The differential. Not "the new cases are denied" — that is what the rest of the file
    says — but "no input that main refused is now allowed".

    The corpus is generated from every base path this file uses, crossed with every
    respelling, plus the guard's other alternatives (`browser`, `active-`, `.edm`) and a
    block of ordinary paths. `test_the_corpus_actually_exercises_main` is the anti-vacuity
    assertion: a corpus main denies nothing in would make the containment trivially true."""

    #: Includes the shapes where normalisation moves the answer, not only the ones where it
    #: is a no-op. `.charter/vaults/../../elsewhere` matches main as written and normalises
    #: OUT of the plane, so it is exactly the operand a branch that kept only the normalised
    #: arm would hand back — the containment test is worthless without it, and dropping the
    #: raw arm now reddens this class as well as its own.
    BASES = (VAULT, REGISTRY, ORDINARY, ".charter", ".charter/", ".charter/vaults/",
             DIRECTORY, ".edm/vaults", ".charter/vaultsomething", ".charter/vaults.json.bak",
             ".charter/browser/profile", ".charter/active-persona", ".edm/vaults/db.json",
             "/home/me/plane/.charter/vaults/db.json", "charter/hooks.py",
             "docs/charter.md", ".charterhouse/notes.md", "a/.charter/vaults/x.json",
             ".charter/vaults/../../elsewhere", ".charter/vaults/../vaults/db.json",
             ".charter/../.charter/vaults/db.json", ".charter/vaults/./db.json",
             ".charter/..", ".charter/vaults/..")

    def corpus(self) -> list[str]:
        out = []
        for base in self.BASES:
            for _name, respell in _RESPELLINGS:
                out.append(respell(base))
        return out

    def test_the_corpus_actually_exercises_main(self):
        denied = [c for c in self.corpus() if _MAIN_VAULT_PATH_RE.search(c)]
        self.assertGreater(len(denied), 20,
                           "the corpus barely triggers main; containment below proves little")

    def test_every_operand_main_denied_is_still_denied(self):
        weaker = [c for c in self.corpus()
                  if _MAIN_VAULT_PATH_RE.search(c) and not hooks._names_a_vault_path(c)]
        self.assertEqual([], weaker,
                         "this branch allows an operand `origin/main` denied")

    def test_the_read_guard_is_not_weaker_either(self):
        """`pretooluse_read` calls the shared predicate with the target exactly as written
        and adds nothing of its own, so this is the same containment reached the way that
        route reaches it. If a caller-local step is ever reintroduced, this stops being a
        transcription of `pretooluse_read` and `TestTheTwoGuardsCannotDisagree` below is the
        one that fails."""
        weaker = [c for c in self.corpus()
                  if _MAIN_VAULT_PATH_RE.search(c) and not hooks._names_a_vault_path(c)]
        self.assertEqual([], weaker)


class TestTheTwoGuardsCannotDisagree(PersonaIso):
    """The differential that was missing, and the one this round's bypass needed.

    Every other test in this file asks each guard whether it denies the operands *it* was
    written for. That cannot catch the defect that shipped: `grep -rn TOKEN .charter/vaults`
    printed a fabricated password through the Bash route while `Grep(path=".charter/vaults")`
    refused the identical target, because `pretooluse_read` carried a private
    "retry with a `/` appended" step that `_leak_reason` did not. Both files were green. The
    Bash suite had no directory operand; the read suite had one and passed *because of* the
    step that made the two routes differ.

    So this asserts the PROPERTY instead of either guard's own list: **one operand, one
    answer, whichever route it arrives on.** It is deliberately direction-free — it fails on
    a read-route denial the Bash route allows AND on the reverse — because either direction
    is a gap, and the gap that shipped was the one nobody was looking for.

    Anti-vacuity is `test_the_corpus_contains_both_answers`: a corpus that is all-deny or
    all-allow makes agreement free.
    """

    #: Whole COMMANDS, so the Bash side is exercised through `_leak_reason` — shlex, argv
    #: split, reader-name check, `_file_operands` — rather than through the predicate both
    #: sides share. A test that called `_names_a_vault_path` twice would agree by
    #: construction and could never have failed.
    READER = "cat"

    def corpus(self) -> list[str]:
        out = []
        for base in TestThisBranchNeverDeniesLessThanMain.BASES:
            for _name, respell in _RESPELLINGS:
                out.append(respell(base))
        return out

    def _bash_denies(self, operand: str) -> bool:
        return hooks._leak_reason(f"{self.READER} {operand}") is not None

    def _read_denies(self, operand: str) -> bool:
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Read", "tool_input": {"file_path": operand},
                      "session_id": "s", "cwd": "/tmp"})
        return _decision(r) == "deny"

    def test_the_corpus_contains_both_answers(self):
        answers = {self._bash_denies(c) for c in self.corpus()}
        self.assertEqual({True, False}, answers,
                         "agreement is vacuous unless the corpus is answered both ways")

    def test_every_operand_gets_the_same_answer_on_both_routes(self):
        split = [(c, self._bash_denies(c), self._read_denies(c)) for c in self.corpus()]
        split = [(c, b, r) for c, b, r in split if b != r]
        self.assertEqual([], split[:5],
                         f"{len(split)} operands get two answers depending on the route")

    def test_the_directory_operand_specifically(self):
        """Named on its own, not left to the generated corpus, because it is the whole
        finding: the operand that names the directory holding every vault file, spelled
        without the trailing slash the pattern used to demand."""
        for spelling in (DIRECTORY, ".charter//vaults", ".charter/./vaults",
                         "./" + DIRECTORY, ".CHARTER/vaults", ".charter/x/../vaults",
                         ".edm/vaults"):
            with self.subTest(spelling=spelling):
                self.assertTrue(self._bash_denies(spelling),
                                f"`{self.READER} {spelling}` walks past the Bash guard")
                self.assertTrue(self._read_denies(spelling),
                                f"Read({spelling}) walks past the read guard")

    def test_a_recursive_grep_of_the_directory_is_denied_by_name(self):
        """The exact command the reviewer ran. `grep -rn <pat> <dir>` puts the directory in
        the SECOND positional, so this also pins that `_file_operands` still hands it over
        after consuming the pattern — a route the `cat` corpus above never takes."""
        self.assertIsNotNone(hooks._leak_reason(f"grep -rn TOKEN {DIRECTORY}"))
        self.assertIsNotNone(hooks._leak_reason(f"rg -n TOKEN {DIRECTORY}"))
        self.assertIsNone(hooks._leak_reason(f"grep -rn {DIRECTORY} docs/"),
                          "naming the directory as a PATTERN is not reading it")

    def test_the_registry_is_the_boundary_control_on_both_routes(self):
        """`vaults` anchored to a path SEGMENT, not to a prefix. If the anchor ever slips to
        "starts with vaults", every one of these starts denying and #443's false positive is
        back — on both routes at once, which is why it is asserted on both."""
        for spelling in (REGISTRY, ".charter/vaults.json.bak", ".charter/vaultsomething",
                         ".CHARTER/VAULTS.JSON"):
            with self.subTest(spelling=spelling):
                self.assertFalse(self._bash_denies(spelling), spelling)
                self.assertFalse(self._read_denies(spelling), spelling)


class TestWhatStillWalksPastBothGuards(unittest.TestCase):
    """The limit this round did NOT close, pinned so the docs cannot quietly outgrow it.

    Anchoring `vaults` to a segment closes every spelling of an operand that NAMES the vault
    directory. It does nothing for an operand that merely CONTAINS it: `grep -r TOKEN .`
    walks every vault file and names none of them. That is the same limit `pretooluse_read`'s
    docstring already states for `Grep` — denying every broad search is untenable — and
    `docs/secrets.md` now states it for the Bash route in the same words, where the
    completeness claim is made.

    Pinned as BEHAVIOUR, not as a wish: if someone later denies these, this test fails next
    to the paragraph that has to change with it."""

    def test_a_search_rooted_above_the_plane_is_allowed_on_both_routes(self):
        for operand in (".", "..", "./", "/", "~", "$PWD"):
            with self.subTest(operand=operand):
                self.assertIsNone(hooks._leak_reason(f"grep -rn TOKEN {operand}"))
                self.assertFalse(hooks._names_a_vault_path(operand))


class TestSeparatorMeansTheForwardSlash(unittest.TestCase):
    """`docs/` and `SECURITY.md` say "separators" — this pins which separator they mean.

    `_VAULT_PATH_RE` and `os.path.normpath` are both POSIX `/`. A Windows-style
    `.charter\\vaults\\db.json` names the same file on a Windows filesystem and a
    DIFFERENT one on POSIX, where a backslash is an ordinary filename character. Charter
    does not fold it, and the docs say `/` rather than "separators" for that reason. Filed
    separately rather than closed here: folding `\\` on POSIX would deny real filenames.
    """

    def test_a_backslash_spelling_is_not_folded(self):
        self.assertFalse(hooks._names_a_vault_path(".charter\\vaults\\db.json"))

    def test_and_the_forward_slash_spelling_of_the_same_path_is_denied(self):
        """Positive control: the assertion above is about the separator, not about the test
        having mistyped the path."""
        self.assertTrue(hooks._names_a_vault_path(".charter/vaults/db.json"))


if __name__ == "__main__":
    unittest.main()
