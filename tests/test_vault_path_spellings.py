"""Two spellings of one path got two answers out of the vault guards.

`cat .charter/vaults/db.json` was denied. `cat .charter//vaults/db.json` was allowed — one
extra separator, the identical file to the kernel, no wrapper, no interpreter, no program
off the allowlist. `.charter/./vaults/db.json` likewise. This is not the documented
`_READERS` ceiling (a program the guard does not know) and not the documented path-spelling
limit (a path *you* chose, outside the plane): it is the guard failing on the one path it
claims to cover, which is what `SECURITY.md` and `docs/hooks.md` now promise it catches.

**The property, stated no wider than what is tested here.** The guards answer on the TEXT
OF THE OPERAND AS WRITTEN, and this file pins that they are complete over the three ways
that text can vary while still naming the same file *without a shell's help*: redundant
separators, dot segments, and LETTER CASE. So the tests generate those spellings
mechanically and assert every one lands where the canonical form lands — denied for a vault
file, allowed for the registry and for ordinary work. A new separator or case trick is a new
element of `_RESPELLINGS`, not a new bypass.

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

    def test_a_directory_operand_keeps_its_trailing_slash_denial(self):
        """`normpath` strips a trailing slash and `_VAULT_PATH_RE` requires it, so testing
        only the normalised form would have quietly re-opened the recursive grep that walks
        every vault file. Both forms are tested; this is the case that proves it."""
        self.assertIsNotNone(hooks._leak_reason("grep -r . .charter/vaults/"))
        self.assertIsNotNone(hooks._leak_reason("grep -r . .charter//vaults//"))


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
        """The composed decision, not the shared helper. `.charter/vaults` carries no
        trailing slash and the pattern requires one, so this route only lands because
        `pretooluse_read` retries the target with a `/` appended — and that retry has to keep
        working through the case fold, on the exact operand a `Grep` names when an agent
        points it at the directory holding every vault file."""
        for path in (".charter/vaults", ".CHARTER/VAULTS", ".Charter/Vaults",
                     ".charter/VAULTS", ".CHARTER/vaults"):
            with self.subTest(path=path):
                self.assertEqual("deny",
                                 _decision(self.read(path, tool="Grep", pattern="token")),
                                 f"{path} walks past the read guard")

    def test_the_registry_stays_readable_through_the_read_guard_in_every_case(self):
        """Boundary control for the pair above: the retry appends a slash to `vaults.json`
        too, and `vaults.json/` still has no `vaults/` in it. If this ever denies, #443's
        false positive is back and the case fold is what widened it."""
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
        """`pretooluse_read` shares the predicate but adds its own trailing-slash retry, so
        the containment is asserted on the composed decision, not on the shared helper."""
        weaker = [c for c in self.corpus()
                  if _MAIN_VAULT_PATH_RE.search(c)
                  and not (hooks._names_a_vault_path(c) or hooks._names_a_vault_path(c + "/"))]
        self.assertEqual([], weaker)


if __name__ == "__main__":
    unittest.main()
