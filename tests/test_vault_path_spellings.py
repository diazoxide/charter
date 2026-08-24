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

**And the operand that CONTAINS the vault directory without naming it** — `grep -rn TOKEN .`
from the plane root, which reads every vault file and names none of them. That was pinned
here as a limit for two rounds and is closed in round five (#474) by a SECOND predicate
rather than by widening this one: text about a path cannot answer a question about a walk.
`TestAnOperandThatContainsTheVaultDirectoryIsRefused` holds the property, and
`TestTheWalkGuardDoesNotOverreach` holds the half that makes it liveable — a broad search
that reaches no vault, and the exclusion the denial recommends, both still run.

**What is deliberately NOT closed, pinned as behaviour rather than left to be rediscovered.**
`TestWhatStillWalksPastBothGuards` — a program charter does not know walks directories
(`find … -exec cat`, `tar`), an interpreter's argument, and a `cd` earlier in the same
command. `TestSeparatorMeansTheForwardSlash` — a Windows-style backslash spelling (#476),
which stays unfolded because charter's harness targets POSIX and is not supported on
Windows. All are stated as limits in `SECURITY.md`, `docs/secrets.md` and
`skills/secrets/SKILL.md`, and if any ever starts being denied these tests fail next to the
paragraph that has to move with it.

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


class WalkCase(PersonaIso):
    """An isolated plane whose vault directory HOLDS SOMETHING.

    `PersonaIso` is not optional here and the class it replaced went without it. The old
    `TestWhatStillWalksPastBothGuards` was a bare `TestCase` asserting that
    `grep -rn TOKEN .` is allowed — an assertion that reached `config.STATE_DIR` for the
    REAL plane, and in a linked worktree `root.find_root()` redirects that to the operator's
    main tree. It passed because the answer was "allowed" everywhere; the moment the answer
    depends on what is on disk, a test without isolation is a test that reads someone's
    actual `.charter/`.

    The fabricated file matters too: `_guarded_state_entries` deliberately ignores an EMPTY
    guarded directory, because a fresh plane has one and refusing every broad search there
    protects nothing. So a fixture that only `mkdir`-ed would make every denial below
    unreachable and every allowance below unfalsifiable.
    """

    def setUp(self) -> None:
        super().setUp()
        from charter import config
        self.state = config.STATE_DIR
        self.vaults = self.state / "vaults"
        self.vaults.mkdir(parents=True, exist_ok=True)
        # A fabricated name and a fabricated value; nothing here is a credential.
        (self.vaults / "db.json").write_text('{"password": "FABRICATED-not-real-9f3a"}\n')
        self.root = config.ROOT

    def bash(self, cmd: str, cwd=None) -> str | None:
        return hooks._leak_reason(cmd, str(cwd if cwd is not None else self.root))

    def grep_tool(self, cwd=None, **ti) -> str | None:
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Grep", "tool_input": ti, "session_id": "s",
                      "cwd": str(cwd if cwd is not None else self.root)})
        return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason")


class TestAnOperandThatContainsTheVaultDirectoryIsRefused(WalkCase):
    """The limit round three pinned and round five closed (#474).

    Anchoring `vaults` to a path segment closed every spelling of an operand that NAMES the
    vault directory. It did nothing for an operand that merely CONTAINS it: `grep -rn TOKEN
    .` from the plane root printed every vault file and named none of them, and both guards
    stood aside because both decided on the TEXT of the operand.

    **The property is "the walk reaches the directory", and that is what is tested.** Not a
    list of spellings of "here" — `.`, `..`, an absolute path, a path through a symlinked
    parent and a path with dot segments in it are all the same ancestor, and the six
    bypasses this guard family has had were every one of them a literal set. The operand is
    resolved against the shell's directory and compared by ancestry, and the thing it is
    compared against is asked of the filesystem rather than matched as text — so a plane
    whose `$CHARTER_HOME` puts the vaults somewhere no pattern can spell is covered by the
    same code.

    What it costs, deliberately: a broad search from the plane root is now refused, with the
    exclusion that fixes it in the message. `SECURITY.md` used to call that untenable. The
    trade is revisited on purpose here — the denial is scoped to a plane that has vault
    FILES, it is one flag away from running, and the thing it prevents is plaintext in a
    transcript.
    """

    def test_the_reported_command(self):
        """The exact reproduction in #474: a recursive grep of the plane root."""
        reason = self.bash("grep -rn TOKEN .")
        self.assertIsNotNone(reason)
        self.assertIn("vaults", reason)
        self.assertIn("--exclude-dir", reason)

    def test_every_spelling_of_the_same_ancestor(self):
        """One property, so the spellings are varied and the answer must not."""
        for operand in (".", "./", "..", "./.", ".//", "x/..", str(self.root),
                        str(self.root) + "/", str(self.root) + "/./"):
            with self.subTest(operand=operand):
                self.assertIsNotNone(self.bash(f"grep -rn TOKEN {operand}"), operand)

    def test_a_symlinked_route_to_the_same_directory(self):
        """`resolve()` is what makes this a question about the directory rather than about
        the string, and a symlink is the cheapest proof that the two differ."""
        link = self.tmp / "link-to-plane"
        link.symlink_to(self.root)
        self.assertIsNotNone(self.bash(f"grep -rn TOKEN {link}"))

    def test_a_walker_that_needs_no_flag_at_all(self):
        """`rg` and `ag` recurse by default, so gating on `-r` would have been a guard
        against one program's spelling of a property two programs have."""
        for cmd in ("rg TOKEN", "rg TOKEN .", "ag TOKEN", "ag TOKEN ."):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(self.bash(cmd), cmd)

    def test_every_spelling_of_grep_recursion(self):
        for cmd in ("grep -r TOKEN .", "grep -R TOKEN .", "grep --recursive TOKEN .",
                    "grep --dereference-recursive TOKEN .", "grep -rn TOKEN .",
                    "grep -nr TOKEN .", "grep -d recurse TOKEN .",
                    "grep --directories=recurse TOKEN ."):
            with self.subTest(cmd=cmd):
                self.assertIsNotNone(self.bash(cmd), cmd)

    def test_a_recursive_grep_with_no_operand_searches_here(self):
        """`grep -r PATTERN` and `rg PATTERN` both default to the cwd, so "no path was
        named" is a path — and it is the one an agent standing in the plane root types."""
        self.assertIsNotNone(self.bash("grep -r TOKEN"))
        self.assertIsNotNone(self.bash("rg TOKEN"))

    def test_it_follows_the_shell_and_not_the_hook_process(self):
        """A relative operand belongs to the SHELL's directory. Judged from a workspace
        clone, `../..` is the plane root — the same class of defect `_git_target` had."""
        clone = self.root / "workspaces" / "ws" / "svc"
        clone.mkdir(parents=True, exist_ok=True)
        import os as _os
        up = _os.path.relpath(self.root, clone)
        self.assertIsNotNone(self.bash(f"grep -rn TOKEN {up}", cwd=clone))
        self.assertIsNone(self.bash("grep -rn TOKEN .", cwd=clone),
                          "a search of the clone reaches no vault and must stay allowed")

    def test_the_grep_tool_route_answers_the_same_way(self):
        """#462's lesson: one operand, one answer, whichever route it arrives on."""
        self.assertIsNotNone(self.grep_tool(pattern="TOKEN", path="."))
        self.assertIsNotNone(self.grep_tool(pattern="TOKEN"), "no path means the cwd")
        self.assertIsNotNone(self.grep_tool(pattern="TOKEN", path=str(self.root)))


class TestTheWalkGuardDoesNotOverreach(WalkCase):
    """The other half, and the reason the guard is scoped the way it is. Every denial that
    is not needed is a denial that gets the guard switched off."""

    def test_a_non_recursive_read_of_the_same_directory_is_untouched(self):
        """`grep -n TOKEN .` without recursion reads the directory and no file in it — GNU
        grep answers "Is a directory". Nothing leaks, so nothing is refused."""
        self.assertIsNone(self.bash("grep -n TOKEN ."))
        self.assertIsNone(self.bash("cat ."))

    def test_a_search_of_a_sibling_directory_is_untouched(self):
        (self.root / "docs").mkdir(exist_ok=True)
        for cmd in ("grep -rn TOKEN docs", "grep -rn TOKEN ./docs", "rg TOKEN docs"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(self.bash(cmd), cmd)

    def test_a_search_outside_the_plane_is_untouched(self):
        outside = self.tmp / "elsewhere"
        outside.mkdir()
        self.assertIsNone(self.bash(f"grep -rn TOKEN {outside}", cwd=outside))

    def test_the_exclusion_the_denial_recommends_actually_runs(self):
        """A guard that refuses the command it recommends teaches people to route around
        it — the lesson the plane-root guard's remedy carve-out records. So the fix in the
        message is executed here, in every spelling the message offers."""
        for cmd in ("grep -rn --exclude-dir=.charter TOKEN .",
                    "grep -rn --exclude-dir .charter TOKEN .",
                    "grep -rn --exclude-dir=vaults TOKEN .",
                    "grep -rn --exclude-dir='.charter*' TOKEN .",
                    "rg --glob '!.charter' TOKEN .",
                    "rg -g '!.charter/**' TOKEN ."):
            with self.subTest(cmd=cmd):
                self.assertIsNone(self.bash(cmd), cmd)

    def test_an_empty_vault_directory_is_not_worth_a_denial(self):
        """A fresh plane has `vaults/` and nothing in it. Refusing every broad search there
        protects nothing and spends the guard's credibility."""
        (self.vaults / "db.json").unlink()
        self.assertIsNone(self.bash("grep -rn TOKEN ."))

    def test_a_narrowed_grep_tool_call_is_answered_by_looking(self):
        """`Grep` has no exclude, so its own narrowing stands in for one — and whether a
        glob reaches a vault is decided by looking inside the directory, not by reading the
        pattern. `*.json` does select the fixture; `*.py` does not."""
        self.assertIsNone(self.grep_tool(pattern="TOKEN", path=".", glob="*.py"))
        self.assertIsNotNone(self.grep_tool(pattern="TOKEN", path=".", glob="*.json"))

    def test_the_registry_is_the_boundary_control_for_the_WALK_predicate_too(self):
        """#443's false positive, one predicate over.

        `.charter/vaults.json` is the registry — provider config and file paths, never a
        value — and `_VAULT_PATH_RE` anchors `vaults` to a path SEGMENT so that it stays an
        ordinary read. The walk predicate has its own copy of that question ("which entries
        under the state directory are guarded?"), and its first cut asked
        `name.startswith(("vaults", …))`, which matched the registry and started refusing
        `ag TOKEN .charter/vaults.json` — a command `origin/main` allows.

        The sibling test one class up asserts this through `cat`, which does not walk, so it
        could not have caught it. This one uses a walker on purpose.
        """
        registry = self.state / ("vaul" + "ts.json")
        registry.write_text("{}\n")
        for cmd in (f"ag TOKEN {registry}", f"rg TOKEN {registry}",
                    f"grep -rn TOKEN {registry}"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(self.bash(cmd), cmd)
        self.assertIsNone(self.grep_tool(pattern="TOKEN", path=str(registry)))

    def test_an_exclusion_naming_the_vault_directory_still_lets_the_search_run(self):
        """The same bug had a second face: with the registry counted as a guarded entry,
        `--exclude-dir=vaults` excluded one target and not the other, so the remedy stopped
        working while the denial kept recommending it."""
        (self.state / ("vaul" + "ts.json")).write_text("{}\n")
        self.assertIsNone(self.bash("grep -rn --exclude-dir=vaults TOKEN ."))

    def test_read_does_not_walk(self):
        """`Read` opens one file. It cannot reach a vault it did not name, and the named
        case is the other predicate's."""
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Read", "tool_input": {"file_path": "README.md"},
                      "session_id": "s", "cwd": str(self.root)})
        self.assertIsNone(_decision(r))


class TestWhatStillWalksPastBothGuards(WalkCase):
    """The limits round five did NOT close, pinned so the docs cannot quietly outgrow them.

    Closing #474 moved the boundary; it did not remove it. Each of these really does read a
    vault file, and each is allowed. They are the `_READERS` ceiling and the shell residual
    in a new shape, and both are stated in `SECURITY.md` and `docs/secrets.md` — if one ever
    starts being denied, this test fails next to the paragraph that has to change with it.
    """

    def test_a_reader_charter_does_not_know_walks_unguarded(self):
        """The ceiling `_READERS` already has: `_TREE_WALKERS` is a subset of it, and a name
        missing from either is a read the guard does not see. Widening the list does not fix
        this — the next name is always the missing one."""
        for cmd in ("find . -type f -exec cat {} +", "tar cf - .",
                    "python3 -c \"import pathlib;print(pathlib.Path('.').rglob('*'))\""):
            with self.subTest(cmd=cmd):
                self.assertIsNone(self.bash(cmd), cmd)

    def test_an_interpreters_argument_is_text_and_is_not_re_parsed(self):
        self.assertIsNone(self.bash("sh -c 'grep -rn TOKEN .'"))

    def test_a_cd_earlier_in_the_command_is_not_followed_by_this_guard(self):
        """`_plane_root_git` follows a `cd`; `_leak_reason` does not, and that difference is
        older than this change. Recorded here rather than half-fixed on the hot path."""
        outside = self.tmp / "elsewhere"
        outside.mkdir()
        self.assertIsNone(self.bash(f"cd {self.root} && grep -rn TOKEN .", cwd=outside))


class TestSeparatorMeansTheForwardSlash(unittest.TestCase):
    """`docs/` and `SECURITY.md` say "separators" — this pins which separator they mean.

    `_VAULT_PATH_RE` and `os.path.normpath` are both POSIX `/`. A Windows-style
    `.charter\\vaults\\db.json` names the same file on a Windows filesystem and a
    DIFFERENT one on POSIX, where a backslash is an ordinary filename character. Charter
    does not fold it, and the docs say `/` rather than "separators" for that reason.

    **#476 asked whether that should change, and the answer is no, on the prerequisite the
    issue itself named: charter's harness does not run on Windows.** It is a tmux program —
    `charter claude` builds and drives a tmux session, the frame repaints panes, and the
    suite is run on macOS and Ubuntu — and it writes its vaults at `0o600`, a mode Windows
    does not have. There is no Windows CI and no Windows install path. Folding `\\` would
    therefore buy nothing on any host charter supports, and would cost a real denial on
    POSIX filenames that legitimately contain a backslash; a platform-CONDITIONAL fold
    would buy the same nothing at the price of making the guard's answer depend on the host,
    which is the exact property `re.IGNORECASE` was added to remove. `toolgate._norm` made
    the same call for the same reason in #443, where an unconditional fold INVENTED a
    spelling and matched nothing.

    So this is pinned as a decision, not as a gap, and `SECURITY.md` and `docs/secrets.md`
    now say "charter's harness targets POSIX" where they used to name Windows as a reason.
    The day a Windows harness exists, this test is where the change starts.
    """

    def test_a_backslash_spelling_is_not_folded(self):
        self.assertFalse(hooks._names_a_vault_path(".charter\\vaults\\db.json"))

    def test_and_the_forward_slash_spelling_of_the_same_path_is_denied(self):
        """Positive control: the assertion above is about the separator, not about the test
        having mistyped the path."""
        self.assertTrue(hooks._names_a_vault_path(".charter/vaults/db.json"))


if __name__ == "__main__":
    unittest.main()
