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
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock

from charter import cli, config, instance, profiles
from tests import _envguard, _isolation
from tests._isolation import PersonaIso

#: The checkout these tests were loaded from: a child `python -c` run here imports it.
REPO = Path(__file__).resolve().parents[1]

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

    def _current(self, text: str) -> profiles.ProfileSet:
        # `profiles.current()` reads the file itself (ruling 43): nothing to re-derive.
        (config.ROOT / "charter.local.toml").write_text(text)
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
        self.assertEqual(set(r.profiles), _BUILT_INS)
        for p in r.profiles.values():
            self.assertEqual(p.source, "built-in")
        self.assertEqual(r.refused, ())

    def test_a_built_in_runs_its_kind_with_no_environment(self):
        self.assertEqual(self._read().profiles["claude"],
                         profiles.Profile("claude", "claude", "claude-code", ("claude",), (),
                                          "built-in"))

    def test_a_declared_profile_is_read_with_its_kind_command_and_env(self):
        p = self._local(_WORK).profiles["claude-work"]
        self.assertEqual(p.kind, "claude")
        self.assertEqual(p.harness, "claude-code")
        self.assertEqual(p.command, ("claude",))
        self.assertEqual(p.env, (("CLAUDE_CONFIG_DIR", "~/.claude-work"),))
        self.assertEqual(p.source, "charter.local.toml")

    def test_a_declared_profile_replaces_the_built_in_of_its_name(self):
        """How plain `claude` gets pinned."""
        r = self._local('[harness.claude]\nkind = "claude"\ncommand = ["/opt/claude"]\n')
        self.assertEqual(r.profiles["claude"].command, ("/opt/claude",))
        self.assertEqual(r.profiles["claude"].source, "charter.local.toml")

    def test_env_is_optional(self):
        self.assertEqual(self._local(_OK).profiles["ok"].env, ())

    def test_env_is_kept_sorted_by_name(self):
        """One order for one environment, so two reads of the same file compare equal."""
        r = self._local('[harness.w]\nkind = "claude"\ncommand = ["claude"]\n'
                        'env = { Z_LAST = "z", A_FIRST = "a" }\n')
        self.assertEqual(r.profiles["w"].env, (("A_FIRST", "a"), ("Z_LAST", "z")))

    def test_the_local_default_wins_over_charter_toml(self):
        self._charter_toml('[harness]\ndefault = "codex"\n')
        r = self._local('[harness]\ndefault = "claude-work"\n' + _WORK)
        self.assertEqual(r.default, "claude-work")
        self.assertEqual(r.default_from, "charter.local.toml")
        self.assertIsNone(r.default_refused)

    def test_a_default_naming_no_profile_is_refused_by_value(self):
        r = self._local('[harness]\ndefault = "nope"\n')
        self.assertIsNone(r.default)
        self.assertEqual(r.default_refused, "nope")

    def test_a_default_that_is_not_text_is_refused_by_value(self):
        """`tomllib` hands back lists and tables too. A value of the wrong type is as
        declared, and as not in force, as a misspelt name — `instance.harness_of`'s rule."""
        r = self._local('[harness]\ndefault = ["claude"]\n')
        self.assertIsNone(r.default)
        self.assertIn("claude", r.default_refused)

    def test_no_default_declared_is_not_a_refusal(self):
        r = self._read()
        self.assertIsNone(r.default)
        self.assertIsNone(r.default_from)
        self.assertIsNone(r.default_refused)


class WhatALaunchWillBeHanded(_LocalFile):
    """The command and environment as declared, and the two expansions no shell does."""

    def test_a_leading_tilde_is_expanded_in_the_first_word_and_every_env_value(self):
        r = self._local('[harness.w]\nkind = "claude"\n'
                        'command = ["~/bin/claude", "~/not-expanded"]\n'
                        'env = { CLAUDE_CONFIG_DIR = "~/.claude-work", B = "~/b" }\n')
        p = r.profiles["w"]
        with mock.patch.dict(os.environ, {"HOME": "/home/op"}):
            self.assertEqual(profiles.expanded_command(p),
                             ["/home/op/bin/claude", "~/not-expanded"])
            self.assertEqual(profiles.expanded_env(p),
                             {"B": "/home/op/b", "CLAUDE_CONFIG_DIR": "/home/op/.claude-work"})
        self.assertEqual(p.command, ("~/bin/claude", "~/not-expanded"))

    def test_display_shows_the_environment_then_the_command(self):
        p = self._local(_WORK).profiles["claude-work"]
        self.assertEqual(profiles.display(p), "CLAUDE_CONFIG_DIR=~/.claude-work claude")

    def test_display_escapes_control_bytes_in_an_env_value_too(self):
        """Ruling 35: a value from a file a chat can write is shown, never interpreted."""
        p = self._local('[harness.w]\nkind = "claude"\ncommand = ["claude"]\n'
                        'env = { CLAUDE_CONFIG_DIR = "~/x\\r\\u001B[2Kfine" }\n').profiles["w"]
        shown = profiles.display(p)
        self.assertNotIn("\r", shown)
        self.assertNotIn("\x1b", shown)


class ABrokenProfileIsRefusedAlone(_LocalFile):
    def _refused(self, body: str, name: str, *, read: dict | None = None) -> str:
        r = read if read is not None else self._local(_OK + body)
        self.assertIn("ok", r.profiles, "a broken profile took a good one down with it")
        self.assertNotIn(name, r.profiles)
        reasons = [x.reason for x in r.refused if x.name == name]
        self.assertEqual(len(reasons), 1, r.refused)
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
        self.assertIn("ok", r.profiles)
        self.assertNotIn("work\n", r.profiles)
        (refusal,) = [x for x in r.refused if "letters, digits" in x.reason]
        self.assertNotIn("\n", refusal.name)

    def test_a_dotted_name_is_refused_by_its_dotted_spelling_in_either_spelling(self):
        """F4. `[harness.claude.alt]` is TOML for a table `alt` inside `claude`, and it is how
        a dotted name gets written without quotes. Both spellings mean one name, so both are
        refused as `claude.alt`, for the dot — and `claude`, which holds only the sub-table,
        declares nothing, so the built-in stays."""
        for spelling in ("[harness.claude.alt]", '[harness."claude.alt"]'):
            with self.subTest(spelling=spelling):
                r = self._local(_OK + f'{spelling}\nkind = "claude"\ncommand = ["claude"]\n')
                self.assertEqual([x.name for x in r.refused], ["claude.alt"])
                self.assertIn("dot", r.refused[0].reason)
                self.assertIn("letters, digits", r.refused[0].reason)
                self.assertEqual(r.profiles["claude"].source, "built-in")
                self.assertIn("ok", r.profiles)

    def test_ruling_41_an_env_table_alone_declares_the_profile_and_refuses_the_name(self):
        """Pin, ruling 41. `[harness.claude.env]` is the TOML spelling of profile `claude`'s
        `env`, not a dotted name: `env` is a profile key. So it declares how `claude` runs,
        and a replacement with no `kind` or `command` refuses the name — the built-in does
        not stand in (ruling 37). Reading `env` as a dotted child instead would leave the
        built-in running the default account."""
        r = self._local(_OK + '[harness.claude.env]\nCLAUDE_CONFIG_DIR = "~/.claude-work"\n')
        self.assertIn("ok", r.profiles)
        self.assertNotIn("claude", r.profiles)
        self.assertEqual([x.name for x in r.refused], ["claude"])
        self.assertIn("claude, opencode, codex", r.refused[0].reason)

    def test_a_declared_profile_holding_a_dotted_child_is_refused_for_it_too(self):
        """F4's other half. A parent with keys of its own is a declaration, and it is read
        with its nested table in place: once parsed, `[harness.work.alt]` is the same thing as
        a typo'd `enviroment = { … }`, and review 13 needs that one to refuse the profile. So
        the child is refused for its dot and the parent for a key charter does not read."""
        r = self._local(_OK + '[harness.work]\nkind = "claude"\ncommand = ["claude"]\n'
                              '[harness.work.alt]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIn("ok", r.profiles)
        self.assertNotIn("work", r.profiles)
        refusals = {x.name: x.reason for x in r.refused}
        self.assertEqual(set(refusals), {"work.alt", "work"})
        self.assertIn("dot", refusals["work.alt"])
        self.assertIn("does not read", refusals["work"])

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

    def test_ruling_37_a_refused_replacement_never_falls_back_to_the_built_in(self):
        """Ruling 37, which extends ruling 19. The operator declared how `claude` runs; if
        that declaration is refused and the built-in stood in, `claude` would run the default
        account — a command the operator replaced, and review 13's `enviroment` typo is
        exactly that. So the name is refused, with its reason, and nothing takes its place."""
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
        self.assertIn("near", read.profiles)

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
                self.assertNotIn("sk-live-123", repr(r.refused))
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
        self.assertEqual(set(r.profiles), _BUILT_INS)
        self.assertEqual(len(r.refused), 1)
        self.assertEqual(r.refused[0].name, "")
        self.assertEqual(r.refused[0].source, "charter.local.toml")

    def test_a_harness_that_is_not_a_table_is_refused_whole(self):
        """F5. The file parsed, so it is not unreadable; a sentence that says it is sends
        the reader to fix TOML that is fine (ADR 0009's rule for an answer's kind)."""
        r = self._local('harness = "claude-work"\n')
        self.assertEqual(set(r.profiles), _BUILT_INS)
        (refusal,) = r.refused
        self.assertIn("not a table", refusal.reason)
        self.assertNotIn("could not be read", refusal.reason)

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
        reasons = [self._local(b).refused[0].reason for b in bodies]
        reasons.append(self._current('[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
                       .refused[0].reason)
        reasons.append(self._current('[harness.x]\nkind = "claude"\ncommand = ["charter"]\n')
                       .refused[0].reason)
        (config.ROOT / "charter.local.toml").unlink()
        self._charter_toml(profile)
        reasons.append(self._read().refused[0].reason)
        self.assertEqual(len(set(reasons)), len(reasons), reasons)


class CharterTomlCarriesNoProfile(_LocalFile):
    def test_a_profile_table_in_charter_toml_is_refused_with_a_pointer(self):
        """A committed command could be changed by a merged PR or by a chat and then run on
        every machine, so the committed file is refused as a home for one."""
        self._charter_toml('[harness.x]\nkind = "claude"\ncommand = ["claude"]\n')
        r = self._read()
        (refusal,) = r.refused
        self.assertEqual(refusal.source, "charter.toml")
        self.assertIn("move the table", refusal.reason.lower())
        self.assertNotIn("x", r.profiles)

    def test_a_committed_table_named_like_a_built_in_leaves_the_built_in(self):
        """charter.toml's tables are not read as profiles at all, so one there has no power
        to take a launcher away from somebody else's machine."""
        self._charter_toml('[harness.claude]\nkind = "claude"\ncommand = ["/opt/claude"]\n')
        r = self._read()
        self.assertEqual(r.profiles["claude"].source, "built-in")
        self.assertEqual([x.source for x in r.refused], ["charter.toml"])

    def test_charter_toml_default_is_still_read(self):
        self._charter_toml('[harness]\ndefault = "codex"\n')
        r = self._read()
        self.assertEqual(r.default, "codex")
        self.assertEqual(r.default_from, "charter.toml")

    def test_other_keys_in_charter_tomls_harness_are_still_ignored(self):
        """Pin. Review 13 dropped this refusal: no `doctor` warning on a plane that did
        nothing wrong."""
        self._charter_toml('[harness]\nnothing = "here"\n')
        self.assertEqual([x for x in self._read().refused if x.source == "charter.toml"], [])


class NamesThatClashWithACommand(_LocalFile):
    def test_a_profile_named_like_a_command_is_refused_by_current(self):
        """The kind clash in `cli._add_frame_parsers` raises and takes every command down,
        which is right for a registry mistake CI sees and wrong for one machine's file."""
        read = self._current(_OK + '[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIn("ok", read.profiles)
        self.assertNotIn("doctor", read.profiles)
        (refusal,) = [x for x in read.refused if x.name == "doctor"]
        self.assertIn("charter doctor", refusal.reason)

    def test_a_default_naming_a_clashing_profile_is_refused(self):
        read = self._current('[harness]\ndefault = "doctor"\n'
                             '[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertIsNone(read.default)
        self.assertEqual(read.default_refused, "doctor")

    def test_the_command_words_hold_the_core_commands_and_not_the_kinds(self):
        self.assertLessEqual({"doctor", "workspace", "frame", "frame-new-chat"},
                             cli.command_words())
        self.assertNotIn("claude", cli.command_words())


class ReadingProfilesIsCheap(_LocalFile):
    def test_reading_profiles_runs_no_subprocess(self):
        """The git check belongs to the ignore check a surface asks for, never to the read
        itself. The file here is one git would commit, which is exactly when a git call in
        the read would be tempting."""
        (config.ROOT / "charter.local.toml").write_text(_WORK)
        with mock.patch("subprocess.run", side_effect=AssertionError("reading profiles ran git")), \
             mock.patch("subprocess.Popen", side_effect=AssertionError("reading profiles spawned")):
            read = profiles.current()
        self.assertIn("claude-work", read.profiles)


class ImportingConfigReadsNoProfile(unittest.TestCase):
    """Ruling 43. `config.derive` runs for every command and every hook process, so a profile
    read there is paid on every tool call — and the deletion sweep charged every line of
    `charter.profiles` to the whole suite, 347 test modules a mutation, which no CI shard could
    finish. So importing config opens no `charter.local.toml` and runs no profiles code, and
    `profiles.current()` reads the file when a surface asks."""

    def setUp(self) -> None:
        # Outside a frame, with no session id: stated rather than inherited from the shell.
        _envguard.unset_all()

    def test_importing_config_opens_no_local_file_and_runs_no_profiles_code(self):
        plane, env = _isolation.child_plane_env(self)
        (plane / "charter.local.toml").write_text(_WORK)
        probe = (
            "import sys\n"
            "opened = []\n"
            "sys.addaudithook(lambda event, args: opened.append(str(args[0])) "
            "if event == 'open' and 'charter.local.toml' in str(args[0]) else None)\n"
            "import charter.config\n"
            "print('charter.profiles' in sys.modules, bool(opened), "
            "hasattr(charter.config, 'PROFILES'))\n")
        done = subprocess.run([sys.executable, "-c", probe], cwd=REPO, env=env,
                              capture_output=True, text=True, timeout=60)
        self.assertEqual(done.stdout.strip(), "False False False", done.stderr)

    def test_config_derives_no_profiles(self):
        """Ruling 43 supersedes review 14's "PROFILES derived before HARNESS"."""
        self.assertNotIn("PROFILES", config.DERIVED)


class BareCharterIsUnchanged(_LocalFile):
    def test_a_local_default_naming_a_declared_profile_leaves_the_harness_default_alone(self):
        """Pin, ruling 43 point 3. Bare `charter` reads `config.HARNESS`, which keeps exactly
        `charter.toml`'s `[harness] default`: a local `default` naming a declared profile must
        not reach it until launching a profile exists — while `harness list` and doctor, which
        ask `profiles.current()`, still see it."""
        for committed in ("", '[harness]\ndefault = "codex"\n'):
            with self.subTest(committed=committed):
                self._charter_toml(committed)
                (config.ROOT / "charter.local.toml").unlink(missing_ok=True)
                config.use(config.ROOT)
                without = dict(config.HARNESS)
                (config.ROOT / "charter.local.toml").write_text(
                    '[harness]\ndefault = "claude-work"\n' + _WORK)
                config.use(config.ROOT)
                self.assertEqual(dict(config.HARNESS), without)
                self.assertEqual(profiles.current().default, "claude-work")


class CurrentIsReadOncePerFile(_LocalFile):
    """`profiles.current()` is memoized per process (ruling 43), and the memo is keyed on what
    the two files hold, so an edit is read again without anybody re-deriving config."""

    def test_the_answer_is_memoized_while_the_files_are_unchanged(self):
        (config.ROOT / "charter.local.toml").write_text(_WORK)
        self.assertIs(profiles.current(), profiles.current())

    def test_an_edited_local_file_is_read_again(self):
        (config.ROOT / "charter.local.toml").write_text(_WORK)
        self.assertIn("claude-work", profiles.current().profiles)
        (config.ROOT / "charter.local.toml").write_text(_OK)
        now = profiles.current()
        self.assertIn("ok", now.profiles)
        self.assertNotIn("claude-work", now.profiles)

    def test_an_edited_charter_toml_is_read_again(self):
        self._charter_toml('[harness]\ndefault = "codex"\n')
        self.assertEqual(profiles.current().default, "codex")
        self._charter_toml('[harness]\ndefault = "opencode"\n')
        self.assertEqual(profiles.current().default, "opencode")

    def test_the_file_is_the_one_other_places_spell_out(self):
        """`commands.LOCAL_PROFILES_IGNORE` and `tests/_planeguard` spell the name rather than
        import this module (ruling 43: importing it would put it on every import path)."""
        self.assertEqual(profiles.LOCAL_FILE, "charter.local.toml")


class TheSentencesSayOnlyWhatIsTrueNow(unittest.TestCase):
    def test_no_refusal_speaks_of_the_selector(self):
        """F2. The profile selector arrives in a later task. A refusal that explains itself by
        a feature the reader cannot find reads as a bug waiting to be filed."""
        for name, value in vars(profiles).items():
            if name.isupper() and isinstance(value, str):
                with self.subTest(constant=name):
                    self.assertNotIn("selector", value.lower())




if __name__ == "__main__":
    unittest.main()
