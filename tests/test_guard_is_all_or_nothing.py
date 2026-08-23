"""One `charter guard` command lands under every harness that can hold the rule, or none.

#369 fixed the *claim* and left the behaviour: `cmd_guard_ask` and `cmd_guard_allow`
iterated `registry.all()` and wrote harness by harness, so a malformed `.claude/settings.json`
left the rule in force under opencode and absent under Claude Code from one invocation, and
the command said so at the end. #376 is the behaviour — the plane must not be left in a
state one command did not intend.

**The design question is what "all-or-nothing" means when a harness legitimately cannot
express the rule.** `base.py` records `unsupported` as an honest answer rather than a
failure, and Codex answers it to every pattern there is. So the transaction cannot key off
"did this harness write" — that would mean charter never writes a guard rule anywhere while
Codex is registered. It keys off the two refusals being different in kind:

* ``malformed`` is a condition of a FILE. Somebody can fix it in five minutes, and until
  they do, writing the other harnesses is what splits the plane. It **blocks the command**.
* ``unsupported`` is a standing property of a HARNESS. Re-running changes nothing. It is
  **reported and stepped over**, and the rule is genuinely not in force there — which is a
  fact `guard` has to state rather than resolve.

So the tests below come in pairs: the same fake harness refusing in each of the two ways,
with opposite consequences for everybody else's files. A fix that collapsed the two would
pass one of each pair and fail the other.

**Two phases, and the check is the write path minus the write.** Claude Code is the FIRST
harness in the registry, so aborting on the first refusal is not enough: by the time
opencode refuses, Claude Code's file is already written. Every harness is asked first, with
``dry_run=True``, and nothing is written unless all of them can take it. The check is the
same code path as the write because a separate validator is a second answer to one question,
and a transaction whose check disagrees with its commit prints a tick either way.

What is NOT claimed: the commit phase writes several files in sequence with no rollback
primitive, so an IO failure between them still leaves the plane uneven. That is why
`_say_if_uneven` survives — it moved from the ordinary outcome to the one charter cannot
rule out.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config
from charter.harness import registry
from charter.harness.base import Harness
from tests._isolation import PersonaIso

PATTERN = "terraform apply *"

#: What each real harness writes, relative to the plane root. Named here rather than
#: derived from the harness, so a bug that moved a file would fail these tests instead of
#: relocating their assertions with it.
CLAUDE_FILE = Path(".claude") / "settings.json"
OPENCODE_FILE = Path("opencode.json")


class _Unsupported(Harness):
    """A harness with nowhere to put a command-pattern rule, ever.

    Spelled out rather than inherited from :class:`Harness`'s default so that this test
    keeps testing `unsupported` even if the base class one day answers something else.
    """

    name = "fake-unsupported"

    def apply_ask_rule(self, root, pattern, local=False, dry_run=False):
        return "unsupported", "fake-unsupported has no command-pattern permissions"

    def apply_allow_rule(self, root, pattern, local=False, dry_run=False):
        return "unsupported", "fake-unsupported has no command-pattern permissions"


class _Refusing(Harness):
    """A harness whose config file is somebody's to fix, and which writes nothing meanwhile.

    Never touches the filesystem in either phase — so an assertion that the OTHER
    harnesses' files are absent can only be about the transaction, never about this one
    having cleaned up after itself.
    """

    name = "fake-refusing"

    def apply_ask_rule(self, root, pattern, local=False, dry_run=False):
        return "malformed", "/fake/config.json"

    def apply_allow_rule(self, root, pattern, local=False, dry_run=False):
        return "malformed", "/fake/config.json"


class _ChangedUnderneath(Harness):
    """A harness whose file passes the check and then refuses the write.

    Not a hypothetical shape: the check reads the file and the commit re-reads it, so an
    editor saving or a `git checkout` in between lands exactly here. It is the one uneven
    landing this transaction cannot rule out, and a message kept for a case with no test on
    it is a comment.
    """

    name = "fake-changed-underneath"

    def apply_ask_rule(self, root, pattern, local=False, dry_run=False):
        return ("added" if dry_run else "malformed"), "/fake/config.json"

    def apply_allow_rule(self, root, pattern, local=False, dry_run=False):
        return ("added" if dry_run else "malformed"), "/fake/config.json"


class GuardCase(PersonaIso):
    def root(self) -> Path:
        return Path(config.ROOT)

    def claude(self) -> Path:
        return self.root() / CLAUDE_FILE

    def opencode(self) -> Path:
        return self.root() / OPENCODE_FILE

    def local_settings(self) -> Path:
        return self.root() / ".claude" / "settings.local.json"

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def ask(self, pattern: str = PATTERN, **kw):
        return self.invoke(commands.cmd_guard_ask, pattern=pattern,
                           local=kw.get("local", False))

    def allow(self, pattern: str = PATTERN, **kw):
        return self.invoke(commands.cmd_guard_allow, pattern=pattern,
                           local=kw.get("local", False))

    def break_file(self, p: Path) -> None:
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{not json")

    def with_harnesses(self, *extra: type):
        """Run with *extra* appended to the registry, after every real harness."""
        kinds = dict(registry.KINDS)
        for cls in extra:
            kinds[cls.name] = cls
        return mock.patch.object(registry, "KINDS", kinds)


class TestTheWholeCommandOrNoneOfIt(GuardCase):
    def test_every_harness_takes_the_rule_when_nothing_is_broken(self):
        """Precondition for every abort test below. Without this, "opencode.json was not
        written" would pass in a world where opencode never writes at all."""
        rc, _ = self.ask()
        self.assertEqual(0, rc)
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["ask"])
        self.assertEqual("ask", json.loads(self.opencode().read_text())
                         ["permission"]["bash"]["terraform apply *"])

    def test_a_refusal_from_the_first_harness_stops_the_second(self):
        """Claude Code refuses; opencode must not gain the rule anyway.

        The half-write #369 documented and left in place.
        """
        self.break_file(self.claude())
        self.ask()
        self.assertFalse(self.opencode().exists(),
                         "opencode took a rule from a command that refused elsewhere")

    def test_a_refusal_from_the_last_harness_stops_the_first(self):
        """opencode refuses; Claude Code must not have been written BEFORE it was asked.

        Claude Code is first in `registry.KINDS`, so this is the case a bail-on-first-error
        loop cannot fix — the write has already happened by then. Only asking every harness
        before writing any of them makes this pass.
        """
        self.break_file(self.opencode())
        self.ask()
        self.assertFalse(self.claude().exists(),
                         "the first harness wrote before the last one was consulted")

    def test_the_refused_file_is_still_byte_identical(self):
        """charter never repairs a file it could not parse — the restraint the whole
        refusal exists to keep."""
        self.break_file(self.opencode())
        self.ask()
        self.assertEqual("{not json", self.opencode().read_text())

    def test_a_command_that_wrote_nothing_exits_nonzero(self):
        self.break_file(self.opencode())
        rc, _ = self.ask()
        self.assertEqual(1, rc)

    def test_it_says_nothing_was_written_rather_than_naming_an_uneven_landing(self):
        """The operator's remedy changed with the behaviour. "It landed unevenly, fix that
        file and re-run" told them to go and reconcile two harnesses; there is nothing to
        reconcile now, and repeating the old sentence would send them looking."""
        self.break_file(self.opencode())
        _, said = self.ask()
        self.assertRegex(said.lower(), r"nothing was written")
        self.assertNotRegex(said.lower(), r"landed unevenly")

    def test_guard_allow_gets_the_same_transaction(self):
        """`ask` and `allow` are one job with opposite verbs — a transaction on one of them
        is the split this closes, reopened under the other name."""
        self.break_file(self.opencode())
        self.allow()
        self.assertFalse(self.claude().exists())

    def test_fixing_the_file_and_re_running_is_a_first_write_everywhere(self):
        """The residue the issue names: after a half-write, the re-run makes one harness's
        copy a duplicate rather than a first one. With nothing written the first time, the
        second run is simply the write."""
        self.break_file(self.opencode())
        self.ask()
        self.opencode().write_text("{}")
        rc, _ = self.ask()
        self.assertEqual(0, rc)
        self.assertEqual(1, json.loads(self.claude().read_text())["permissions"]["ask"]
                         .count("Bash(terraform apply *)"))
        self.assertEqual("ask", json.loads(self.opencode().read_text())
                         ["permission"]["bash"]["terraform apply *"])


class TestUnsupportedIsNotAFailure(GuardCase):
    """The distinction, asserted from both sides with the same fixture shape.

    Each test registers ONE extra harness that refuses in ONE of the two ways, and asks
    what happened to the real harnesses' files. A transaction keyed on anything coarser
    than the status — "did it write", "was it a non-added answer" — passes one and fails
    the other.
    """

    def test_a_harness_with_nowhere_to_put_the_rule_does_not_block_the_others(self):
        with self.with_harnesses(_Unsupported):
            rc, _ = self.ask()
        self.assertEqual(0, rc)
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["ask"])
        self.assertTrue(self.opencode().exists())

    def test_a_harness_refusing_its_file_does_block_the_others(self):
        with self.with_harnesses(_Refusing):
            rc, _ = self.ask()
        self.assertEqual(1, rc)
        self.assertFalse(self.claude().exists())
        self.assertFalse(self.opencode().exists())

    def test_both_verbs_draw_the_line_in_the_same_place(self):
        with self.with_harnesses(_Unsupported):
            rc, _ = self.allow()
        self.assertEqual(0, rc)
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["allow"])
        with self.with_harnesses(_Refusing):
            self.allow("git status *")
        self.assertNotIn("Bash(git status *)",
                         json.loads(self.claude().read_text())["permissions"]["allow"])


class TestTheOperatorCanSeeWhichKindOfGapItIs(GuardCase):
    """Two gaps that look identical in a listing and have opposite remedies.

    A rule missing under a harness because a file is broken is fixed by fixing the file and
    running the command again. A rule missing because the harness has no such rule is not
    fixed by anything, and an operator who re-runs hoping is doing so because charter let
    them believe the two were the same gap.
    """

    def test_a_standing_limit_is_named_as_one_and_not_as_a_remedy(self):
        _, said = self.ask()
        self.assertRegex(said.lower(), r"not in force under[^\n]*codex")
        self.assertRegex(said.lower(), r"re-running will not change")

    def test_a_broken_file_is_named_with_its_remedy_instead(self):
        self.break_file(self.opencode())
        _, said = self.ask()
        self.assertRegex(said.lower(), r"fix it by hand")
        self.assertNotRegex(said.lower(), r"re-running will not change")

    def test_a_standing_limit_alone_is_not_an_error(self):
        """Codex answers `unsupported` to every pattern. If that read as a failure, no
        `charter guard` command could ever succeed."""
        rc, _ = self.ask()
        self.assertEqual(0, rc)


class TestTheCheckIsTheWritePathMinusTheWrite(GuardCase):
    """`dry_run` is not a validator of its own, and these are what stop it becoming one."""

    def test_a_dry_run_answers_added_and_creates_nothing(self):
        for name in ("claude-code", "opencode"):
            with self.subTest(harness=name):
                root = Path(config.ROOT) / name
                root.mkdir(parents=True, exist_ok=True)
                status, _ = registry.get(name).apply_ask_rule(root, PATTERN, dry_run=True)
                self.assertEqual("added", status)
                self.assertEqual([], list(root.rglob("*")),
                                 "a check wrote something")

    def test_a_dry_run_answers_present_once_the_rule_is_really_there(self):
        for name in ("claude-code", "opencode"):
            with self.subTest(harness=name):
                root = Path(config.ROOT) / name
                root.mkdir(parents=True, exist_ok=True)
                registry.get(name).apply_ask_rule(root, PATTERN)
                self.assertEqual("present", registry.get(name)
                                 .apply_ask_rule(root, PATTERN, dry_run=True)[0])

    def test_a_dry_run_answers_malformed_over_a_file_nobody_can_parse(self):
        root = Path(config.ROOT) / "oc"
        root.mkdir(parents=True, exist_ok=True)
        (root / "opencode.json").write_text("{not json")
        self.assertEqual("malformed", registry.get("opencode")
                         .apply_ask_rule(root, PATTERN, dry_run=True)[0])

    def test_every_registered_harness_answers_a_dry_run_without_writing(self):
        """Iterated over the registry on purpose: a harness added tomorrow is covered the
        day it is registered, not the day somebody remembers this file."""
        for h in registry.all():
            for verb in ("apply_ask_rule", "apply_allow_rule"):
                with self.subTest(harness=h.name, verb=verb):
                    root = Path(config.ROOT) / f"{h.name}-{verb}"
                    root.mkdir(parents=True, exist_ok=True)
                    status, _ = getattr(h, verb)(root, PATTERN, dry_run=True)
                    self.assertIn(status, ("added", "present", "malformed", "unsupported"))
                    self.assertEqual([], list(root.rglob("*")))


class TestTheOneSplitItCannotRuleOut(GuardCase):
    """The claim stops exactly where the guarantee does, and says so in the output.

    The commit phase writes several files in sequence with no rollback primitive. A file
    that changes between being checked and being written still splits the plane, and
    `_say_if_uneven` survives #376 for that case alone — it went from the ordinary outcome
    of a malformed config to the one charter cannot prevent. Without this test, the honest
    residue and the message describing it are both unexercised, and "we kept the warning
    for the rare case" is a sentence nobody can check.
    """

    def test_a_file_that_changes_between_the_check_and_the_write_is_named(self):
        with self.with_harnesses(_ChangedUnderneath):
            rc, said = self.ask()
        self.assertEqual(1, rc)
        self.assertRegex(said.lower(), r"landed unevenly")
        self.assertRegex(said.lower(), r"changed underneath")

    def test_the_harnesses_that_did_land_are_still_reported_as_landed(self):
        """An operator told "unevenly" has to be able to see WHICH side each harness is on,
        or the only safe reading is to check three files by hand."""
        with self.with_harnesses(_ChangedUnderneath):
            self.ask()
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["ask"])

    def test_a_clean_run_never_says_it(self):
        """The message has to stay the exception, or it stops carrying information."""
        _, said = self.ask()
        self.assertNotRegex(said.lower(), r"landed unevenly")


class TestLocalKeepsItsOwnPromise(GuardCase):
    def test_a_harness_declining_the_local_file_does_not_block_the_one_that_has_one(self):
        """opencode answers `unsupported` to `--local` — its only uncommitted config is
        machine-wide. Blocking on that would make `--local` write nothing at all, which is
        the flag's whole purpose deleted."""
        rc, _ = self.ask(local=True)
        self.assertEqual(0, rc)
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.local_settings().read_text())["permissions"]["ask"])

    def test_a_blocked_local_run_does_not_even_touch_gitignore(self):
        """`--local`'s gitignore line is written at the point of use, before the settings
        file is read. A check that writes it is a check with a side effect, and the plane
        is no longer exactly as it was."""
        self.break_file(self.local_settings())
        self.ask(local=True)
        gitignore = self.root() / ".gitignore"
        body = gitignore.read_text() if gitignore.exists() else ""
        self.assertNotIn("/.claude/settings.local.json", body)


if __name__ == "__main__":
    unittest.main()
