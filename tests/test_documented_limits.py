"""The vault guards' documented limits, pinned as behaviour so the docs cannot drift (#423).

Four rounds of adversarial review established that these six cannot be closed in code:
every added parser was defeated by a new spelling, and one attempt shipped a regression. So
they are *documented* — in `SECURITY.md`, `docs/hooks.md` and `docs/secrets.md`, and, for
the one reader who cannot go and check, in `skills/secrets/SKILL.md`.

A documented limit rots in one specific way: someone narrows or widens the behaviour and
the sentence stays. These tests are the tripwire for that. **Every assertion here is an
`allow`, which is the unusual direction for a security suite and is the point** — each one
says "the docs promise nothing here", and a failure means the code moved and a paragraph
now over- or under-claims. A failure is therefore not "you broke security": it is "go read
the doc named in the docstring and make it true again".

Each class carries a positive control, because a test whose every assertion is `is None`
passes just as well against a guard that was deleted.

The six, and where each is written down:

1. Redaction is `str.replace`, not a boundary — `SECURITY.md`, `docs/secrets.md`,
   `skills/secrets/SKILL.md`.
2. `--exec`/`--stream` capture nothing and therefore redact nothing — same three.
3. `_READERS` is a name allowlist with a ceiling — `SECURITY.md`, `docs/hooks.md`.
4. `sh -c '<string>'` is not re-parsed — `docs/hooks.md` (and
   `test_leak_guard_readers_that_write` pins the sibling `bash <<EOF` case).
5. A path the guard's pattern cannot spell — a vault registered outside `.charter/`, and a
   file `charter secret cp` wrote where you asked — is unguarded: `docs/secrets.md`.
6. Everything a SHELL does to an operand between the guard's decision and the kernel's —
   glob, `$VAR`, `$(…)`, `~`, brace expansion, and a preceding `cd` — happens on text the
   guard never sees: all four docs, and `TestAShellExpandsAfterTheGuardHasAnswered` below.
"""

from __future__ import annotations

import base64
import glob as globmod
import os
import shutil
import subprocess
import tempfile
import unittest

from charter import hooks
from charter.secrets import base
from tests._isolation import PersonaIso, run_hook

#: Fabricated. Never a real credential in a fixture, and never in an assertion message.
VALUE = "FABRICATED-not-real-9f3a"


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TestRedactionIsASubstringNet(unittest.TestCase):
    """`charter/secrets/base.py` — `text.replace(value, "***")`.

    It catches the accident it was built for and nothing else. Any transform of the value
    survives it, and no scrubber can win that race: the next encoding is always available.
    Documented as a net rather than a boundary in `SECURITY.md`."""

    def test_it_masks_a_plain_echo(self):
        """Positive control: the case redaction exists for. If this ever fails, the rest of
        this class is asserting nothing."""
        self.assertEqual("token=***", base.redact(f"token={VALUE}", [VALUE]))

    def test_base64_of_the_value_comes_back_whole(self):
        encoded = base64.b64encode(VALUE.encode()).decode()
        self.assertEqual(encoded, base.redact(encoded, [VALUE]))

    def test_a_reversed_value_comes_back_whole(self):
        self.assertEqual(VALUE[::-1], base.redact(VALUE[::-1], [VALUE]))

    def test_one_character_per_line_comes_back_whole(self):
        folded = "\n".join(VALUE)
        self.assertEqual(folded, base.redact(folded, [VALUE]))

    def test_a_value_split_across_a_line_break_comes_back_whole(self):
        """`fold -w40`, a wrapped header, a JSON pretty-printer: the bytes are all there and
        `str.replace` sees no occurrence."""
        split = VALUE[:8] + "\n" + VALUE[8:]
        self.assertEqual(split, base.redact(split, [VALUE]))


class TestUnredactedExecModesSayItInTheirOwnHelp(unittest.TestCase):
    """`--exec` and `--stream` hand the child charter's stdio, so charter captures nothing
    and there is nothing to redact. That is not a gap to fix — it is what those modes are —
    and the requirement is that the CLI says so where someone reads it.

    Pinned on the argparse help rather than on prose in a doc, because the help is the text
    the person running the command sees."""

    def _exec_help(self, flag: str) -> str:
        from charter import cli
        found: list[str] = []

        def walk(parser):
            for action in parser._actions:                   # noqa: SLF001
                if flag in (action.option_strings or []):
                    found.append(action.help or "")
                choices = getattr(action, "choices", None)
                for p in (choices.values() if isinstance(choices, dict) else ()):
                    if hasattr(p, "_actions"):
                        walk(p)
        walk(cli.build_parser())
        self.assertTrue(found, f"{flag} is no longer a flag on any subcommand")
        return " ".join(found)

    def test_stream_help_says_output_is_not_redacted(self):
        self.assertIn("NOT redacted", self._exec_help("--stream"))

    def test_exec_help_says_output_is_not_redacted(self):
        self.assertIn("NOT redacted", self._exec_help("--exec"))

    def test_the_capturing_path_is_the_one_that_redacts(self):
        """Positive control for the pair above: plain `secret exec` captures, so it redacts.
        Without this, both assertions would still pass if redaction were removed entirely."""
        self.assertEqual("***", base.redact(VALUE, [VALUE]))


class TestTheReaderAllowlistHasACeiling(unittest.TestCase):
    """`hooks._READERS` is 16 program names. A program that reads a file without being
    called a reader is not covered, and the list is deliberately not widened: the missing
    name is always the next one, and every added name buys false positives on ordinary work.

    Stated in those words in `SECURITY.md` and `docs/hooks.md`. If one of these starts
    denying, the paragraph that says it does not must move in the same commit."""

    GUARDED = ".charter/vaults/db.json"

    def allowed(self, cmd: str) -> bool:
        return hooks._leak_reason(cmd) is None

    def test_the_accident_it_exists_for_is_still_denied(self):
        """Positive control. `cat` on a vault file is the whole point of the guard; if this
        fails, every `allowed()` below is measuring a guard that no longer runs."""
        reason = hooks._leak_reason(f"cat {self.GUARDED}")
        self.assertIsNotNone(reason)
        self.assertIn("reads a vault/secret file directly", reason)

    def test_an_interpreter_reads_it(self):
        self.assertTrue(self.allowed(f'python3 -c "print(open({self.GUARDED!r}).read())"'))

    def test_a_program_that_reads_without_being_called_a_reader(self):
        for cmd in (f"base64 {self.GUARDED}",
                    f"cp {self.GUARDED} /tmp/x",
                    f"jq . {self.GUARDED}",
                    f"cut -d: -f2 {self.GUARDED}",
                    f"dd if={self.GUARDED}"):
            with self.subTest(cmd=cmd.split()[0]):
                self.assertTrue(self.allowed(cmd))

    def test_git_reads_it_out_of_the_object_store(self):
        self.assertTrue(self.allowed(f"git show HEAD:{self.GUARDED}"))

    def test_a_redirection_reads_it_with_no_reader_named(self):
        self.assertTrue(self.allowed(f"tr a b < {self.GUARDED}"))


class TestAShellStringIsNotReParsed(unittest.TestCase):
    """The guard inspects the argv it is given. `sh -c '<string>'` is one argument to `sh`,
    and re-parsing it would mean writing a shell — the parser this suite has now watched
    lose four times.

    `tests/test_leak_guard_readers_that_write.py` pins the sibling `bash <<EOF` case for the
    same reason. `docs/hooks.md` says it in prose so it is not only in a test docstring."""

    def test_sh_dash_c_carrying_a_read_is_allowed(self):
        self.assertIsNone(hooks._leak_reason("sh -c 'cat .charter/vaults/db.json'"))

    def test_the_same_command_unwrapped_is_denied(self):
        """Positive control: the difference is the wrapper, not the payload."""
        self.assertIsNotNone(hooks._leak_reason("cat .charter/vaults/db.json"))


class TestAPathThePatternCannotSpellIsUnguarded(PersonaIso):
    """Both guards recognise a vault by its **path** — `_VAULT_PATH_RE`, `.charter/…`.

    Two consequences charter's own documentation now states rather than implying otherwise:

    * A plain-file vault registered outside `.charter/` is an ordinary file to both guards.
      That is the direct cost of the remedy `charter vault add` prints when the default
      location would be committed, and `docs/secrets.md` says so next to that remedy.
    * A file `charter secret cp` materialised at a path the caller named is an ordinary file
      too (#423). A ledger of those paths was considered and is the same shape of guard as
      `_READERS` — it matches a spelling, so `/tmp/./x`, a hardlink, a copy or
      `python3 -c open(...)` walks past it, at the price of a registry read on a hot path.
      What changed instead is the denial text, pinned below, which used to send the agent to
      `cp` as the way to *get at* a value."""

    CP_DEST = "/tmp/kubeconfig-materialised"
    OUTSIDE = "/home/me/creds/devops.json"

    def read(self, path: str):
        return run_hook(hooks.pretooluse_read,
                        {"tool_name": "Read", "tool_input": {"file_path": path},
                         "session_id": "s", "cwd": "/tmp"})

    def test_the_bash_guard_still_denies_the_path_it_can_spell(self):
        """Positive control for both halves of this class."""
        self.assertIsNotNone(hooks._leak_reason("cat .charter/vaults/db.json"))
        self.assertEqual("deny", _decision(self.read(".charter/vaults/db.json")))

    def test_a_vault_outside_the_plane_is_not_covered_by_either_guard(self):
        self.assertIsNone(hooks._leak_reason(f"cat {self.OUTSIDE}"))
        self.assertIsNone(_decision(self.read(self.OUTSIDE)))

    def test_a_materialised_secret_is_not_covered_by_either_guard(self):
        self.assertIsNone(hooks._leak_reason(f"cat {self.CP_DEST}"))
        self.assertIsNone(_decision(self.read(self.CP_DEST)))


class TestAShellExpandsAfterTheGuardHasAnswered(unittest.TestCase):
    """The sixth limit, and the one the other five kept being mistaken for.

    `_names_a_vault_path` decides on the TEXT OF AN OPERAND AS WRITTEN. A shell rewrites
    that text — glob expansion, parameter expansion, command substitution, brace and tilde
    expansion, and the working directory a preceding `cd` moved — strictly *after* the hook
    has already returned its decision, on bytes the hook was never shown. So `cat
    .charter/vault?/db.json` and `V=.charter/vaults/db.json; cat $V` reach the identical
    inode as the denied form, through `cat`, one keystroke apart, and are allowed.

    **The boundary is exact, and writing the test found it.** A metacharacter only escapes
    the guard when it lands INSIDE the guarded prefix `.charter/vaults/`, because that is the
    run of text `_VAULT_PATH_RE` has to see literally. `cat .charter/vaults/*.json` keeps the
    prefix intact and is still denied; `cat .charter/vault?/db.json` breaks it and is not.
    The first drafts of this class asserted `*.json` was a bypass and the `expand_to` half
    of `test_a_glob_reaches_the_guarded_file_and_is_allowed` refused it — which is the reason
    both halves are here. The docs state the limit in this shape, not as "globs are not
    covered", because the wider sentence would be an *under*-claim and those rot too.

    **What is the next spelling of this that still gets through?** Any construct that keeps
    the prefix from appearing literally in the operand: `[s]`, `*`, `.cha*ter`, `${V}`,
    ``P=`printf %s .charter/vaults`; cat $P/db.json``, `{vaults,x}`, `~/plane/.charter/…`,
    `cd` then a bare name, an `IFS` split, a `$'\\x2e'` byte escape. Each is a different
    construct of one language, and the guard closes none of them, because closing one means
    implementing a shell one construct at a time — which leaves a hole shaped like whichever
    construct came next. `charter/hooks.py::_names_a_vault_path` says so in the docstring
    that draws the line, and all four documents say it in prose.

    **These tests do not merely assert `allow`.** Each one first PROVES the operand reaches
    the guarded file — the glob cases by expanding them against a real fixture tree, the
    `$VAR`, `cd`, substitution and brace cases by running the command in a real shell and
    reading the fabricated value out of its stdout. An `assertIsNone` on its own would keep
    passing against a guard that had been deleted, and would keep passing if the spelling
    were simply wrong — as two of these spellings were.
    """

    VAULT = ".charter/vaults/db.json"

    #: Generated, not listed: each entry rewrites the canonical path into an operand a shell
    #: resolves back to it, with the metacharacter INSIDE the guarded prefix. The `glob`
    #: assertion beside it is what keeps the list from drifting into wishful thinking.
    GLOBS = (
        ("single-character wildcard", lambda p: p.replace("vaults", "vault?", 1)),
        ("bracket class", lambda p: p.replace("vaults", "vault[s]", 1)),
        ("star in the vault segment", lambda p: p.replace("vaults", "vault*", 1)),
        ("star in the state segment", lambda p: p.replace(".charter", ".cha*ter", 1)),
        ("negated bracket class", lambda p: p.replace("vaults", "vault[!x]", 1)),
        ("both segments", lambda p: p.replace(".charter/vaults", ".charte?/vault*", 1)),
    )

    #: The other side of the same boundary: the prefix survives, so the guard still sees it.
    GLOBS_OUTSIDE_THE_PREFIX = (
        ("star for the filename", lambda p: p.replace("db.json", "*.json", 1)),
        ("star for the extension", lambda p: p.replace("db.json", "db.*", 1)),
        ("bracket class in the filename", lambda p: p.replace("db.json", "d[b].json", 1)),
    )

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="charter-glob-limit-")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        target = os.path.join(self.tmp, self.VAULT)
        os.makedirs(os.path.dirname(target))
        with open(target, "w") as fh:
            fh.write('{"API_TOKEN": "%s"}\n' % VALUE)

    def _sh(self, cmd: str) -> str:
        return subprocess.run(["sh", "-c", cmd], cwd=self.tmp, capture_output=True,
                              text=True, timeout=30).stdout

    def test_the_canonical_spelling_is_denied_and_the_fixture_holds_the_value(self):
        """Positive control for the whole class, on both halves: the guard still denies the
        path as written, and the fixture this class globs at really does hold the value —
        so an `allow` below is a reachable read, not a miss on an empty tree."""
        self.assertIsNotNone(hooks._leak_reason(f"cat {self.VAULT}"))
        self.assertIn(VALUE, self._sh(f"cat {self.VAULT}"))

    def test_a_glob_reaches_the_guarded_file_and_is_allowed(self):
        for name, respell in self.GLOBS:
            with self.subTest(spelling=name):
                pattern = respell(self.VAULT)
                self.assertEqual([self.VAULT], sorted(globmod.glob(pattern, root_dir=self.tmp)),
                                 f"{name} does not expand to the vault file; fix the test")
                self.assertIsNone(hooks._leak_reason(f"cat {pattern}"),
                                  f"{name} is now denied — the docs must stop calling it a limit")

    def test_a_glob_that_leaves_the_guarded_prefix_intact_is_still_denied(self):
        """The boundary control. Without this the class reads as "globs walk past", which is
        an under-claim: the guard sees `.charter/vaults/` here and answers on it."""
        for name, respell in self.GLOBS_OUTSIDE_THE_PREFIX:
            with self.subTest(spelling=name):
                pattern = respell(self.VAULT)
                self.assertEqual([self.VAULT], sorted(globmod.glob(pattern, root_dir=self.tmp)),
                                 f"{name} does not expand to the vault file; fix the test")
                reason = hooks._leak_reason(f"cat {pattern}")
                self.assertIsNotNone(reason, f"{name} now walks past the guard")
                self.assertIn("reads a vault/secret file directly", reason)

    def test_a_shell_variable_reaches_the_guarded_file_and_is_allowed(self):
        cmd = f"V={self.VAULT}; cat $V"
        self.assertIn(VALUE, self._sh(cmd), "precondition: the command really reads it")
        self.assertIsNone(hooks._leak_reason(cmd))

    def test_a_changed_working_directory_reaches_the_guarded_file_and_is_allowed(self):
        cmd = "cd .charter/vaults && cat db.json"
        self.assertIn(VALUE, self._sh(cmd), "precondition: the command really reads it")
        self.assertIsNone(hooks._leak_reason(cmd))

    def test_a_command_substitution_that_assembles_the_prefix_is_allowed(self):
        """Spelled so the guarded prefix never appears as one run of text.

        `cat $(printf %s .charter/vaults/db.json)` IS denied, by accident rather than by
        design: `shlex` hands `.charter/vaults/db.json)` to `cat` as an operand and the
        pattern matches it. Asserting on that accident would pin a coincidence; this asserts
        on the construct the accident does not cover, and the sibling below names it."""
        cmd = "P=$(printf %s .charter/vaults); cat $P/db.json"
        self.assertIn(VALUE, self._sh(cmd), "precondition: the command really reads it")
        self.assertIsNone(hooks._leak_reason(cmd))

    def test_a_substitution_leaving_the_prefix_spelled_out_is_denied_by_accident(self):
        """Pinned as an accident, not as coverage. `shlex` splits `$(printf %s <path>)` into
        tokens and one of them still carries `.charter/vaults/`, so the guard answers on text
        it was never designed to reach. If a later change to `_segment_argv` drops this
        denial, no documented promise breaks — the docs never claimed it — and this test
        should be updated rather than the parser bent to keep it."""
        self.assertIsNotNone(hooks._leak_reason("cat $(printf %s .charter/vaults/db.json)"))

    def test_brace_expansion_reaches_the_guarded_file_and_is_allowed(self):
        """`sh` on this box may be POSIX and not expand braces, so the read is proved with
        `bash -c` while the guard is asked about the string a shell that does would run."""
        cmd = "cat .charter/{vaults,elsewhere}/db.json"
        self.assertIsNone(hooks._leak_reason(cmd))
        bash = shutil.which("bash")
        if not bash:
            self.skipTest("no bash to prove the expansion with")
        out = subprocess.run([bash, "-c", cmd], cwd=self.tmp, capture_output=True,
                             text=True, timeout=30).stdout
        self.assertIn(VALUE, out, "precondition: the command really reads it")


class TestTheDenialDoesNotRouteTheAgentAroundItself(unittest.TestCase):
    """#423's other half. The `--reveal` denial used to read *"Use `charter … secret
    exec`/`cp`"*, which answered "I want this value" with a command that writes the value to
    a file the guard cannot see — two commands, both allowed, to the same bytes.

    The property: every route a denial recommends must be one that keeps the value out of
    the conversation. `secret exec` hands it to a command; `cp` hands a *path* to a tool, and
    the message must not present it as a way to read the value."""

    REASONS = ("charter secret get v k --reveal", "cat .charter/vaults/db.json")

    def _reasons(self) -> list[str]:
        out = [hooks._leak_reason(c) for c in self.REASONS]
        self.assertTrue(all(out), "precondition: both commands are still denied")
        return out

    def test_every_denial_names_the_route_that_stays_covered(self):
        for reason in self._reasons():
            with self.subTest(reason=reason[:40]):
                self.assertIn("secret exec", reason)

    def test_no_denial_offers_cp_without_saying_it_is_for_a_file(self):
        """`cp` may be mentioned — it is the honest answer for a tool that needs a path —
        but never bare, because a bare mention is the suggestion that made this a finding."""
        for reason in self._reasons():
            with self.subTest(reason=reason[:40]):
                if "cp" not in reason:
                    continue
                self.assertTrue("FILE" in reason or "file" in reason or "path" in reason,
                                "a denial names `cp` without saying it writes a file")

    def test_the_tally_prefix_of_each_denial_is_unchanged(self):
        """`_trace` records `reason[:70]` as the tally key, so the first 70 characters are a
        data schema, not prose. Both denials were reworded here; neither key moved."""
        expected = ("would reveal a secret value into the conversation (--reveal). Use `cha",
                    "reads a vault/secret file directly (would print plaintext). Use `chart")
        self.assertEqual(list(expected), [r[:70] for r in self._reasons()])


if __name__ == "__main__":
    unittest.main()
