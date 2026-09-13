"""`charter <profile>` names a profile, and bare `charter` runs the default one.

A profile is `charter <profile>` the way `charter claude` already is (spec, *A profile*),
and it is a REWRITE rather than a route of its own: `charter claude-work -p hi` becomes
`charter claude --profile claude-work -p hi`, so every splitter, flag and branch below it
runs on the tokens a typed launch produces. That is `_bare_launch`'s own shape and for its
own reason — a second path into `cmd_launch` would be a second set of answers about the
workspace picker, `$CHARTER_HARNESS`, `--probe` and the frame.

**No subparser per profile, and no profile read for a command.** `build_parser()` runs in
every `charter hook …` process, which fires on Bash, Read, Grep, Write, Edit, Task, Skill
and SendMessage; reading profiles there is what ruling 43 took off the import path, and
`tests/_planeguard` refuses the read outright. So the rewrite asks the parser charter has
already built whether the word is a command, and only a word that is none reaches
`profiles.current()`.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr
from unittest import mock

from charter import cli, commands_frame, config, doctor, profiles
from charter.doctor import OK
from tests._isolation import PersonaIso, declare_profiles, make_plane

_LOCAL = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.cw" }

[harness.codex-pinned]
kind = "codex"
command = ["npx", "-y", "@openai/codex@0.140.0"]
"""


class _APlaneWithProfiles(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        make_plane(self)
        # The shared fixture, for its `$GIT_CEILING_DIRECTORIES` as much as for its file:
        # the doctor row below runs a real `git status`, and a temp plane inside somebody's
        # own checkout would otherwise be answered for by THEIR repository.
        self.local = declare_profiles(self, _LOCAL)
        config.use(config.ROOT)
        self.parser = cli.build_parser()

    def _launch_argv(self, argv: list[str]):
        return cli._profile_launch(list(argv), self.parser)


class ADeclaredProfileIsItsKindsLauncher(_APlaneWithProfiles, unittest.TestCase):
    def test_a_declared_profile_name_becomes_its_kinds_launcher(self):
        self.assertEqual(self._launch_argv(["claude-work", "-p", "hi"]),
                         (["claude", "--profile", "claude-work", "-p", "hi"], None))

    def test_a_profile_of_another_kind_becomes_that_kinds_launcher(self):
        self.assertEqual(self._launch_argv(["codex-pinned"]),
                         (["codex", "--profile", "codex-pinned"], None))

    def test_a_built_in_name_is_left_as_typed(self):
        """`charter claude` already parses, and `_launch` resolves `args.profile or
        args.harness` — so a declared replacement of a built-in is reached without a
        rewrite."""
        self.assertEqual(self._launch_argv(["claude", "-p", "hi"]),
                         (["claude", "-p", "hi"], None))

    def test_an_unknown_word_is_left_for_argparse(self):
        self.assertEqual(self._launch_argv(["nope"]), (["nope"], None))

    def test_a_flag_is_left_alone(self):
        self.assertEqual(self._launch_argv(["--version"]), (["--version"], None))

    def test_a_core_command_never_reads_the_profiles(self):
        """The hook path. Every `charter hook …` process runs `main`, and a profile read
        there is the cost ruling 43 measured and removed."""
        with mock.patch.object(profiles, "current",
                               side_effect=AssertionError("read the profiles")):
            for typed in (["doctor"], ["frame-new-chat"], ["hook", "pretooluse"],
                          ["frame"], ["claude"]):
                self.assertEqual(self._launch_argv(typed), (typed, None))

    def test_a_refused_profile_named_on_the_command_line_says_why(self):
        """A declared profile charter refused is a word argparse would call an invalid
        choice, and the operator would be offered the gap reporter for a profile they can
        see in their own file. It says the rule instead."""
        self.local.write_text('[harness.bad]\nkind = "claude"\ncommand = "claude -x"\n')
        err = io.StringIO()
        with redirect_stderr(err):
            argv, rc = self._launch_argv(["bad"])
        self.assertEqual(rc, 2)
        self.assertIn("never a shell string", err.getvalue())

    def test_main_builds_the_parser_once(self):
        """The rewrite asks the parser `main` has already built. Building a second one
        costs every `charter hook …` process ~10 ms, measured on this machine."""
        built: list[int] = []
        real = cli.build_parser
        with mock.patch.object(cli, "build_parser",
                               side_effect=lambda: (built.append(1), real())[1]):
            with self.assertRaises(SystemExit), redirect_stderr(io.StringIO()):
                cli.main(["nope"])
        self.assertEqual(built, [1])


class TheProfileFlagIsChartersAndTheRestIsTheHarnesses(_APlaneWithProfiles,
                                                       unittest.TestCase):
    def test_the_flag_is_split_off_before_argparse_sees_the_rest(self):
        argv, rest = cli._split_frame_argv(
            ["claude", "--profile", "claude-work", "-p", "hi"])
        self.assertEqual(argv, ["claude", "--profile", "claude-work"])
        self.assertEqual(rest, ["-p", "hi"])

    def test_the_parser_carries_the_profile_to_the_launcher(self):
        args = self.parser.parse_args(["claude", "--profile", "claude-work"])
        self.assertIs(args.func, commands_frame.cmd_launch)
        self.assertEqual(args.profile, "claude-work")

    def test_a_launch_that_names_no_profile_carries_none(self):
        self.assertIsNone(self.parser.parse_args(["claude"]).profile)

    def test_frame_launch_is_unattended_unless_told(self):
        """Review 3: a pane nobody asked for never waits on a question."""
        self.assertFalse(self.parser.parse_args(
            ["frame-launch", "--profile", "claude", "--"]).attended)
        self.assertTrue(self.parser.parse_args(
            ["frame-launch", "--profile", "claude", "--attended", "--"]).attended)

    def test_frame_launch_is_a_command_no_profile_can_take(self):
        self.assertIn("frame-launch", cli.command_words())


class BareCharterOpensTheSelector(_APlaneWithProfiles, unittest.TestCase):
    """Bare `charter` names no profile any more: it opens a chat at the selector, and
    `[harness] default` chooses which row the cursor starts on."""

    def _bare(self, text: str):
        self.local.write_text(text)
        config.use(config.ROOT)
        with mock.patch("sys.stdout.isatty", return_value=True):
            return cli._bare_launch([])

    def test_bare_charter_on_a_terminal_opens_the_selector(self):
        argv, rc = self._bare(_LOCAL + '\n[harness]\ndefault = "claude-work"\n')
        self.assertEqual((argv, rc), (["frame", "--select"], None))

    def test_the_declared_default_is_the_row_the_selector_starts_on(self):
        """The rewrite carries no name, so this is where the default is read — by the
        launch, for the row rather than for the command (`_selector_start`)."""
        self.local.write_text(_LOCAL + '\n[harness]\ndefault = "claude-work"\n')
        config.use(config.ROOT)
        self.assertEqual(commands_frame._selector_start(mock.Mock(spec=[])),
                         "claude-work")

    def test_a_refused_default_no_longer_stops_it_and_marks_no_row(self):
        """Ruling 18. The refusal is `doctor`'s now: a launch that refused over a name this
        machine does not have would take the command away over a list charter is about to
        show, with nothing marked on it."""
        err = io.StringIO()
        with redirect_stderr(err):
            argv, rc = self._bare('[harness]\ndefault = "nope"\n')
        self.assertEqual((argv, rc), (["frame", "--select"], None))
        self.assertEqual(err.getvalue(), "")
        self.assertIsNone(commands_frame._selector_start(mock.Mock(spec=[])))

    def test_a_plane_that_declares_nothing_opens_it_too(self):
        self.assertEqual(self._bare(""), (["frame", "--select"], None))

    def test_bare_charter_off_a_terminal_starts_nothing(self):
        """#687/#690: `charter 2>&1 | head` is a probe that costs nothing, and it must stay
        one whatever the plane defaults to — a pipe is no place to draw a selector."""
        self.local.write_text(_LOCAL + '\n[harness]\ndefault = "claude-work"\n')
        config.use(config.ROOT)
        with mock.patch("sys.stdout.isatty", return_value=False):
            self.assertEqual(cli._bare_launch([]), ([], None))


class ACharterTomlDefaultMayNameADeclaredProfile(_APlaneWithProfiles, unittest.TestCase):
    """`charter.toml`'s `[harness] default` names the row the selector starts on, and a
    profile is a name it may hold — so doctor's `charter.toml` row must not report one as a
    harness charter cannot launch."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n[harness]\ndefault = "claude-work"\n')
        config.use(config.ROOT)

    def test_the_charter_toml_row_is_ok(self):
        row = doctor.check_control_plane_config()
        self.assertEqual(row.status, OK, f"{row.detail} — {row.hint}")

    def test_a_default_naming_nothing_at_all_is_still_reported(self):
        (config.ROOT / "charter.toml").write_text(
            'schema = 1\n[harness]\ndefault = "clyde"\n')
        config.use(config.ROOT)
        self.assertNotEqual(doctor.check_control_plane_config().status, OK)


if __name__ == "__main__":
    unittest.main()
