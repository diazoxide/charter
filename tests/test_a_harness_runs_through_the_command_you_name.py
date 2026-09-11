"""`charter claude` runs the command YOU launch Claude Code with — `.charter/local.toml`.

An operator who reaches Claude Code through a wrapper — `ccs work`, which picks the
subscription a chat is billed to; a script that pins a binary off `$PATH` — had two doors,
both wrong. `charter frame -- ccs work` runs the words but forgets the harness: no
`$CHARTER_HARNESS` in the pane, no workspace layer, nothing `charter reopen` can resume. A
shell alias is invisible to charter, which execs `claude` by name. `[harness.<name>]
command` in the plane's `.charter/local.toml` is the third door: the REGISTERED harness,
launched through the operator's own words, with everything the registration carries.

**Per developer, never committed — and that is the design rather than a limitation.**
The containment rule (README) says a name charter reads out of a committed file cannot
choose what it runs; `instance.harness_of` refuses ``[harness] default = "clyde"`` on
exactly those grounds, and a ``command`` in `charter.toml` would hand a cloned plane
``argv[0]`` of what runs on a teammate's machine. It is also the right scope on the
merits: which account a chat is billed to is the operator's business, and a teammate on
the same plane may well run plain `claude`. `.charter/` is gitignored on every plane
`init` ever wrote, its files are one account's (`config.STATE_FILE_MODE`), and it already
holds the per-developer half of the vault registry over the committed one — the precedent
this follows.

Three things are new and each is tested here rather than described:

* a second TOML file, ``<state dir>/local.toml``, parsed by `instance.load_local` and
  degraded — never raised — by `config.derive`, the way `charter.toml` is;
* a refusal at the CONFIG boundary for a table charter cannot honour, carried out to the
  two readers that can say so (`doctor`, `charter harness list`) rather than degrading into
  silence — the #535 shape, a declared thing that is not in force;
* the launcher handing the command to tmux in the binary's place, and asking `$PATH` about
  the command's first word rather than about a binary it is no longer going to run.
"""

from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_frame, commands_harness, config, doctor, instance
from tests._isolation import PersonaIso
# Imported at MODULE level, deliberately: `test_frame_launcher` captures the real
# `config.STATE_DIR` at import so `_refuse_the_real_plane` can compare against it. Imported
# inside a `PersonaIso` case it would capture the case's tmp instead, and then refuse that
# very case as "the real plane".
from tests.test_frame_launcher import _FakeTmux, _launch

_NONE = {"command": {}, "refused": []}


def _run_capturing(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


class TheOverrideIsReadAtTheConfigBoundary(unittest.TestCase):
    """`instance.harness_local_of` — pure, no plane, no filesystem."""

    def test_a_file_that_declares_nothing_overrides_nothing(self):
        self.assertEqual(instance.harness_local_of({}), _NONE)

    def test_a_declared_command_is_stored_for_the_harness_it_names(self):
        got = instance.harness_local_of({"harness": {"claude": {"command": ["ccs", "work"]}}})
        self.assertEqual(got, {"command": {"claude": ["ccs", "work"]}, "refused": []})

    def test_every_registered_harness_can_carry_one(self):
        for name in instance.launchable_harnesses():
            with self.subTest(harness=name):
                got = instance.harness_local_of({"harness": {name: {"command": ["run-it"]}}})
                self.assertEqual(got["command"], {name: ["run-it"]})
                self.assertEqual(got["refused"], [])

    def test_two_harnesses_can_each_carry_their_own(self):
        got = instance.harness_local_of({"harness": {
            "claude": {"command": ["ccs", "work"]},
            "codex": {"command": ["/opt/codex/bin/codex"]},
        }})
        self.assertEqual(got["command"], {"claude": ["ccs", "work"],
                                          "codex": ["/opt/codex/bin/codex"]})

    def test_a_command_for_a_harness_charter_cannot_launch_is_refused_and_named(self):
        """The refusal asserts the REASON: a `command` dict that came back empty proves
        nothing on its own, because that is also what a file declaring nothing gets."""
        got = instance.harness_local_of({"harness": {"clyde": {"command": ["x"]}}})
        self.assertEqual(got["command"], {})
        self.assertEqual(len(got["refused"]), 1)
        self.assertIn("clyde", got["refused"][0])

    def test_the_tables_are_named_by_the_words_an_operator_types(self):
        """`claude`, not `claude-code`: the same rule `[harness] default` follows, because
        it is the same word — the one after `charter` on the command line."""
        got = instance.harness_local_of({"harness": {"claude-code": {"command": ["x"]}}})
        self.assertEqual(got["command"], {})
        self.assertIn("claude-code", got["refused"][0])

    def test_a_command_that_is_not_a_list_of_words_is_refused_and_named(self):
        """tmux and `execvp` take a LIST; `"ccs work"` is one word that names no program.
        Refused rather than split, because splitting is a shell's job and there is no
        shell here on purpose (`harness.base.launch_argv`). The other shapes are
        `tomllib` handing over something declared and unusable — as declared, and as not
        in force, as the string."""
        for value in ("ccs work", ["ccs", 1], [], [""], 7, True, {"words": ["ccs"]}):
            with self.subTest(value=value):
                got = instance.harness_local_of({"harness": {"claude": {"command": value}}})
                self.assertEqual(got["command"], {})
                self.assertEqual(len(got["refused"]), 1, got)
                self.assertIn("claude", got["refused"][0])

    def test_a_harness_entry_that_is_not_a_table_is_refused_and_named(self):
        """``[harness] claude = "ccs work"`` — the shape of a likely typo, and a declared
        thing. Silently reading it as no table would be the #535 failure."""
        got = instance.harness_local_of({"harness": {"claude": "ccs work"}})
        self.assertEqual(got["command"], {})
        self.assertIn("claude", got["refused"][0])

    def test_a_table_with_no_command_key_is_not_a_refusal(self):
        got = instance.harness_local_of({"harness": {"claude": {"nothing": "here"}}})
        self.assertEqual(got, _NONE)

    def test_a_section_that_is_not_a_section_degrades_rather_than_raising(self):
        for section in ("claude", 3, None, ["claude"]):
            with self.subTest(section=section):
                self.assertEqual(instance.harness_local_of({"harness": section}), _NONE)

    def test_a_refused_value_cannot_forge_a_second_report_line(self):
        """The refusal is a sentence `doctor` prints; the value in it comes off a file."""
        got = instance.harness_local_of(
            {"harness": {"claude": {"command": "ccs\n✓ charter: pwned"}}})
        self.assertEqual(len(got["refused"]), 1)
        self.assertNotIn("\n", got["refused"][0])

    def test_a_refused_name_cannot_forge_a_second_report_line_either(self):
        """TOML quotes a key, so a harness NAME can carry a newline as readily as a value
        — and so can the value sitting where a table should be."""
        got = instance.harness_local_of({"harness": {
            "cly\nde": {"command": ["x"]},
            "claude": "ccs\n✓ charter: pwned",
        }})
        self.assertEqual(len(got["refused"]), 2)
        for line in got["refused"]:
            self.assertNotIn("\n", line)

    def test_refused_is_a_list_on_every_path(self):
        """`frame_of`'s rule for `components`: a key present on one path and absent on
        another is two shapes for one answer."""
        for cfg in ({}, {"harness": None}, {"harness": {}},
                    {"harness": {"claude": {"command": ["ccs"]}}}):
            with self.subTest(cfg=cfg):
                self.assertIsInstance(instance.harness_local_of(cfg)["refused"], list)


class TheFileIsReadFromTheStateDir(PersonaIso):
    """`config.HARNESS_LOCAL` derives from ``<state dir>/local.toml``, like `config.HARNESS`
    derives from `charter.toml` — and from the STATE dir, not the plane root, because that
    is the per-developer directory, gitignored on every plane and redirected as one by
    ``$CHARTER_HOME``."""

    def _declare(self, text: str) -> None:
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        (config.STATE_DIR / "local.toml").write_text(text)
        config.use(self.tmp)

    def test_a_local_file_that_declares_a_command_reaches_config(self):
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        self.assertEqual(config.HARNESS_LOCAL,
                         {"command": {"claude": ["ccs", "work"]}, "refused": []})
        self.assertIsNone(config.LOCAL_CONFIG_ERROR)

    def test_a_plane_with_no_local_file_derives_no_override(self):
        self.assertFalse((config.STATE_DIR / "local.toml").exists())
        self.assertEqual(config.HARNESS_LOCAL, _NONE)
        self.assertIsNone(config.LOCAL_CONFIG_ERROR)

    def test_the_path_is_a_derived_setting_so_reports_can_name_it(self):
        self.assertEqual(config.LOCAL_CONFIG, config.STATE_DIR / "local.toml")

    def test_a_malformed_local_file_is_recorded_and_still_derives_a_usable_shape(self):
        """`config` is imported by every command including `charter --version`; a typo in
        a per-developer file must not take the CLI down, and every setting must still
        have a shape a caller can read without a `KeyError`."""
        self._declare("this is not = valid = toml\n")
        self.assertIsNotNone(config.LOCAL_CONFIG_ERROR)
        self.assertEqual(config.HARNESS_LOCAL, _NONE)

    def test_a_refused_declaration_reaches_config_as_a_refusal(self):
        self._declare('[harness.clyde]\ncommand = ["x"]\n')
        self.assertEqual(config.HARNESS_LOCAL["command"], {})
        self.assertIn("clyde", config.HARNESS_LOCAL["refused"][0])

    def test_the_settings_are_ones_the_test_harness_isolates(self):
        """`config.DERIVED` is what `config.use` swaps. A setting absent from it is one no
        test can isolate — and this one is read off the developer's own machine."""
        for name in ("HARNESS_LOCAL", "LOCAL_CONFIG_ERROR", "LOCAL_CONFIG"):
            self.assertIn(name, config.DERIVED)

    def test_bytes_that_are_not_utf8_are_refused_as_not_toml(self):
        """`load_local`'s contract is `load`'s: one exception class for "cannot read this
        as TOML", so `config.derive` has one thing to catch and `doctor` one thing to
        print. A raw `UnicodeDecodeError` would be a second."""
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        (config.STATE_DIR / "local.toml").write_bytes(b"\xff\xfe[harness]")
        with self.assertRaises(ValueError):
            instance.load_local(config.STATE_DIR)

    def test_a_state_dir_that_does_not_exist_yet_is_an_empty_file(self):
        """A fresh clone has no `.charter/` at all until something writes one."""
        self.assertEqual(instance.load_local(self.tmp / "never-made"), {})

    def test_the_file_follows_the_state_dir_when_charter_home_moves_it(self):
        """``$CHARTER_HOME`` is documented as the way to share one `.charter/` across
        clones; a per-developer override travels with it."""
        home = self.tmp / "elsewhere"
        home.mkdir()
        (home / "local.toml").write_text('[harness.claude]\ncommand = ["ccs", "personal"]\n')
        with mock.patch.dict(os.environ, {"CHARTER_HOME": str(home)}):
            config.use(self.tmp)
            self.assertEqual(config.HARNESS_LOCAL["command"], {"claude": ["ccs", "personal"]})


class TheLaunchRunsTheCommand(PersonaIso):
    """`commands_frame.cmd_launch`, on both of its paths."""

    def _declare(self, text: str) -> None:
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        (config.STATE_DIR / "local.toml").write_text(text)
        config.use(self.tmp)

    def test_a_bare_launch_execs_the_command_with_the_operators_arguments_after_it(self):
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        args = SimpleNamespace(harness="claude", rest=["-p", "hi"], no_frame=True)
        with mock.patch("os.execvp") as execvp:
            commands_frame.cmd_launch(args)
        execvp.assert_called_once_with("ccs", ["ccs", "work", "-p", "hi"])

    def test_a_plane_with_no_override_execs_the_binary_as_before(self):
        args = SimpleNamespace(harness="claude", rest=["-p", "hi"], no_frame=True)
        with mock.patch("os.execvp") as execvp:
            commands_frame.cmd_launch(args)
        execvp.assert_called_once_with("claude", ["claude", "-p", "hi"])

    def test_the_framed_launch_hands_tmux_the_command(self):
        """The `new-session` that starts the chat carries the command's words where the
        binary used to be — verbatim, one argv element each, never joined."""
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        fake = _FakeTmux(exit_code=0)
        with mock.patch("os.execvp", side_effect=AssertionError("bypassed the frame")):
            rc = _launch(fake, harness="claude", rest=["--resume", "abc"])
        self.assertEqual(rc, 0)
        starts = [c for c in fake.calls if "new-session" in c]
        self.assertTrue(starts, fake.calls)
        words = starts[0]
        at = words.index("ccs")
        self.assertEqual(words[at:at + 4], ["ccs", "work", "--resume", "abc"])
        self.assertNotIn("claude", words[at:])

    def test_the_pre_tmux_check_asks_about_the_command_not_the_binary(self):
        """`cmd_launch` refuses to reach tmux for a registered harness whose binary is not
        on `$PATH`, because a `new-session` whose exec fails draws nothing at all. With a
        command in force the binary is not what is about to be exec'd, so `claude` being
        installed answers the wrong question: `ccs` missing has to be what is refused,
        before tmux, and named."""
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        fake = _FakeTmux(exit_code=0)
        buf = []
        with mock.patch("os.execvp", side_effect=FileNotFoundError(2, "No such file")), \
             mock.patch("charter.util.err", side_effect=lambda m: buf.append(m)):
            rc = _launch(fake, harness="claude",
                         which=lambda name, *a, **k: None if name == "ccs"
                         else f"/usr/bin/{name}")
        self.assertEqual(rc, 127)
        self.assertTrue(any("ccs" in m for m in buf), buf)
        self.assertFalse([c for c in fake.calls if "new-session" in c],
                         "refused AFTER tmux — the operator was left with nothing drawn")

    def test_a_binary_off_the_path_does_not_stop_a_command_that_is_on_it(self):
        """The other direction: the wrapper is what runs, so the wrapper is what has to
        be findable — a `claude` that `$PATH` cannot see is the wrapper's problem to
        solve, not charter's to refuse."""
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        fake = _FakeTmux(exit_code=0)
        with mock.patch("os.execvp", side_effect=AssertionError("bypassed the frame")):
            rc = _launch(fake, harness="claude",
                         which=lambda name, *a, **k: None if name == "claude"
                         else f"/usr/bin/{name}")
        self.assertEqual(rc, 0)
        self.assertTrue([c for c in fake.calls if "new-session" in c])


class DoctorNamesAnOverrideNotInForce(PersonaIso):
    """The reader for the operator who runs `doctor`. A value refused inside `derive` has
    nobody to tell — `charter claude` deliberately prints nothing before tmux takes the
    screen (`commands_frame.frame_ready`) — so this row asks the same question of the file
    and names it, the way the `charter.toml` row names a refused `[harness] default`."""

    def _declare(self, text: str):
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        (config.STATE_DIR / "local.toml").write_text(text)
        config.use(self.tmp)
        return doctor.check_local_config()

    def test_the_row_is_named_for_the_file(self):
        self.assertEqual(doctor.check_local_config().name, "local.toml")

    def test_no_file_is_not_a_problem_and_says_there_is_nothing_in_force(self):
        result = doctor.check_local_config()
        self.assertEqual(result.status, doctor.OK)
        self.assertIn("no per-developer overrides", result.detail)

    def test_a_command_in_force_is_green_and_says_what_runs(self):
        result = self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        self.assertEqual(result.status, doctor.OK)
        self.assertIn("ccs work", result.detail)

    def test_a_refused_table_is_a_warning_that_names_it(self):
        result = self._declare('[harness.clyde]\ncommand = ["x"]\n')
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("clyde", result.detail + result.hint)
        for name in instance.launchable_harnesses():
            self.assertIn(name, result.hint)

    def test_more_than_three_refusals_are_clipped_rather_than_printed_whole(self):
        """One row is one row. The `[[forge]]` branch of the `charter.toml` row clips at
        three for the same reason, and the fourth name is what proves the clip happened."""
        result = self._declare("".join(f'[harness.wrong{i}]\ncommand = ["x"]\n'
                                       for i in range(4)))
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("wrong2", result.hint)
        self.assertNotIn("wrong3", result.hint)
        self.assertIn("…", result.hint)

    def test_a_word_of_the_command_cannot_forge_a_second_row(self):
        """A list of non-empty strings is a VALID command — the operator's own words, out
        of their own file — and one of them can still hold a newline. This is charter's
        report format, and a newline in a detail is a second row."""
        result = self._declare('[harness.claude]\ncommand = ["ccs", "work\\n✓ pwned"]\n')
        self.assertEqual(result.status, doctor.OK)
        self.assertNotIn("\n", result.detail)

    def test_a_malformed_file_is_a_warning_that_names_the_parse_error(self):
        """A warning and not a failure: charter carries on with no override, which is a
        working plane — the operator just is not getting what they wrote."""
        result = self._declare("this is not = valid = toml\n")
        self.assertEqual(result.status, doctor.WARN)
        self.assertIn("TOML", result.detail + result.hint)

    def test_the_row_is_in_the_preflight_and_its_name_is_pinned(self):
        """`_FIXED_CHECK_NAMES` sizes the table before a single check runs; a row added to
        `_checks` and not to it fails the width pin. Beside `charter.toml`, which is the
        row it is the per-developer half of."""
        names = doctor.check_names()
        self.assertIn("local.toml", names)
        self.assertEqual(names.index("local.toml"), names.index("charter.toml") + 1)


class HarnessListSaysWhatRuns(PersonaIso):
    """The second reader, for an operator asking charter what it knows about a harness."""

    def _declare(self, text: str) -> None:
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        (config.STATE_DIR / "local.toml").write_text(text)
        config.use(self.tmp)

    def test_a_harness_with_a_command_in_force_shows_it(self):
        self._declare('[harness.claude]\ncommand = ["ccs", "work"]\n')
        _rc, text = _run_capturing(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertIn("ccs work", text)
        self.assertLess(text.index("claude-code"), text.index("ccs work"))
        self.assertLess(text.index("ccs work"), text.index("opencode"),
                        "the command belongs under the harness it launches")

    def test_a_word_of_the_command_cannot_forge_a_second_row(self):
        self._declare('[harness.claude]\ncommand = ["ccs", "work\\n* opencode"]\n')
        _rc, text = _run_capturing(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertNotIn("\n* opencode", text,
                         "a word out of the operator's file drew a second harness row")
        self.assertIn("ccs work", text)

    def test_a_harness_without_one_shows_nothing_new(self):
        _rc, text = _run_capturing(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertNotIn("runs", text)


if __name__ == "__main__":
    unittest.main()
