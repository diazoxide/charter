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
        """Nothing fires: the fallback's argv is a guess made without quoting, and this is
        the guard whose failure mode is annoyance. `_plane_root_branch_reason` says so in
        one line rather than relying on a tokenizer accident to say it."""
        self.assertIsNone(_decision(self.run_cmd("echo 'unbalanced ; git checkout -b x")))

    def test_the_leak_guard_still_fails_CLOSED(self):
        """The opposite direction: the secret guard still scans an unparseable command.
        Not printing a secret is a safety invariant, and swallowing an unparseable command
        is the one failure it may not have.

        **Rescoped (#429).** This asserted the property with the offending program at token
        0 — `charter secret get … --reveal 'unbalanced` — which is the one arrangement
        where collapsing to a single segment is harmless. It passed green for as long as
        the property it names was false. The first case below is the arrangement that
        matters; the original spelling is kept as the second.
        """
        r = self.run_cmd("echo hi ; charter secret get v k --reveal 'unbalanced")
        self.assertEqual(_decision(r), "deny")
        r = self.run_cmd("charter secret get v k --reveal 'unbalanced")
        self.assertEqual(_decision(r), "deny")

    def test_an_unparseable_quote_does_not_hide_a_later_invocation(self):
        """The bypass: `echo $'it\\'s fine'` is valid bash and trips `shlex`, and the
        fallback returned ONE whitespace-split segment. Every guard reads token 0 as the
        program, so the `echo` was the whole command and everything after `;` was invisible
        — the leak guard, the SSH guard, the signing guard and the release floor all
        flipped from deny to allow behind the same four characters.
        """
        prefix = "echo $'it\\'s fine' ; "
        for cmd in (prefix + "cat .charter/vaults/x.json",
                    prefix + "git clone git@github.com:o/r.git",
                    prefix + "git commit -S -m x",
                    prefix + "GIT_SSH_COMMAND=/tmp/k git fetch"):
            with self.subTest(cmd=cmd):
                self.assertEqual(_decision(self.run_cmd(cmd)), "deny")

    def test_a_path_in_a_flag_VALUE_is_denied_when_the_command_cannot_be_parsed(self):
        """What the raw scan adds on top of re-segmenting. Parsed, `grep -e <vault> docs/`
        is a search for a MENTION and is allowed — the false positive #258 removed. Once
        the quoting is broken, the argv that judgement rests on is a guess, so the guard
        stops being clever and matches the string. Fail-closed on a malformed command is
        survivable; printing a credential is not."""
        self.assertEqual(_decision(
            self.run_cmd("grep -e .charter/vaults/db.json 'unbalanced")), "deny")
        self.assertIsNone(_decision(
            self.run_cmd("grep -e .charter/vaults/db.json docs/personas.md")))

    def test_reveal_in_an_unparseable_command_is_denied_whoever_the_program_is(self):
        """Same trade for the other arm: parsed, `--reveal` only counts as a flag of
        `charter` itself."""
        self.assertEqual(_decision(self.run_cmd("some-other-tool --reveal 'unbalanced")),
                         "deny")
        self.assertIsNone(_decision(self.run_cmd("some-other-tool --reveal")))

    def test_an_operator_without_spaces_is_still_a_boundary(self):
        """The next input after the fix: a fallback that only split on free-standing
        operators would be a rule you satisfy by deleting a space."""
        self.assertEqual(
            _decision(self.run_cmd("echo 'x;cat .charter/vaults/db.json")), "deny")


class TestAWrapperDoesNotHideTheProgram(GuardCase):
    """`prog` came from token 0, so any one-word wrapper was the program (#430).

    Every command here was verified ALLOW against the real handler before the fix, and the
    unwrapped form of each was DENY — a guard you walk past by typing `env` first.
    `_split_env` now strips the wrapper run, so all four guards get the real program from
    one place instead of four.
    """

    WRAPPED = (
        "env charter secret get v k --reveal --force",
        "/usr/bin/env charter secret get v k --reveal --force",
        "command cat .charter/vaults/x.json",
        "time cat .charter/vaults/x.json",
        "nohup cat .charter/vaults/x.json",
        "exec cat .charter/vaults/x.json",
        "sudo cat .charter/vaults/x.json",
        "{ cat .charter/vaults/x.json; }",
        "( cat .charter/vaults/x.json )",
        "echo $(cat .charter/vaults/x.json)",
        "if true; then cat .charter/vaults/x.json; fi",
        "cd .charter/vaults && cat x.json",
    )

    def test_a_wrapper_prefix_does_not_hide_the_program(self):
        for cmd in self.WRAPPED:
            with self.subTest(cmd=cmd):
                self.assertEqual(_decision(self.run_cmd(cmd)), "deny")

    def test_the_wrappers_own_options_do_not_swallow_the_program(self):
        """Cover the class, not the demo: each of these puts a flag — and for three of
        them a flag VALUE — between the wrapper and the program. `env -i` is the one that
        makes a flat "flags take a value" table wrong, which is why the table is per
        wrapper, exactly like `_TAKES_VALUE` upstairs."""
        for cmd in ("sudo -u root cat .charter/vaults/x.json",
                    "env -i cat .charter/vaults/x.json",
                    "env -u PATH cat .charter/vaults/x.json",
                    "timeout 5 cat .charter/vaults/x.json",
                    "timeout -s KILL 5 cat .charter/vaults/x.json",
                    "nice -n 10 cat .charter/vaults/x.json",
                    "stdbuf -o0 cat .charter/vaults/x.json",
                    "sudo -- cat .charter/vaults/x.json",
                    "env FOO=bar cat .charter/vaults/x.json",
                    "sudo env cat .charter/vaults/x.json",
                    "xargs cat .charter/vaults/x.json"):
            with self.subTest(cmd=cmd):
                self.assertEqual(_decision(self.run_cmd(cmd)), "deny")

    def test_env_split_string_is_not_a_way_through(self):
        """`env -S 'cat <vault>'` packs the command into one token. Skipping `-S`'s value
        the way the other value-taking flags are skipped would leave no program at all —
        fail-open on the one flag whose value IS the command."""
        for cmd in ("env -S 'cat .charter/vaults/x.json'",
                    "env -Scat .charter/vaults/x.json",
                    "env --split-string='cat .charter/vaults/x.json'"):
            with self.subTest(cmd=cmd):
                self.assertEqual(_decision(self.run_cmd(cmd)), "deny")

    def test_the_program_name_is_case_folded_too(self):
        """Same class as the path spellings (#431): on the filesystems this runs on,
        `CHARTER` and `charter` are the same binary."""
        self.assertEqual(_decision(
            self.run_cmd("CHARTER secret get v k --reveal --force")), "deny")
        self.assertEqual(_decision(self.run_cmd("CAT .charter/vaults/x.json")), "deny")

    def test_the_one_credential_guard_gets_the_same_program(self):
        """The sharper half: unlike `--reveal`, nothing downstream re-checks an SSH
        transport override, so this was a clean route around golden rule 0."""
        for cmd in ("env GIT_SSH_COMMAND=/tmp/k git push",
                    "/usr/bin/env git push git@github.com:o/r.git",
                    "sudo git push git@github.com:o/r.git",
                    "{ git clone git@github.com:o/r.git; }"):
            with self.subTest(cmd=cmd):
                self.assertEqual(_decision(self.run_cmd(cmd)), "deny")

    def test_a_wrapper_is_not_a_reason_to_deny_by_itself(self):
        """The other direction. `env`, `sudo` and a group are ordinary shell, and a guard
        that denied them would be turned off within a day."""
        for cmd in ("env FOO=bar make test",
                    "sudo systemctl restart nginx",
                    "( cd workspaces && ls )",
                    "timeout 5 python3 -m pytest",
                    "env python3 -m charter doctor"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(_decision(self.run_cmd(cmd)))

    def test_a_shell_c_string_is_still_out_of_scope(self):
        """Pinned, not fixed. `sh -c '<string>'` is a documented limit of this guard; a
        wrapper runs its own argv, a shell runs a string, and conflating them here would
        smuggle a much larger change into a parsing fix."""
        self.assertIsNone(_decision(self.run_cmd("sh -c 'cat .charter/vaults/x.json'")))


class TestTheTokenizer(unittest.TestCase):
    def test_quoted_operators_do_not_split(self):
        self.assertEqual(hooks._segment_argv("echo " + repr("a ; b")),
                         [["echo", "a ; b"]])

    def test_unquoted_operators_do_split(self):
        self.assertEqual(hooks._segment_argv("a && b"), [["a"], ["b"]])

    def test_unparseable_yields_one_segment(self):
        # One segment, but tokenized — the leak guard must still see the arguments.
        self.assertEqual(hooks._segment_argv("echo 'x"), [["echo", "'x"]])

    def test_unparseable_still_splits_on_operators(self):
        """The whole of #429 in one assertion: a second command after an unparseable
        quote is a second SEGMENT, not the tail of the first one's argv."""
        self.assertEqual(hooks._segment_argv("echo 'x ; cat v"),
                         [["echo", "'x"], ["cat", "v"]])

    def test_an_operator_glued_to_its_neighbours_still_splits(self):
        self.assertEqual(hooks._segment_argv("echo 'x;cat v"),
                         [["echo", "'x"], ["cat", "v"]])

    def test_the_parse_flag_says_which_path_ran(self):
        """`_segment_argv_parsed` is how the leak guard knows its argv is a guess and adds
        a raw scan. A flag that never went False would make that scan dead code."""
        self.assertTrue(hooks._segment_argv_parsed("cat v")[1])
        self.assertFalse(hooks._segment_argv_parsed("cat 'v")[1])

    def test_grouping_tokens_end_a_segment(self):
        self.assertEqual(hooks._segment_argv("{ cat v; }"), [["cat", "v"]])
        self.assertEqual(hooks._segment_argv("( cat v )"), [["cat", "v"]])

    def test_empty_is_empty(self):
        self.assertEqual(hooks._segment_argv(""), [])


class TestTheProgramIsWhatRuns(unittest.TestCase):
    """`_split_env` is the one answer to "what program is this segment", so all four
    guards get the same one. Unit level, because the table of wrappers and their
    value-taking flags is where the next mistake will be."""

    def prog(self, cmd: str) -> str:
        return hooks._split_env(cmd.split())[0]

    def test_wrappers_are_stripped(self):
        for cmd in ("env cat x", "command cat x", "sudo cat x", "time cat x",
                    "nohup cat x", "exec cat x", "/usr/bin/env cat x", "xargs cat x",
                    "sudo env FOO=1 nice -n 5 cat x"):
            with self.subTest(cmd=cmd):
                self.assertEqual(self.prog(cmd), "cat")

    def test_env_assignments_survive_a_wrapper(self):
        """What the one-credential guard reads. `env GIT_SSH_COMMAND=… git push` must
        yield the assignment as an ENV prefix, not as an argument."""
        prog, env, _argv = hooks._split_env("env GIT_SSH_COMMAND=/tmp/k git push".split())
        self.assertEqual(prog, "git")
        self.assertEqual(env, ["GIT_SSH_COMMAND=/tmp/k"])

    def test_a_real_program_is_not_mistaken_for_a_wrapper(self):
        for cmd in ("git push", "python3 -m charter doctor", "make test"):
            with self.subTest(cmd=cmd):
                self.assertEqual(self.prog(cmd), cmd.split()[0])

    def test_argv_still_starts_at_the_program(self):
        """`_file_operands` drops argv[0] when it is the program — if the wrapper stripping
        left argv pointing at the wrapper, every operand would shift by one."""
        prog, _env, argv = hooks._split_env("sudo -u root cat .charter/vaults/x".split())
        self.assertEqual((prog, argv), ("cat", ["cat", ".charter/vaults/x"]))


if __name__ == "__main__":
    unittest.main()
