"""The guard reads commands, not prose — and does not block the workflow it recommends (#183).

Two parsing defects in the plane-root guard, both false positives on things that are not
branch moves at all.

**Prose inside a quoted argument was treated as a command.** `_segments` split on shell
operators with a regex, *before* any tokenizer, so an operator living inside a quoted string
split it. `echo 'example: cd somewhere ; git checkout -b my-branch'` became two "commands",
the second an apparent branch move — with the string's closing quote riding along into what
the guard believed was a branch name. And `_invocation`'s naive fallback for unbalanced
quotes then dignified the fragment: the regex created it, the fallback made it look real.

The sharp end: `charter report bug '<text containing such an example>'` was refused, so
**the guard blocked its own bug report**. The reporter had to file via a file. So did I,
reproducing it.

**A `cd` in the same command was ignored.** The denial says branch work belongs in a
workspace clone — and `cd workspaces/ws/repo && git checkout -b x` was refused, because the
target came from the session cwd. The first time someone obeys the message they are told
they are doing the forbidden thing.

What is deliberately unchanged: the guard already fires only when the target **positively
resolves** to the plane root, and already treats an unresolvable target as not-the-root. The
inversion the report asks for was there; the `cd` was what it was missing.
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class GuardCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.root)],
                       check=True, capture_output=True)
        self.clone = config.WORKSPACES_DIR / "ws" / "repo"
        self.clone.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.clone)],
                       check=True, capture_output=True)

    def run_cmd(self, cmd: str, cwd=None):
        return run_hook(hooks.pretooluse, {
            "tool_input": {"command": cmd},
            "cwd": str(cwd if cwd is not None else self.root),
            "session_id": "s"})


class TestProseIsNotACommand(GuardCase):
    def test_a_quoted_example_containing_an_operator_is_not_a_branch_move(self):
        """The reported case, verbatim. Nothing is executed and no branch is created."""
        cmd = "echo " + repr("example: cd somewhere ; git checkout -b my-branch")
        self.assertIsNone(_decision(self.run_cmd(cmd)))

    def test_filing_a_bug_report_about_this_is_not_refused(self):
        """The guard blocked its own bug report. That is a denial of the path people use to
        tell us about denials, which is the worst possible thing for it to block."""
        body = "the guard fires on: cd somewhere ; git checkout -b my-branch"
        self.assertIsNone(_decision(self.run_cmd("charter report bug " + repr(body))))

    def test_a_commit_message_mentioning_a_branch_move_is_not_one(self):
        self.assertIsNone(_decision(
            self.run_cmd("git commit -m " + repr("docs: run git checkout -b x to branch"))))

    def test_a_heredoc_style_body_is_not_a_command(self):
        self.assertIsNone(_decision(
            self.run_cmd("printf %s " + repr("step 1 ; git switch other"))))


class TestTheRecommendedWorkflowIsNotBlocked(GuardCase):
    def test_cd_into_a_clone_then_branch_is_allowed(self):
        """Exactly what the denial message tells you to do."""
        self.assertIsNone(_decision(
            self.run_cmd(f"cd {self.clone} && git checkout -b feature/x")))

    def test_a_relative_cd_is_honoured_too(self):
        self.assertIsNone(_decision(
            self.run_cmd("cd workspaces/ws/repo && git checkout -b feature/x")))

    def test_cd_elsewhere_then_back_to_the_root_still_fires(self):
        """The `cd` is followed, not merely noticed: landing back in the root is still the
        root."""
        self.assertEqual(_decision(
            self.run_cmd(f"cd {self.clone} && cd {self.root} && git checkout -b x")), "deny")


class TestItStillGuards(GuardCase):
    def test_a_real_branch_move_in_the_root_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git checkout -b real")), "deny")

    def test_a_real_switch_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git switch other")), "deny")

    def test_a_chained_real_move_is_denied(self):
        """Quoting is respected, not ignored — an operator OUTSIDE quotes still separates."""
        self.assertEqual(_decision(self.run_cmd("echo hi && git checkout -b real")), "deny")

    def test_git_dash_C_into_the_root_from_a_clone_is_denied(self):
        self.assertEqual(_decision(
            self.run_cmd(f"git -C {self.root} checkout -b x", cwd=self.clone)), "deny")


class TestUnparseableInput(GuardCase):
    def test_the_plane_root_guard_fails_OPEN(self):
        """One segment for the whole string, so the program is not `git` and nothing fires.
        Right direction for a guard whose failure mode is annoyance."""
        self.assertIsNone(_decision(self.run_cmd("echo 'unbalanced ; git checkout -b x")))

    def test_the_leak_guard_still_fails_CLOSED(self):
        """The opposite direction, from the same behaviour: the whole string stays one
        segment, so the secret guard still scans it. Not printing a secret is a safety
        invariant, and swallowing an unparseable command is the one failure it may not have.
        """
        r = self.run_cmd("charter secret get v k --reveal 'unbalanced")
        self.assertEqual(_decision(r), "deny")


class TestTheTokenizer(unittest.TestCase):
    def test_quoted_operators_do_not_split(self):
        self.assertEqual(hooks._segment_argv("echo " + repr("a ; b")),
                         [["echo", "a ; b"]])

    def test_unquoted_operators_do_split(self):
        self.assertEqual(hooks._segment_argv("a && b"), [["a"], ["b"]])

    def test_unparseable_yields_one_segment(self):
        # One segment, but tokenized — the leak guard must still see the arguments.
        self.assertEqual(hooks._segment_argv("echo 'x"), [["echo", "'x"]])

    def test_empty_is_empty(self):
        self.assertEqual(hooks._segment_argv(""), [])


if __name__ == "__main__":
    unittest.main()
