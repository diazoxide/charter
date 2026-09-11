"""A harness profile is read from `charter.local.toml`, and a broken one is refused alone.

Charter launched one program per harness kind, one way (`Harness.binary`), so an operator
with a work and a personal Claude Code account, or a Codex pinned to an older release, had
no way to tell charter so. A profile is that way: a kind, a command and an environment,
declared in a file that stays on this machine
(`docs/superpowers/specs/2026-09-11-harness-profiles.md`). These cases pin what is read,
what is refused and why, and that reading costs no subprocess, because every hook process
derives config.

The filename is spelled out in every fixture rather than read off `profiles.LOCAL_FILE`: a
round trip through the constant cannot pin the name an operator types.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import cli, config, instance, profiles
from tests._isolation import PersonaIso

#: The spec's own example: a second Claude Code account in its own config folder.
_WORK = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }
"""

#: A good profile declared beside every broken one, so "refused alone" is observable.
_OK = """
[harness.ok]
kind = "claude"
command = ["claude"]
"""

_BUILT_INS = {"claude", "codex", "opencode"}


class _LocalFile(PersonaIso):
    """Writes the throwaway plane's local file and reads profiles the two ways charter does."""

    def _local(self, text: str) -> dict:
        (config.ROOT / "charter.local.toml").write_text(text)
        return profiles.derive(config.ROOT, instance.load(config.ROOT))

    def _current(self, text: str) -> dict:
        (config.ROOT / "charter.local.toml").write_text(text)
        # Re-derived, so `config.PROFILES` is this plane's; `PersonaIso`'s own snapshot is
        # the one its cleanup puts back.
        config.use(config.ROOT)
        return profiles.current()

    def _charter_toml(self, text: str) -> None:
        (config.ROOT / "charter.toml").write_text("schema = 1\n" + text)

    def _read(self) -> dict:
        return profiles.derive(config.ROOT, instance.load(config.ROOT))


class TheLocalFileIsRead(_LocalFile):
    def test_a_plane_with_no_local_file_has_exactly_the_built_ins(self):
        """A plane that declared nothing sees no change: every kind is still launchable as
        the word it always was."""
        r = self._read()
        self.assertEqual(set(r["profiles"]), _BUILT_INS)
        for p in r["profiles"].values():
            self.assertEqual(p.source, "built-in")
        self.assertEqual(r["refused"], ())

    def test_a_built_in_runs_its_kind_with_no_environment(self):
        self.assertEqual(self._read()["profiles"]["claude"],
                         profiles.Profile("claude", "claude", "claude-code", ("claude",), (),
                                          "built-in"))

    def test_a_declared_profile_is_read_with_its_kind_command_and_env(self):
        p = self._local(_WORK)["profiles"]["claude-work"]
        self.assertEqual(p.kind, "claude")
        self.assertEqual(p.harness, "claude-code")
        self.assertEqual(p.command, ("claude",))
        self.assertEqual(p.env, (("CLAUDE_CONFIG_DIR", "~/.claude-work"),))
        self.assertEqual(p.source, "charter.local.toml")

    def test_a_declared_profile_replaces_the_built_in_of_its_name(self):
        """How plain `claude` gets pinned."""
        r = self._local('[harness.claude]\nkind = "claude"\ncommand = ["/opt/claude"]\n')
        self.assertEqual(r["profiles"]["claude"].command, ("/opt/claude",))
        self.assertEqual(r["profiles"]["claude"].source, "charter.local.toml")

    def test_env_is_optional(self):
        self.assertEqual(self._local(_OK)["profiles"]["ok"].env, ())

    def test_env_is_kept_sorted_by_name(self):
        """One order for one environment, so two reads of the same file compare equal."""
        r = self._local('[harness.w]\nkind = "claude"\ncommand = ["claude"]\n'
                        'env = { Z_LAST = "z", A_FIRST = "a" }\n')
        self.assertEqual(r["profiles"]["w"].env, (("A_FIRST", "a"), ("Z_LAST", "z")))

    def test_the_local_default_wins_over_charter_toml(self):
        self._charter_toml('[harness]\ndefault = "codex"\n')
        r = self._local('[harness]\ndefault = "claude-work"\n' + _WORK)
        self.assertEqual(r["default"], "claude-work")
        self.assertEqual(r["default_from"], "charter.local.toml")
        self.assertIsNone(r["default_refused"])

    def test_a_default_naming_no_profile_is_refused_by_value(self):
        r = self._local('[harness]\ndefault = "nope"\n')
        self.assertIsNone(r["default"])
        self.assertEqual(r["default_refused"], "nope")

    def test_a_default_that_is_not_text_is_refused_by_value(self):
        """`tomllib` hands back lists and tables too. A value of the wrong type is as
        declared, and as not in force, as a misspelt name — `instance.harness_of`'s rule."""
        r = self._local('[harness]\ndefault = ["claude"]\n')
        self.assertIsNone(r["default"])
        self.assertIn("claude", r["default_refused"])

    def test_no_default_declared_is_not_a_refusal(self):
        r = self._read()
        self.assertIsNone(r["default"])
        self.assertIsNone(r["default_from"])
        self.assertIsNone(r["default_refused"])


class WhatALaunchWillBeHanded(_LocalFile):
    """The command and environment as declared, and the two expansions no shell does."""

    def test_a_leading_tilde_is_expanded_in_the_first_word_and_every_env_value(self):
        r = self._local('[harness.w]\nkind = "claude"\n'
                        'command = ["~/bin/claude", "~/not-expanded"]\n'
                        'env = { CLAUDE_CONFIG_DIR = "~/.claude-work", B = "~/b" }\n')
        p = r["profiles"]["w"]
        with mock.patch.dict(os.environ, {"HOME": "/home/op"}):
            self.assertEqual(profiles.expanded_command(p),
                             ["/home/op/bin/claude", "~/not-expanded"])
            self.assertEqual(profiles.expanded_env(p),
                             {"B": "/home/op/b", "CLAUDE_CONFIG_DIR": "/home/op/.claude-work"})
        self.assertEqual(p.command, ("~/bin/claude", "~/not-expanded"))

    def test_display_shows_the_environment_then_the_command(self):
        p = self._local(_WORK)["profiles"]["claude-work"]
        self.assertEqual(profiles.display(p), "CLAUDE_CONFIG_DIR=~/.claude-work claude")

    def test_display_escapes_control_bytes_in_an_env_value_too(self):
        """Ruling 35: a value from a file a chat can write is shown, never interpreted."""
        p = self._local('[harness.w]\nkind = "claude"\ncommand = ["claude"]\n'
                        'env = { CLAUDE_CONFIG_DIR = "~/x\\r\\u001B[2Kfine" }\n')["profiles"]["w"]
        shown = profiles.display(p)
        self.assertNotIn("\r", shown)
        self.assertNotIn("\x1b", shown)


class ABrokenProfileIsRefusedAlone(_LocalFile):
    def _refused(self, body: str, name: str, *, read: dict | None = None) -> str:
        r = read if read is not None else self._local(_OK + body)
        self.assertIn("ok", r["profiles"], "a broken profile took a good one down with it")
        self.assertNotIn(name, r["profiles"])
        reasons = [x.reason for x in r["refused"] if x.name == name]
        self.assertEqual(len(reasons), 1, r["refused"])
        return reasons[0]

    def test_an_unknown_kind_is_refused_naming_the_kinds(self):
        for kind in ('kind = "gemini"\n', "", 'kind = "claude-code"\n'):
            with self.subTest(kind=kind):
                reason = self._refused(f'[harness.gem]\n{kind}command = ["gemini"]\n', "gem")
                self.assertIn("claude, opencode, codex", reason)

    def test_a_command_written_as_a_string_is_refused(self):
        reason = self._refused('[harness.s]\nkind = "claude"\ncommand = "claude --resume x"\n',
                               "s")
        self.assertIn("never a shell string", reason)

    def test_an_empty_command_is_refused(self):
        for command in ("command = []\n", ""):
            with self.subTest(command=command):
                reason = self._refused(f'[harness.e]\nkind = "claude"\n{command}', "e")
                self.assertIn("never a shell string", reason)

    def test_a_command_holding_a_non_string_is_refused(self):
        for command in ('["claude", 3]', '[""]'):
            with self.subTest(command=command):
                reason = self._refused(f'[harness.n]\nkind = "claude"\ncommand = {command}\n',
                                       "n")
                self.assertIn("never a shell string", reason)

    def test_a_profile_named_default_is_refused(self):
        """`default` is the one key under `[harness]` that is not a profile."""
        reason = self._refused('[harness.default]\nkind = "claude"\ncommand = ["claude"]\n',
                               "default")
        self.assertIn("cannot be named 'default'", reason)

    def test_a_name_with_a_dot_is_refused(self):
        """Ruling 5: a dot in a name broke tmux targets in #695."""
        reason = self._refused('[harness."claude.work"]\nkind = "claude"\ncommand = ["claude"]\n',
                               "claude.work")
        self.assertIn("letters, digits", reason)

    def test_a_name_with_a_trailing_newline_is_refused(self):
        """`$` matches before a final newline, so the alphabet has to be a whole match."""
        r = self._local(_OK + '[harness."work\\n"]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIn("ok", r["profiles"])
        self.assertNotIn("work\n", r["profiles"])
        (refusal,) = [x for x in r["refused"] if "letters, digits" in x.reason]
        self.assertNotIn("\n", refusal.name)

    def test_a_profile_that_is_not_a_table_is_refused_by_name(self):
        reason = self._refused('[harness]\nwork = "claude"\n', "work")
        self.assertIn("is not a table", reason)

    def test_an_env_that_is_not_text_is_refused(self):
        for env in ("env = { N = 3 }\n", 'env = "CLAUDE_CONFIG_DIR=~/x"\n'):
            with self.subTest(env=env):
                reason = self._refused(f'[harness.v]\nkind = "claude"\ncommand = ["claude"]\n{env}',
                                       "v")
                self.assertIn("table of text values", reason)

    def test_an_unknown_profile_key_is_refused(self):
        """Review 13: a typo'd `env` would otherwise drop `CLAUDE_CONFIG_DIR` and launch the
        default account without a word."""
        reason = self._refused('[harness.t]\nkind = "claude"\ncommand = ["claude"]\n'
                               'enviroment = { CLAUDE_CONFIG_DIR = "~/.claude-work" }\n', "t")
        self.assertIn("does not read", reason)

    def test_a_refused_replacement_of_a_built_in_takes_its_name_with_it(self):
        """Review 13's reason, followed through. The operator declared how `claude` runs; if
        that declaration is refused and the built-in stood in, `claude` would run the default
        account — the launch the refusal exists to stop, and ruling 19's reason for refusing
        a replaced built-in rather than falling back to it."""
        reason = self._refused('[harness.claude]\nkind = "claude"\ncommand = ["claude"]\n'
                               'enviroment = { CLAUDE_CONFIG_DIR = "~/.claude-work" }\n',
                               "claude")
        self.assertIn("does not read", reason)

    def test_an_env_name_starting_with_charter_is_refused(self):
        """Ruling 14: it would tell every hook the wrong harness or plane."""
        for var in ('CHARTER_HARNESS = "codex"', 'charter_root = "/elsewhere"'):
            with self.subTest(var=var):
                reason = self._refused(f'[harness.c]\nkind = "claude"\ncommand = ["claude"]\n'
                                       f'env = {{ {var} }}\n', "c")
                self.assertIn("one of charter's own variables", reason)

    def test_a_command_that_is_charter_itself_is_refused_by_current(self):
        """Ruling 14: a new chat on it would open the selector again, forever."""
        for command in ('["charter"]', '["/usr/local/bin/charter", "claude"]',
                        '["python3", "-m", "charter"]', '["edm"]'):
            with self.subTest(command=command):
                body = f'[harness.loop]\nkind = "claude"\ncommand = {command}\n'
                reason = self._refused(body, "loop", read=self._current(_OK + body))
                self.assertIn("runs charter itself", reason)
        read = self._current(_OK + '[harness.near]\nkind = "claude"\ncommand = ["charterize"]\n')
        self.assertIn("near", read["profiles"])

    def test_a_replacement_that_runs_charter_takes_its_name_with_it_too(self):
        read = self._current(_OK + '[harness.claude]\nkind = "claude"\ncommand = ["charter"]\n')
        self._refused("", "claude", read=read)

    def test_a_secret_shaped_env_name_is_refused_for_each_word(self):
        """Anything set on the harness reaches the model's own shell (measured on Claude Code
        and Codex), so charter holds no credential in a profile and points at the harness's
        own login instead. The value is never repeated back."""
        for var in ("MY_KEY", "MY_TOKEN", "MY_SECRET", "MY_PASSWORD", "api_key"):
            with self.subTest(var=var):
                body = (f'[harness.leaky]\nkind = "claude"\ncommand = ["claude"]\n'
                        f'env = {{ {var} = "sk-live-123" }}\n')
                r = self._local(_OK + body)
                reason = self._refused(body, "leaky", read=r)
                self.assertIn(var, reason)
                self.assertIn("CLAUDE_CONFIG_DIR and run /login", reason)
                self.assertNotIn("sk-live-123", repr(r["refused"]))
        for kind, login in (("codex", "CODEX_HOME and run codex login"),
                            ("opencode", "XDG_DATA_HOME and run opencode auth login")):
            with self.subTest(kind=kind):
                body = (f'[harness.leaky]\nkind = "{kind}"\ncommand = ["{kind}"]\n'
                        f'env = {{ API_TOKEN = "sk-live-123" }}\n')
                reason = self._refused(body, "leaky")
                self.assertIn(login, reason)
                self.assertNotIn("sk-live-123", reason)

    def test_every_kind_charter_can_launch_names_its_login(self):
        """A kind registered without a login sentence would refuse a credential and point at
        nothing in particular."""
        from charter.harness import registry

        for h in registry.all():
            if h.cli_name:
                with self.subTest(harness=h.name):
                    self.assertIn(h.name, profiles.LOGIN)

    def test_an_unreadable_local_file_keeps_the_built_ins(self):
        r = self._local(_OK + "[harness")
        self.assertEqual(set(r["profiles"]), _BUILT_INS)
        self.assertEqual(len(r["refused"]), 1)
        self.assertEqual(r["refused"][0].name, "")
        self.assertEqual(r["refused"][0].source, "charter.local.toml")

    def test_a_harness_that_is_not_a_table_is_refused_whole(self):
        r = self._local('harness = "claude-work"\n')
        self.assertEqual(set(r["profiles"]), _BUILT_INS)
        self.assertEqual([x.name for x in r["refused"]], [""])

    def test_a_section_other_than_harness_is_refused_by_name(self):
        """An ignored file must not change plane policy with no trace in git: `[[forge]]`
        hosts steer the credential guard."""
        reason = self._refused('[forge]\nhost = "example.com"\n', "forge")
        self.assertIn("charter.toml", reason)

    def test_each_refusal_says_a_different_thing(self):
        """Every refusal is asked about one profile named `x` where its rule allows, so two
        rules sharing a sentence cannot hide behind two different names."""
        profile = '[harness.x]\nkind = "claude"\ncommand = ["claude"]\n'
        bodies = [
            '[harness."x.y"]\nkind = "claude"\ncommand = ["claude"]\n',
            '[harness.default]\nkind = "claude"\ncommand = ["claude"]\n',
            '[harness.x]\nkind = "gemini"\ncommand = ["claude"]\n',
            '[harness.x]\nkind = "claude"\ncommand = "claude"\n',
            profile + 'env = { N = 3 }\n',
            profile + 'env = { CHARTER_ROOT = "/x" }\n',
            profile + 'env = { API_KEY = "v" }\n',
            profile + 'enviroment = {}\n',
            '[harness]\nx = "claude"\n',
            '[x]\n',
            '[harness',
        ]
        reasons = [self._local(b)["refused"][0].reason for b in bodies]
        reasons.append(self._current('[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
                       ["refused"][0].reason)
        reasons.append(self._current('[harness.x]\nkind = "claude"\ncommand = ["charter"]\n')
                       ["refused"][0].reason)
        (config.ROOT / "charter.local.toml").unlink()
        self._charter_toml(profile)
        reasons.append(self._read()["refused"][0].reason)
        self.assertEqual(len(set(reasons)), len(reasons), reasons)


class CharterTomlCarriesNoProfile(_LocalFile):
    def test_a_profile_table_in_charter_toml_is_refused_with_a_pointer(self):
        """A committed command could be changed by a merged PR or by a chat and then run on
        every machine, so the committed file is refused as a home for one."""
        self._charter_toml('[harness.x]\nkind = "claude"\ncommand = ["claude"]\n')
        r = self._read()
        (refusal,) = r["refused"]
        self.assertEqual(refusal.source, "charter.toml")
        self.assertIn("move the table", refusal.reason.lower())
        self.assertNotIn("x", r["profiles"])

    def test_a_committed_table_named_like_a_built_in_leaves_the_built_in(self):
        """charter.toml's tables are not read as profiles at all, so one there has no power
        to take a launcher away from somebody else's machine."""
        self._charter_toml('[harness.claude]\nkind = "claude"\ncommand = ["/opt/claude"]\n')
        r = self._read()
        self.assertEqual(r["profiles"]["claude"].source, "built-in")
        self.assertEqual([x.source for x in r["refused"]], ["charter.toml"])

    def test_charter_toml_default_is_still_read(self):
        self._charter_toml('[harness]\ndefault = "codex"\n')
        r = self._read()
        self.assertEqual(r["default"], "codex")
        self.assertEqual(r["default_from"], "charter.toml")

    def test_other_keys_in_charter_tomls_harness_are_still_ignored(self):
        """Pin. Review 13 dropped this refusal: no `doctor` warning on a plane that did
        nothing wrong."""
        self._charter_toml('[harness]\nnothing = "here"\n')
        self.assertEqual([x for x in self._read()["refused"] if x.source == "charter.toml"], [])


class NamesThatClashWithACommand(_LocalFile):
    def test_a_profile_named_like_a_command_is_refused_by_current(self):
        """The kind clash in `cli._add_frame_parsers` raises and takes every command down,
        which is right for a registry mistake CI sees and wrong for one machine's file."""
        read = self._current(_OK + '[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIn("ok", read["profiles"])
        self.assertNotIn("doctor", read["profiles"])
        (refusal,) = [x for x in read["refused"] if x.name == "doctor"]
        self.assertIn("charter doctor", refusal.reason)

    def test_a_default_naming_a_clashing_profile_is_refused(self):
        read = self._current('[harness]\ndefault = "doctor"\n'
                             '[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIsNone(read["default"])
        self.assertEqual(read["default_refused"], "doctor")

    def test_the_command_words_hold_the_core_commands_and_not_the_kinds(self):
        self.assertLessEqual({"doctor", "workspace", "frame", "frame-new-chat"},
                             cli.command_words())
        self.assertNotIn("claude", cli.command_words())


class TheConfigReadIsCheap(_LocalFile):
    def test_deriving_profiles_runs_no_subprocess(self):
        """Every hook process runs `config.derive`, and `hooks/hooks.json` fires on Bash,
        Read, Grep, Write, Edit, Task, Skill and SendMessage — so the git check belongs to
        the surfaces a person runs, never to this read. The file here is one git would
        commit, which is exactly when a git call would be tempting."""
        (config.ROOT / "charter.local.toml").write_text(_WORK)
        with mock.patch("subprocess.run", side_effect=AssertionError("a config read ran git")), \
             mock.patch("subprocess.Popen", side_effect=AssertionError("a config read spawned")):
            config.use(config.ROOT)
        self.assertIn("claude-work", config.PROFILES["profiles"])

    def test_config_carries_the_profiles(self):
        (config.ROOT / "charter.local.toml").write_text(_WORK)
        config.use(config.ROOT)
        self.assertIn("claude-work", config.PROFILES["profiles"])


class ProfilesAreDerivedBeforeTheHarnessDefault(unittest.TestCase):
    def test_profiles_come_first(self):
        """Review 14: what `[harness] default` resolves against is read off the profiles, so
        they have to exist by the time it is derived."""
        self.assertLess(config.DERIVED.index("PROFILES"), config.DERIVED.index("HARNESS"))


if __name__ == "__main__":
    unittest.main()
