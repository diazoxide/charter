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

from charter import cli, config, contain, instance, profiles, util
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

    def test_a_harness_registered_without_a_cli_name_is_no_kind_and_no_profile(self):
        """`Harness.cli_name` defaults to "", and a harness with no word after `charter` is not
        launchable as one. Without the filter in `_kinds` it would be a kind named "" and a
        built-in profile nobody can type. Every harness registered today has a word, so only
        a registry with one that does not can show the filter doing anything."""
        from types import SimpleNamespace

        from charter.harness import registry

        headless = SimpleNamespace(name="headless", cli_name="")
        with mock.patch.object(registry, "all", return_value=[*registry.all(), headless]):
            self.assertNotIn("", profiles._kinds())
            self.assertEqual(set(profiles.builtins()), _BUILT_INS)

    def test_a_cfg_that_is_not_a_table_declares_nothing_and_raises_nothing(self):
        """`derive`'s docstring promises it never raises, and `current` hands it whatever
        `instance.load` returned. A value that is not a table declares nothing: the built-ins,
        and no refusal. The sweep of `ce7f5d8` collapsed the `isinstance(cfg, dict)` guard to
        `cfg.get("harness")` with the suite still green."""
        for cfg in (None, [], "schema = 1"):
            with self.subTest(cfg=cfg):
                r = profiles.derive(config.ROOT, cfg)
                self.assertEqual(set(r.profiles), _BUILT_INS)
                self.assertEqual(r.refused, ())

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

    def test_an_inline_table_under_a_typo_names_both_readings(self):
        """Once parsed, `enviroment = { … }` under `[harness.claude]` is the same thing as a table
        `[harness.claude.enviroment]`, so the refusal names both readings: it is not a key charter
        reads, and a profile named `claude.enviroment` could not carry a dot. The profile itself
        stays refused, for the key it does not read."""
        r = self._local(_OK + '[harness.claude]\nkind = "claude"\ncommand = ["claude"]\n'
                              'enviroment = { CLAUDE_CONFIG_DIR = "~/.claude-work" }\n')
        refusals = {x.name: x.reason for x in r.refused}
        self.assertEqual(set(refusals), {"claude.enviroment", "claude"})
        self.assertIn("not a key", refusals["claude.enviroment"])
        self.assertIn("dot", refusals["claude.enviroment"])
        self.assertIn("does not read", refusals["claude"])
        self.assertNotIn("claude", r.profiles)

    def test_a_table_under_a_profile_key_is_that_keys_value_not_a_dotted_name(self):
        """`kind`, `command` and `env` are a profile's own keys, so a table under one of them is a
        wrong value for that key and refused as one — never read as a profile named `k.kind`."""
        r = self._local(_OK + '[harness.k]\ncommand = ["claude"]\n[harness.k.kind]\nx = 1\n')
        self.assertEqual([x.name for x in r.refused], ["k"])
        self.assertIn("claude, opencode, codex", r.refused[0].reason)

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

    def test_a_kind_registered_without_a_login_sentence_still_gets_one(self):
        """The pin above holds for every kind today. This is what a kind registered without a
        login sentence costs: a general sentence in the refusal, not the row — doctor reads
        through here, and doctor is what the operator runs when something is wrong."""
        with mock.patch.dict(profiles.LOGIN, clear=True):
            reason = self._refused('[harness.s]\nkind = "claude"\ncommand = ["claude"]\n'
                                   'env = { API_KEY = "v" }\n', "s")
        self.assertIn("log in inside that harness", reason)

    def test_a_refusal_names_the_first_offending_variable_by_name(self):
        """By name, not by where it was written: reordering a table changes no sentence. The
        sweep of `ce7f5d8` swapped the `sorted` for `list` with the suite still green."""
        for env, first, later in (('{ CHARTER_Z = "1", CHARTER_A = "2" }', "CHARTER_A", "CHARTER_Z"),
                                  ('{ Z_TOKEN = "1", A_KEY = "2" }', "A_KEY", "Z_TOKEN")):
            with self.subTest(env=env):
                reason = self._refused('[harness.v]\nkind = "claude"\ncommand = ["claude"]\n'
                                       f'env = {env}\n', "v")
                self.assertIn(first, reason)
                self.assertNotIn(later, reason)

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

    def test_every_value_a_refusal_repeats_back_is_escaped(self):
        """Ruling 35: a name, kind, key or variable from a file a chat can write is shown,
        never interpreted. `harness list` prints a refusal as `derive` returned it, so each
        sentence has to carry the escaped spelling itself. Every case puts a carriage return
        into the one value its sentence repeats. No test held such a value until the sweep of
        `ce7f5d8` deleted three of these `contain.readable` calls with the suite still green:
        the name in `_profile_refusal`, the variable `SECRET_ENV` names, and the dotted name
        `NESTED_TABLE` gives."""
        cr, esc = chr(13), contain.readable
        tail = 'kind = "claude"\ncommand = ["claude"]\n'
        cases = [
            # (the sentence, the local file, the refused entry's name, the value repeated)
            ("ILLEGAL_NAME", _OK + '[harness."we\\rird"]\n' + tail,
             esc(f"we{cr}ird"), f"we{cr}ird"),
            ("NOT_A_TABLE", '[harness]\n"we\\rird" = "claude"\n' + _OK,
             esc(f"we{cr}ird"), f"we{cr}ird"),
            ("UNKNOWN_KIND", _OK + '[harness.k]\nkind = "cl\\raude"\ncommand = ["claude"]\n',
             "k", f"cl{cr}aude"),
            ("CHARTER_ENV", _OK + '[harness.c]\n' + tail + 'env = { "CHARTER_\\rX" = "v" }\n',
             "c", f"CHARTER_{cr}X"),
            ("SECRET_ENV", _OK + '[harness.s]\n' + tail + 'env = { "API_\\rKEY" = "v" }\n',
             "s", f"API_{cr}KEY"),
            ("UNKNOWN_PROFILE_KEY", _OK + '[harness.u]\n' + tail + '"envi\\rronment" = "x"\n',
             "u", f"envi{cr}ronment"),
            ("NESTED_TABLE", _OK + '[harness.n."a\\rb"]\n' + tail,
             esc(f"n.a{cr}b"), f"a{cr}b"),
            ("LOCAL_SECTION", _OK + '["for\\rge"]\nhost = "x"\n',
             esc(f"for{cr}ge"), f"for{cr}ge"),
        ]
        for sentence, text, name, value in cases:
            with self.subTest(sentence=sentence):
                reason = self._refused("", name, read=self._local(text))
                self.assertIn(esc(value), reason)
                self.assertNotIn(cr, reason)
        with self.subTest(sentence="a refused default"):
            r = self._local('[harness]\ndefault = "no\\rpe"\n' + _OK)
            self.assertEqual(r.default_refused, esc(f"no{cr}pe"))
        with self.subTest(sentence="PROFILE_IN_COMMITTED"):
            self._charter_toml('[harness."we\\rird"]\n' + tail)
            reason = self._refused("", esc(f"we{cr}ird"), read=self._local(_OK))
            self.assertIn(esc(f"we{cr}ird"), reason)
            self.assertNotIn(cr, reason)

    def test_a_header_with_nothing_under_it_is_refused_not_skipped(self):
        """`[harness.work]` alone declares a profile with no kind and no command, and a broken
        profile is refused by name like any other. Skipped, it would be neither listed nor
        refused, and `[harness.claude]` alone would leave the built-in running where the
        operator declared a replacement (ruling 37). Only a parent holding nothing but
        sub-tables declares nothing (F4) — the `nested and` in `derive` is what keeps an empty
        table from counting as one, and the sweep of `ce7f5d8` found no test for it."""
        for name in ("work", "claude"):
            with self.subTest(name=name):
                reason = self._refused(f"[harness.{name}]\n", name)
                self.assertIn('has kind ""', reason)


class EveryLayerEscapesWhatItRepeats(_LocalFile):
    """Ruling 35 at each layer that repeats a value, not only where the value is first read.

    A name reaching these layers today has passed `NAME_RE` or is a built-in's word, so it
    carries no control byte — but a layer that leaned on that would print whatever a future
    source of names hands it. Survivor [12] asked this of `harness list`'s NAME cell; the
    sweep of `ce7f5d8` asked it of the three layers below. Git's own words come from outside
    charter whatever validates the rest."""

    _ODD = "a" + chr(13) + "b"

    def _unvalidated(self, command=("claude",)) -> profiles.ProfileSet:
        odd = profiles.Profile(self._ODD, "claude", "claude-code", command, (),
                               "charter.local.toml")
        return profiles.ProfileSet({self._ODD: odd}, (), self._ODD, "charter.local.toml", None)

    def test_the_ignore_check_refuses_an_unvalidated_name_escaped(self):
        """`with_ignore_check` names each profile it moves, and `_narrowed` names the default
        it had to drop."""
        r = profiles.with_ignore_check(self._unvalidated(),
                                       profiles.IgnoreCheck("git would carry it", "fix"))
        self.assertEqual([x.name for x in r.refused], [contain.readable(self._ODD)])
        self.assertIsNone(r.default)
        self.assertEqual(r.default_refused, contain.readable(self._ODD))

    def test_current_refuses_an_unvalidated_name_escaped(self):
        """`current()` names a profile it refuses for a command that is charter itself."""
        profiles._last.clear()
        self.addCleanup(profiles._last.clear)
        with mock.patch.object(profiles, "derive", return_value=self._unvalidated(("charter",))):
            r = profiles.current()
        [refused] = [x for x in r.refused if x.source == "charter.local.toml"]
        self.assertEqual(refused.name, contain.readable(self._ODD))
        self.assertIn(contain.readable(self._ODD), refused.reason)
        self.assertNotIn(chr(13), refused.reason)

    def test_gits_own_words_are_escaped_in_the_refusal_and_the_fix(self):
        (config.ROOT / "charter.local.toml").write_text(_OK)
        said = "fatal: " + self._ODD
        with mock.patch.object(util, "git_path_state", return_value=(util.UNKNOWN_GIT, said)):
            check = profiles.ignore_check(config.ROOT)
        for text in check:
            self.assertIn(contain.readable(said), text)
            self.assertNotIn(chr(13), text)

    def test_the_parsers_own_words_are_escaped(self):
        """Why the file could not be read is `tomllib`'s or the operating system's text, and
        `LOCAL_UNREADABLE` repeats it."""
        said = "bad " + self._ODD
        with mock.patch.object(profiles, "_read_local", return_value=({}, said)):
            r = self._read()
        [refused] = [x for x in r.refused if x.source == "charter.local.toml"]
        self.assertIn(contain.readable(said), refused.reason)
        self.assertNotIn(chr(13), refused.reason)


class TheIgnoreCheckMovesOnlyWhatTheLocalFileDeclares(_LocalFile):
    def test_a_built_in_stays_a_profile_and_is_not_also_refused(self):
        """F1 refuses what `charter.local.toml` declares, because that file is the one git
        would carry. A built-in comes from the registry, so it stays a profile — and appears
        once: refused as well, it would read as broken while it still launches. The sweep of
        `ce7f5d8` dropped the `p.source == LOCAL_FILE` filter with the suite still green."""
        r = profiles.with_ignore_check(self._local(_OK),
                                       profiles.IgnoreCheck("git would carry it", "fix"))
        self.assertEqual(set(r.profiles), _BUILT_INS)
        self.assertEqual([x.name for x in r.refused], ["ok"])


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

    def test_an_unreadable_file_and_an_absent_one_are_different_answers(self):
        """A file that cannot be read and no file at all must not share a memo key, or the
        second answer is the first's. A directory stands in for the unreadable file."""
        local = config.ROOT / "charter.local.toml"
        local.mkdir()
        self.assertEqual([x.name for x in profiles.current().refused], [""])
        local.rmdir()
        self.assertEqual(profiles.current().refused, ())

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
