"""Bare ``charter`` launches the frame — §4a of the IDE spec.

`charter claude` becomes `charter`, on a plane that says which harness it means. Three
things are new and each is tested here rather than described:

* a ``[harness]`` section in `charter.toml`, with one key, ``default``;
* a refusal at the CONFIG boundary for a value charter cannot launch, carried out to the
  two readers that can report it rather than degrading into silence;
* an argv REWRITE in `cli.main`, gated on stdout being a terminal.

**The third is the whole reason this needed care, and it has its own class.**
`commands_frame.cmd_launch` returns `bypass(argv)` — an `os.execvp` — when stdout is not a
tty. Today bare `charter` is argparse's own usage error: exit 2, nothing started, so
``charter 2>&1 | head`` is a free probe for "is charter installed". Rewrite argv
unconditionally and, on a plane that sets a default, that pipeline execs Claude Code. It
would be correct against every config anybody tested and wrong the first time a script
looked for charter — #687 and #690's exact shape, twice over. `TheNonTtyPathStartsNothing`
is that hazard as a test.

The refusal cases assert the REASON, not merely that the value was not honoured: a
`default` that came back `None` proves nothing on its own, because that is also what a
plane declaring nothing gets — which is precisely the confusion `refused` exists to end.
"""

from __future__ import annotations

import io
import sys
import unittest
from contextlib import redirect_stderr
from unittest import mock

from charter import cli, commands_frame, config, doctor, harness, instance
from charter.harness.base import Harness
from tests._isolation import PersonaIso


class TheSectionIsReadAtTheConfigBoundary(unittest.TestCase):
    """`instance.harness_of` — pure, no plane, no filesystem."""

    def test_a_plane_that_declares_nothing_gets_no_default(self):
        """The shipped answer, and the decision the whole section turns on: charter does
        not guess a harness for somebody who never named one."""
        self.assertEqual(instance.harness_of({}), {"default": None, "refused": None})

    def test_a_declared_harness_is_the_default(self):
        got = instance.harness_of({"harness": {"default": "claude"}})
        self.assertEqual(got, {"default": "claude", "refused": None})

    def test_every_registered_harness_can_be_the_default(self):
        """Not just `claude`. `opencode` and `codex` are launchers too, and a plane that
        runs one of them is entitled to the same command."""
        for name in ("claude", "opencode", "codex"):
            with self.subTest(name=name):
                got = instance.harness_of({"harness": {"default": name}})
                self.assertEqual(got["default"], name)
                self.assertIsNone(got["refused"])

    def test_what_is_stored_is_the_registrys_own_string_not_the_files(self):
        """`update_of`'s belt-and-braces rule, and it bites harder here: this value
        becomes `argv[0]` of a charter invocation, out of a file that arrives from
        somebody else's machine. No object originating in charter.toml reaches the CLI."""
        supplied = "".join(["cla", "ude"])          # equal to, and not, the constant
        self.assertIsNot(supplied, "claude")
        got = instance.harness_of({"harness": {"default": supplied}})
        self.assertIs(got["default"], instance.launchable_harnesses()[0])

    def test_an_unknown_name_is_refused_and_the_refusal_names_it(self):
        """The grill this feature was built against. `default = None` is also what a plane
        that declared nothing gets, so a test asserting only that would pass against a
        charter which threw the key away — which is the defect, not the fix."""
        got = instance.harness_of({"harness": {"default": "clyde"}})
        self.assertIsNone(got["default"])
        self.assertEqual(got["refused"], "clyde")

    def test_the_frame_escape_hatch_is_not_a_launchable_default(self):
        """`charter frame` is registered by the same loop in `cli._add_frame_parsers`, and
        it carries no command of its own — `charter frame -- <cmd>`. A plane defaulting to
        it would get `charter frame: nothing to run` on every bare `charter`."""
        got = instance.harness_of({"harness": {"default": "frame"}})
        self.assertIsNone(got["default"])
        self.assertEqual(got["refused"], "frame")
        self.assertNotIn("frame", instance.launchable_harnesses())

    def test_a_value_of_the_wrong_type_is_refused_and_named_too(self):
        """`tomllib` hands back lists, tables, ints and bools. A value of the wrong TYPE is
        as declared, and as not in force, as a misspelt string — reporting only the strings
        would leave these in the silent bucket the `refused` key exists to empty."""
        for value in (["claude"], {"name": "claude"}, 3, True, 1.5):
            with self.subTest(value=value):
                got = instance.harness_of({"harness": {"default": value}})
                self.assertIsNone(got["default"])
                self.assertTrue(got["refused"], f"{value!r} was refused without saying so")

    def test_a_case_or_whitespace_variant_is_refused_rather_than_normalised(self):
        """A closed set, matched exactly — `UPDATE_CHANNELS`'s posture. Trimming and
        lowercasing here would be charter guessing what a committed file meant."""
        for value in ("Claude", "CLAUDE", " claude", "claude ", "claude-code", ""):
            with self.subTest(value=value):
                self.assertIsNone(instance.harness_of({"harness": {"default": value}})["default"])

    def test_a_refused_value_cannot_forge_a_second_report_line(self):
        """It is printed into charter's own stderr report and into a `doctor` row. A
        newline there writes a line that looks exactly as much like charter's output as
        the real one — `contain.readable`'s whole subject."""
        got = instance.harness_of({"harness": {"default": "claude\n✓ charter: pwned"}})
        self.assertIsNone(got["default"])
        self.assertNotIn("\n", got["refused"])
        self.assertTrue(got["refused"].isascii())

    def test_a_refused_value_that_renders_as_nothing_still_names_something(self):
        """`contain.readable` over `contain.one_line`, for its own stated reason: this
        sentence tells somebody to go and fix an identifier, and a row naming an invisible
        codepoint names nobody (#498)."""
        got = instance.harness_of({"harness": {"default": "ㅤㅤ"}})
        self.assertTrue(got["refused"].strip(), "the refusal named nothing at all")

    def test_a_section_that_is_not_a_section_degrades_rather_than_raising(self):
        """`instance` is imported by every command including `charter --version`."""
        for section in ("claude", ["claude"], 7, None, True):
            with self.subTest(section=section):
                self.assertEqual(instance.harness_of({"harness": section}),
                                 {"default": None, "refused": None})

    def test_a_section_with_no_default_key_is_not_a_refusal(self):
        """Declaring `[harness]` and nothing in it is declaring nothing. A `refused` here
        would put a `doctor` warning on a plane that did nothing wrong."""
        self.assertEqual(instance.harness_of({"harness": {"nothing": "here"}}),
                         {"default": None, "refused": None})

    def test_the_defaults_view_and_the_fields_table_cannot_drift(self):
        """`FRAME_FIELDS`'s shape, and its whole reason for existing."""
        self.assertEqual(set(instance.HARNESS_DEFAULTS), set(instance.HARNESS_FIELDS))
        self.assertIsNone(instance.HARNESS_DEFAULTS["default"])
        self.assertEqual(instance.HARNESS_FIELDS["default"][1], "default")

    def test_refused_is_not_a_setting_and_is_absent_from_the_fields_table(self):
        """It rides out in the returned dict the way `frame_of` carries `components`:
        nothing in `[harness]` is spelled `refused`, so a plane writing it declares
        nothing."""
        self.assertNotIn("refused", instance.HARNESS_FIELDS)
        self.assertIsNone(instance.harness_of({"harness": {"refused": "claude"}})["default"])


class TheLegalNamesComeFromTheRegistry(unittest.TestCase):
    """`launchable_harnesses` reads `harness.KINDS` rather than listing three words.

    `harness/registry.py`'s own rule — *"a harness added to KINDS is covered everywhere the
    day it is registered, never a hardcoded literal someone has to remember to update"*. A
    tuple in `instance.py` would go stale silently, and the symptom would be charter
    refusing a harness charter can launch.
    """

    def test_a_harness_registered_today_is_a_legal_default_today(self):
        class _Fictional(Harness):
            name = "zzz-fictional"
            cli_name = "zzz"
            binary = "zzz"

        with mock.patch.dict(harness.registry.KINDS, {"zzz-fictional": _Fictional}):
            self.assertIn("zzz", instance.launchable_harnesses())
            self.assertEqual(instance.harness_of({"harness": {"default": "zzz"}})["default"],
                             "zzz")

    def test_a_harness_charter_cannot_launch_is_not_a_legal_default(self):
        """An empty `cli_name` is the attribute's own spelling of "charter cannot launch
        this". A default naming one would rewrite argv to `[""]`."""
        class _Unlaunchable(Harness):
            name = "zzz-unlaunchable"
            cli_name = ""
            binary = ""

        with mock.patch.dict(harness.registry.KINDS,
                             {"zzz-unlaunchable": _Unlaunchable}):
            self.assertNotIn("", instance.launchable_harnesses())
            self.assertIsNone(instance.harness_of({"harness": {"default": ""}})["default"])

    def test_the_names_are_the_words_an_operator_types(self):
        """`cli_name`, never `name`: `charter claude` is the command, `claude-code` is the
        harness's identity in `$CHARTER_HARNESS`. A plane writing the identity gets a
        refusal that names it rather than a launcher that does not exist."""
        self.assertEqual(instance.launchable_harnesses(),
                         tuple(h.cli_name for h in harness.all() if h.cli_name))
        self.assertIsNone(instance.harness_of({"harness": {"default": "claude-code"}})["default"])


class TheSectionReachesConfig(PersonaIso):
    """`config.HARNESS` is derived from the plane's own charter.toml, like `config.UPDATE`."""

    def _declare(self, text: str) -> None:
        (self.tmp / "charter.toml").write_text(text)
        config.use(self.tmp)

    def test_a_plane_that_declares_a_default_derives_it(self):
        self._declare('schema = 1\n[harness]\ndefault = "claude"\n')
        self.assertEqual(config.HARNESS, {"default": "claude", "refused": None})

    def test_a_plane_that_declares_nothing_derives_no_default(self):
        self._declare("schema = 1\n")
        self.assertEqual(config.HARNESS, {"default": None, "refused": None})

    def test_a_refused_declaration_reaches_config_as_a_refusal(self):
        self._declare('schema = 1\n[harness]\ndefault = "clyde"\n')
        self.assertEqual(config.HARNESS, {"default": None, "refused": "clyde"})

    def test_harness_is_one_of_the_settings_the_test_harness_isolates(self):
        """`config.DERIVED` is the source of truth for what `config.use` swaps. A setting
        absent from it is a setting no test can isolate — and this one is declared by
        charter's own dogfood plane, so an unisolated test would assert against whatever
        the machine running the suite happens to commit."""
        self.assertIn("HARNESS", config.DERIVED)

    def test_a_malformed_charter_toml_still_derives_a_usable_harness_setting(self):
        """`derive` swallows a parse error into `CONFIG_ERROR` and carries on, and every
        setting must still have a shape. A `KeyError` out of `cli.main` on a broken plane
        would be a crash report filed against charter for a typo in a TOML file."""
        (self.tmp / "charter.toml").write_text("this is not = valid = toml\n")
        config.use(self.tmp)
        self.assertIsNotNone(config.CONFIG_ERROR)
        self.assertEqual(config.HARNESS, {"default": None, "refused": None})


class _Recorder:
    """Stands in for `commands_frame.cmd_launch` and keeps the args it was handed."""

    def __init__(self) -> None:
        self.calls: list = []

    def __call__(self, args) -> int:
        self.calls.append(args)
        return 0


class _BareLaunchCase(unittest.TestCase):
    """Drives `cli.main`, with the launcher replaced and the plane's setting stated.

    Everything below goes through `cli.main` rather than `_bare_launch`, because the claim
    is about the COMMAND — that bare `charter` reaches the same handler `charter claude`
    reaches, having crossed the same three argv splitters. A test of the helper alone would
    still pass if `main` stopped calling it.
    """

    def setUp(self) -> None:
        # argparse writes its usage error straight to `sys.stderr`, and half the cases
        # below want that to happen — off a buffer it would land in the suite's own output.
        self.enterContext(redirect_stderr(io.StringIO()))
        self.launch = _Recorder()
        self.enterContext(mock.patch.object(commands_frame, "cmd_launch", self.launch))
        # The exec `bypass` would make, pinned as never-happens rather than inferred from
        # `cmd_launch` not being called. It is the actual side effect the hazard is about.
        self.exec = self.enterContext(
            mock.patch("os.execvp", side_effect=AssertionError("charter exec'd a harness")))
        self.err = self.enterContext(mock.patch("charter.util.err"))

    def _declare(self, **harness_setting):
        return mock.patch.object(config, "HARNESS",
                                 {"default": None, "refused": None, **harness_setting})

    def _messages(self) -> str:
        return "\n".join(str(c.args[0]) for c in self.err.call_args_list)


class TheBareCommandLaunchesTheDeclaredHarness(_BareLaunchCase):

    def test_bare_charter_on_a_terminal_reaches_the_launcher(self):
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            rc = cli.main([])
        self.assertEqual(rc, 0)
        self.assertEqual(len(self.launch.calls), 1)
        self.assertEqual(self.launch.calls[0].harness, "claude")

    def test_it_is_the_same_call_typing_the_harness_makes(self):
        """The design in one assertion: bare `charter` is a REWRITE of argv, not a second
        route into `cmd_launch`. Anything the typed form settles — the workspace picker,
        `--probe`, `--no-frame`, `rest` — is settled identically here, because the same
        namespace arrives at the same function."""
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            cli.main([])
            cli.main(["claude"])
        bare, typed = self.launch.calls
        self.assertEqual(vars(bare), vars(typed))

    def test_the_declared_harness_is_the_one_launched(self):
        """Not "whatever is installed" and not "the one last used" — the plane's word."""
        for name in ("claude", "opencode", "codex"):
            with self.subTest(name=name):
                self.launch.calls.clear()
                with self._declare(default=name), \
                     mock.patch("sys.stdout.isatty", return_value=True):
                    cli.main([])
                self.assertEqual(self.launch.calls[0].harness, name)


class TheNonTtyPathStartsNothing(_BareLaunchCase):
    """#687/#690's shape, closed before it shipped.

    `cmd_launch` execs the harness in place of charter when stdout is not a tty. Bare
    `charter` is a usage error today — exit 2, no side effect — so a script may pipe it to
    ask whether charter is there. That must stay true on a plane that sets a default.
    """

    def test_a_piped_bare_charter_prints_usage_and_execs_nothing(self):
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=False):
            with self.assertRaises(SystemExit) as caught:
                cli.main([])
        self.assertEqual(caught.exception.code, 2)
        self.assertEqual(self.launch.calls, [], "a pipe reached the launcher")
        self.exec.assert_not_called()

    def test_the_usage_text_is_the_one_argparse_already_printed(self):
        """Not a new message that happens to exit 2. The claim is that this path is
        UNCHANGED, so the assertion is against argparse's own output on the real parser."""
        buf = io.StringIO()
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=False), \
             mock.patch.object(sys, "stderr", buf):
            with self.assertRaises(SystemExit):
                cli.main([])
        self.assertIn("usage: charter", buf.getvalue())
        self.assertIn("the following arguments are required: command", buf.getvalue())

    def test_the_typed_form_is_untouched_by_the_tty_rule(self):
        """The gate belongs to the BARE command only. `charter claude` piped still reaches
        `cmd_launch`, which is where the bypass decision has always lived — moving that
        decision up here would change what the typed command does."""
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=False):
            cli.main(["claude"])
        self.assertEqual(len(self.launch.calls), 1)


class APlaneThatDeclaredNothingKeepsTodaysUsage(_BareLaunchCase):

    def test_bare_charter_on_a_terminal_still_prints_usage(self):
        """The other half of the design: charter does not guess. A tty is present and a
        harness is installed, and charter still refuses to pick one."""
        with self._declare(), mock.patch("sys.stdout.isatty", return_value=True):
            with self.assertRaises(SystemExit) as caught:
                cli.main([])
        self.assertEqual(caught.exception.code, 2)
        self.assertEqual(self.launch.calls, [])
        self.exec.assert_not_called()


class ARefusedDefaultIsReportedNotSwallowed(_BareLaunchCase):
    """The grill: a committed `default = "clyde"` must fail loudly.

    It degrades to no default, and no default renders as argparse's usage message — which
    is byte-identical to what a plane declaring nothing gets. So the operator who made the
    typo would watch charter behave exactly as though their key were not there.
    """

    def test_the_refusal_is_printed_and_names_the_value(self):
        with self._declare(refused="clyde"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            rc = cli.main([])
        self.assertEqual(rc, 2)
        self.assertIn("clyde", self._messages())
        self.assertEqual(self.launch.calls, [])
        self.exec.assert_not_called()

    def test_the_refusal_names_the_words_that_would_work(self):
        """A refusal that does not say what to write instead sends somebody to the docs
        for a list charter is already holding."""
        with self._declare(refused="clyde"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            cli.main([])
        said = self._messages()
        for name in instance.launchable_harnesses():
            self.assertIn(name, said)

    def test_it_is_reported_off_a_terminal_too(self):
        """A committed file being wrong is not a fact about anybody's terminal, so the
        refusal is checked before the tty rule. Still exit 2 and still nothing started,
        which is what makes it safe to report on the piped path."""
        with self._declare(refused="clyde"), \
             mock.patch("sys.stdout.isatty", return_value=False):
            rc = cli.main([])
        self.assertEqual(rc, 2)
        self.assertIn("clyde", self._messages())
        self.exec.assert_not_called()

    def test_a_typed_subcommand_is_not_blocked_by_a_broken_default(self):
        """The key decides one command. A plane cannot lose `charter status` — or
        `charter claude` — to a typo in a section neither of them reads."""
        with self._declare(refused="clyde"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            cli.main(["claude"])
        self.assertEqual(len(self.launch.calls), 1)
        self.assertEqual(self._messages(), "")


class EveryOtherCommandIsUnchanged(_BareLaunchCase):

    def test_version_is_not_swallowed_by_the_default(self):
        """`--version` is a flag on the ROOT parser, so it is a non-empty argv and the
        rewrite never sees it. `commands_update._handoff` runs the newly installed
        `charter --version` and reads the last word back — a bare launch here would hang
        an upgrade inside a harness."""
        buf = io.StringIO()
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch.object(sys, "stdout", buf):
            with self.assertRaises(SystemExit) as caught:
                cli.main(["--version"])
        self.assertEqual(caught.exception.code, 0)
        self.assertIn("charter", buf.getvalue())
        self.assertEqual(self.launch.calls, [])

    def test_help_is_not_swallowed_by_the_default(self):
        buf = io.StringIO()
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch.object(sys, "stdout", buf):
            with self.assertRaises(SystemExit) as caught:
                cli.main(["--help"])
        self.assertEqual(caught.exception.code, 0)
        self.assertIn("usage: charter", buf.getvalue())
        self.assertEqual(self.launch.calls, [])

    def test_a_core_subcommand_still_dispatches_to_its_own_handler(self):
        called = []
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch("charter.commands.cmd_status", lambda a: called.append(a) or 0):
            rc = cli.main(["status"])
        self.assertEqual(rc, 0)
        self.assertEqual(len(called), 1)
        self.assertEqual(self.launch.calls, [])

    def test_an_unknown_command_is_still_an_unknown_command(self):
        """The gap signal (`report gap`) rides on argparse's refusal of an unrecognised
        first token. A default must not turn a typo into a launch."""
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True), \
             mock.patch.object(sys, "stderr", io.StringIO()):
            with self.assertRaises(SystemExit):
                cli.main(["frobnicate"])
        self.assertEqual(self.launch.calls, [])

    def test_the_harness_flags_still_reach_the_typed_launcher(self):
        """`_split_frame_argv` runs AFTER the rewrite, so it is still the thing that peels
        a harness's own argv off — and the rewrite produces a token it already handles."""
        with self._declare(default="claude"), \
             mock.patch("sys.stdout.isatty", return_value=True):
            cli.main(["claude", "--no-frame", "-p", "hi"])
        args = self.launch.calls[0]
        self.assertTrue(args.no_frame)
        self.assertEqual(args.rest, ["-p", "hi"])


class DoctorNamesASilentlyIgnoredDefault(PersonaIso):
    """The second reader, for the operator who runs `doctor` rather than bare `charter`.

    `config.worktrees_root_for`'s precedent, in its own words: a value refused inside
    `derive` has nobody to tell, so `doctor` asks the same question of the file and names
    it. Every other command on a plane with this typo says nothing at all.
    """

    def _declare(self, text: str):
        (self.tmp / "charter.toml").write_text(text)
        config.use(self.tmp)
        return doctor.check_control_plane_config()

    def test_a_refused_default_is_a_warning_that_names_the_value(self):
        result = self._declare('schema = 1\n[harness]\ndefault = "clyde"\n')
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("clyde", result.detail)
        self.assertIn("[harness]", result.detail)

    def test_the_warning_says_what_the_plane_gets_instead(self):
        """"Ignored" is the fact an operator cannot see for themselves — the usage message
        they are getting is the same one a plane with no key gets."""
        result = self._declare('schema = 1\n[harness]\ndefault = "clyde"\n')
        for name in instance.launchable_harnesses():
            self.assertIn(name, result.hint)

    def test_a_usable_default_is_not_warned_about(self):
        """The precondition. A check that warns about everything is a check nobody reads,
        and one that stopped running looks exactly like one that found nothing."""
        result = self._declare('schema = 1\n[harness]\ndefault = "claude"\n')
        self.assertEqual(result.status, doctor.OK)

    def test_a_plane_that_declares_nothing_is_not_warned_about(self):
        result = self._declare("schema = 1\n")
        self.assertEqual(result.status, doctor.OK)


if __name__ == "__main__":
    unittest.main()
