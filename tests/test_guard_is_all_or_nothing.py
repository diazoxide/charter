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
* ``unsupported`` is a standing property of a HARNESS and the pattern it was asked about.
  Re-running changes nothing. It is **reported and stepped over**, and the rule is
  genuinely not in force there — which is a fact `guard` has to state rather than resolve.
  Since #374 opencode answers it to a pattern rather than to every call, which is why
  `TestTheTransactionMeetsOpencodesMcpTranslation` drives the real harness rather than
  another fake: what needs pinning is that a translation which now succeeds, and one that
  now honestly cannot, each land on the side these two statuses mean.

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
import os
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


class _Unwritable(Harness):
    """A harness whose file passes the check and then raises on the write.

    The chmod fixture in :class:`TestAWriteThatFailsIsReportedNotRaised` is the real
    article and can be skipped (a process running as root writes a mode-444 file happily);
    this one cannot, so the reporting path is pinned on every machine. It raises the error
    a read-only file actually produces, `strerror` and all, rather than a bare `OSError`,
    because the message charter prints quotes that string.
    """

    name = "fake-unwritable"

    def _answer(self, dry_run: bool):
        if dry_run:
            return "added", "/fake/config.json"
        raise PermissionError(13, "Permission denied", "/fake/config.json")

    def apply_ask_rule(self, root, pattern, local=False, dry_run=False):
        return self._answer(dry_run)

    def apply_allow_rule(self, root, pattern, local=False, dry_run=False):
        return self._answer(dry_run)


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

    def test_a_blocked_run_names_the_harnesses_that_would_have_taken_it(self):
        """"Nothing was written" without this is a count, not an account.

        The operator's next question after a refusal is which files to go and look at, and
        the answer charter already has is the check phase's own result. Without a test the
        line is decorative: emptying `pending` leaves every other test in the suite green,
        so the second half of the promise "nothing was written, and here is what would
        have taken it" could be deleted by accident.
        """
        self.break_file(self.opencode())
        _, said = self.ask()
        self.assertRegex(said.lower(), r"claude-code would have taken the rule")

    def test_a_blocked_run_does_not_name_a_harness_that_could_never_take_it(self):
        """Codex answers `unsupported` to every pattern. Listing it beside the harnesses
        waiting on a fix would send somebody to re-run for a file that does not exist."""
        self.break_file(self.opencode())
        _, said = self.ask()
        self.assertNotRegex(said.lower(), r"codex would have taken")

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


class TestABlockedRunSaysWhereTheRuleAlreadyStands(GuardCase):
    """"Nothing was written" is not "the rule is nowhere", and only one of those is true.

    The commonest way to meet a malformed config is not on a first attempt — it is on a
    re-run over a plane that already holds the rule somewhere: a bad merge, a teammate's
    machine, a file edited since. The transaction aborts either way, so the same sentence
    covers two planes that could not be more different, and the operator's whole question
    is which one they are on.

    The half-writing loop this replaced answered it by accident: it printed `✓ claude-code:
    already asking for …` beside the `✗`. A transaction that says LESS about the plane than
    the split it removed is not an improvement, however much truer each sentence is on its
    own. These tests hold the blocked run to naming both sides.
    """

    def already_under_claude_code_and_broken_under_opencode(self) -> None:
        """The plane the tests below describe, established and then MEASURED.

        Without the two assertions this is a fixture that might not have made the plane
        uneven at all, and every "charter said so" below would be a claim about nothing.
        """
        rc, _ = self.ask()
        self.assertEqual(0, rc, "the setup run did not land the rule anywhere")
        self.break_file(self.opencode())
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["ask"],
                      "the rule is not in force under claude-code, so nothing is uneven")
        self.assertEqual("{not json", self.opencode().read_text(),
                         "opencode's file parses, so the rule may still be in force there")

    def test_it_names_the_harness_that_already_holds_the_rule(self):
        self.already_under_claude_code_and_broken_under_opencode()
        _, said = self.ask()
        self.assertRegex(said.lower(), r"already in force under[^\n]*claude-code")

    def test_it_says_the_plane_is_uneven_right_now(self):
        """The state, not the command. "The plane is exactly as it was before this command"
        is true of the COMMAND and says nothing about the plane, and a reader who takes it
        for an all-clear stops looking exactly where the rule is missing."""
        self.already_under_claude_code_and_broken_under_opencode()
        _, said = self.ask()
        self.assertRegex(said.lower(), r"uneven right now")

    def test_a_first_attempt_that_reached_nowhere_says_no_such_thing(self):
        """The other plane, and the one the sentence is already right about. A line that
        printed on both would carry no information, which is how the old `✗` beside a `✓`
        came to mean nothing."""
        self.break_file(self.opencode())
        _, said = self.ask()
        self.assertRegex(said.lower(), r"nothing was written")
        self.assertNotRegex(said.lower(), r"already in force")
        self.assertNotRegex(said.lower(), r"uneven right now")

    def test_both_verbs_say_it(self):
        rc, _ = self.allow()
        self.assertEqual(0, rc)
        self.break_file(self.opencode())
        _, said = self.allow()
        self.assertRegex(said.lower(), r"already in force under[^\n]*claude-code")


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

    def test_a_harness_that_already_holds_the_rule_is_not_called_out_of_reach(self):
        """The line names harnesses with NOWHERE to put the rule, and nothing else.

        `present` is the status one keystroke away from `unsupported` in that filter, and
        it is the one every harness answers on the second run of any command — so a filter
        that admitted it would print "Not in force under claude-code" over a rule that is
        in force under claude-code, constantly. Only a second run puts a real harness into
        `present` while codex is still `unsupported`, which is why this test runs the
        command twice.
        """
        self.ask()
        _, said = self.ask()
        self.assertRegex(said.lower(), r"not in force under[^\n]*codex")
        self.assertNotRegex(said.lower(), r"not in force under[^\n]*claude-code")
        self.assertNotRegex(said.lower(), r"not in force under[^\n]*opencode")

    def test_the_second_run_really_does_answer_present(self):
        """Precondition for the test above, and it has to name BOTH harnesses.

        If the re-run wrote the rule afresh instead of finding it, `present` would never be
        in those results and the filter above would never be asked the question. One
        harness answering `present` is not enough either: the assertion above forbids
        claude-code AND opencode from being called out of reach, and each half is only a
        real question while that harness is really in the `present` state.
        """
        self.ask()
        _, said = self.ask()
        self.assertRegex(said.lower(), r"claude-code: already asking for")
        self.assertRegex(said.lower(), r"opencode: already asking for")

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


class TestAWriteThatFailsIsReportedNotRaised(GuardCase):
    """The other half of the trigger the issue named: a malformed OR UNWRITABLE file.

    `_guard_apply`'s docstring said from the first commit that an IO failure between two
    writes still leaves the plane uneven and that `_say_if_uneven` is what survives to
    report it. It was not true of the code: `write_text` raised, the `OSError` went out
    through `cmd_guard_ask` as a traceback, and everything below it — every `✓` already
    earned, the uneven warning, the exit code — never ran. The operator got a stack trace
    and a split plane, which is #369's failure with the message deleted rather than moved.

    The split itself is NOT closed here and is not closable: there is no rollback
    primitive, and a file that will not take a write cannot be found out about without
    writing to it. What is closed is charter going silent about it.
    """

    def test_the_reporting_path_exists_at_all(self):
        """Driven by a harness that raises on the write, so this cannot be skipped: the
        chmod fixture below is the real article and root ignores mode bits."""
        with self.with_harnesses(_Unwritable):
            rc, said = self.ask()
        self.assertEqual(1, rc)
        self.assertRegex(said.lower(), r"could not write[^\n]*/fake/config\.json")
        self.assertRegex(said.lower(), r"landed unevenly")
        self.assertRegex(said.lower(), r"or would not take the write",
                         "the uneven warning names only the cause it had before this case")

    def test_it_quotes_the_operating_systems_own_reason(self):
        """charter has no account of WHY a write failed beyond `strerror`, and inventing
        one — permissions, ownership, a full disk — is naming a cause it only inferred."""
        with self.with_harnesses(_Unwritable):
            _, said = self.ask()
        self.assertIn("Permission denied", said)

    def test_it_is_not_reported_as_a_file_that_does_not_parse(self):
        """The two refusals send the operator to different places. `malformed` says the
        content is broken; a file charter could not write is usually perfectly valid, and
        telling somebody their good JSON "is not valid" is a wrong cause stated with
        total confidence."""
        with self.with_harnesses(_Unwritable):
            _, said = self.ask()
        self.assertNotRegex(said.lower(), r"is not valid")

    def test_it_says_re_running_is_the_remedy(self):
        """The exact opposite of the standing-limit line, which is why both are printed.

        A harness with nowhere to put the rule is told "Re-running will not change that".
        A file that would not take the write is the other kind of gap entirely — nothing
        about the harness is wrong and running the command again is precisely the fix —
        and an operator who cannot tell the two apart re-runs the one that never helps and
        gives up on the one that does.
        """
        with self.with_harnesses(_Unwritable):
            _, said = self.ask()
        self.assertRegex(said.lower(), r"re-run once that file can be written")

    def test_the_harnesses_that_did_land_still_say_so(self):
        with self.with_harnesses(_Unwritable):
            _, said = self.ask()
        self.assertRegex(said.lower(), r"claude-code: asking for")
        self.assertIn("Bash(terraform apply *)",
                      json.loads(self.claude().read_text())["permissions"]["ask"])

    def test_both_verbs_report_it(self):
        with self.with_harnesses(_Unwritable):
            rc, said = self.allow()
        self.assertEqual(1, rc)
        self.assertRegex(said.lower(), r"could not write")

    def test_a_real_read_only_file_reaches_that_path(self):
        """The measurement behind the fake. A harness that raises proves the report; only
        a real file proves a real `charter guard` can produce the raise at all.

        The capability — can a write in THIS process be made to fail — is decided by the
        attempt itself rather than by a probe beside it. Mode bits are advisory to root,
        and a separate probe that answered wrongly would either skip this on a machine
        that can run it (silently, forever) or claim a capability the machine lacks. Here
        the run happens either way and the file's own content afterwards says which
        machine this is.
        """
        oc = self.opencode()
        oc.write_text("{}\n")
        os.chmod(oc, 0o444)
        try:
            rc, said = self.ask()
        finally:
            os.chmod(oc, 0o644)
        if oc.read_text() != "{}\n":
            self.skipTest("this process wrote a mode-444 file it owns (running as root?), "
                          "so no real write here can be made to fail")
        self.assertEqual(1, rc)
        self.assertRegex(said.lower(), r"could not write[^\n]*opencode\.json")
        self.assertRegex(said.lower(), r"landed unevenly")


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


class TestTheTransactionMeetsOpencodesMcpTranslation(GuardCase):
    """#374 gave opencode a REAL rule for an MCP pattern, and the transaction spans it.

    Before #374, `charter guard ask mcp__slack__send` fell through to `bash` under opencode
    and wrote an inert rule under a tick. It now translates, and two new answers reach this
    transaction from a harness that used to answer `added` to everything. Both are honest
    outcomes rather than failures, and each has to land on the right side of the line:

    * ``unsupported`` — :data:`opencode.FLAT_ONLY_PERMISSIONS` with a real glob. opencode's
      `webfetch` takes a bare action and no pattern, so the rule genuinely cannot be
      expressed there. That is a standing property of the harness and the pattern: re-running
      changes nothing, so it must NOT stop Claude Code taking the rule it can express.
    * ``malformed`` — one of those keys already holding an object. That is a file somebody
      can fix, so it must stop everybody, exactly as an unparseable file does.

    Asserted through the REAL opencode harness rather than a fake. A fake would only prove
    the transaction sorts two strings it was handed; what is worth pinning is that the
    harness whose translation changed still lands on the side these statuses mean.
    """

    def test_a_pattern_opencode_cannot_express_does_not_block_claude_code(self):
        """`webfetch` is flat-only, so a URL glob has nowhere to go under opencode."""
        rc, said = self.ask("WebFetch(https://example.com/*)")
        self.assertEqual(0, rc)
        self.assertIn("WebFetch(https://example.com/*)",
                      json.loads(self.claude().read_text())["permissions"]["ask"])
        self.assertFalse(self.opencode().exists(),
                         "opencode cannot express this rule and must not have been written")
        self.assertRegex(said.lower(), r"not in force under[^\n]*opencode")

    def test_that_pattern_really_is_unsupported_under_opencode(self):
        """Precondition. Without it the test above passes in a world where opencode
        answered `added` and simply wrote somewhere this test does not look."""
        status, _detail = registry.get(registry.OPENCODE).apply_ask_rule(
            self.root(), "WebFetch(https://example.com/*)", dry_run=True)
        self.assertEqual("unsupported", status)

    def test_a_flat_only_key_holding_an_object_blocks_every_harness(self):
        """The other half of the pair, and the reason the two cannot be collapsed. Same
        harness, same command, a refusal of the other kind — and Claude Code's file must
        not exist afterwards."""
        self.opencode().write_text(
            json.dumps({"permission": {"webfetch": {"*": "ask"}}}, indent=2) + "\n")
        rc, said = self.ask("WebFetch(*)")
        self.assertEqual(1, rc)
        self.assertFalse(self.claude().exists(),
                         "claude-code was written during a blocked run")
        self.assertRegex(said.lower(), r"nothing was written")

    def test_an_mcp_pattern_lands_under_both_harnesses_in_one_command(self):
        """The ordinary case #374 created, driven end to end. opencode's key is the
        translated name, not the operator's pattern, and Claude Code's is the pattern."""
        rc, _ = self.ask("mcp__slack__send")
        self.assertEqual(0, rc)
        self.assertIn("mcp__slack__send",
                      json.loads(self.claude().read_text())["permissions"]["ask"])
        self.assertEqual({"slack_send": {"*": "ask"}},
                         json.loads(self.opencode().read_text())["permission"])


class TestABlockedRunStillSaysWhatTheStandingRuleOutranks(GuardCase):
    """A rule that already stands still outranks what it outranked (#374), blocked or not.

    The blocked path replaces the per-harness loop, so it also replaces the loop's
    `_warn_if_outranking` call — and losing it was measured against a `git archive
    origin/main` export rather than reasoned about: same plane, same command, main printed
    the line and the first rebase of this branch did not. `charter guard allow mcp__plan`
    with opencode already holding `plan_*` means two of opencode's OWN denies are allow
    right now, which is the whole reason #374 prints anything at all.

    The pair below is the filter: ``present`` warns because that rule is in force, and
    ``added`` must not, because a blocked run wrote it nowhere and its consequences do not
    exist yet. A fix that reinstated the loop's two-status filter wholesale would pass the
    first and fail the second.
    """

    def already_under_opencode_and_broken_under_claude_code(self) -> None:
        rc, _ = self.allow("mcp__plan")
        self.assertEqual(0, rc, "the setup run did not land the rule anywhere")
        self.break_file(self.claude())
        self.assertEqual({"plan_*": {"*": "allow"}},
                         json.loads(self.opencode().read_text())["permission"],
                         "opencode does not hold the outranking rule, so nothing outranks")

    def test_the_operator_is_told_what_the_standing_rule_decides_for_them(self):
        self.already_under_opencode_and_broken_under_claude_code()
        rc, said = self.allow("mcp__plan")
        self.assertEqual(1, rc)
        self.assertIn("plan_enter", said)
        self.assertIn("plan_exit", said)

    def test_the_line_that_needs_acting_on_still_comes_first(self):
        """The other half of `_warn_if_outranking`'s own reasoning: a warning printed over
        a malformed file must not bury the sentence that fixes it."""
        self.already_under_opencode_and_broken_under_claude_code()
        _, said = self.allow("mcp__plan")
        self.assertLess(said.index("is not valid"), said.index("plan_enter"))

    def test_a_rule_that_was_never_written_is_not_warned_about(self):
        """Nothing landed, so there is nothing outranking anything. Warning here would
        describe a consequence of a rule that does not exist — which is exactly why
        `_warn_if_outranking` skips `unsupported` in the ordinary loop."""
        self.break_file(self.claude())
        rc, said = self.allow("mcp__plan")
        self.assertEqual(1, rc)
        self.assertRegex(said.lower(), r"nothing was written")
        self.assertNotIn("plan_enter", said)

    def test_a_clean_run_still_says_it_too(self):
        """The path this one branched off. Guards against a fix that moved the warning
        into the blocked path instead of adding it there."""
        rc, said = self.allow("mcp__plan")
        self.assertEqual(0, rc)
        self.assertIn("plan_enter", said)


if __name__ == "__main__":
    unittest.main()
