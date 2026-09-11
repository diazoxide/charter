"""`charter harness list` shows every profile charter read, the file it came from, and why
any was refused.

There is no `charter harness add`: a chat can run a command as easily as it can edit a
file, so a command could never stand for the operator's approval, and all it would buy is
typing (spec, *Where profiles live*). This listing is how an operator who edited the file
reads back what charter made of it. A profile's command comes from a file a chat can
write, so a carriage return or an ESC in one is shown escaped and never interpreted: it
could otherwise redraw a line to show a harmless command (ruling 35).

Everything is on stderr, where `util.info` and `util.warn` already put this command's output
(review 11).
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import commands_harness, config, contain
from tests import _gitguard
from tests._isolation import PersonaIso

_WORK = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.claude-work" }
"""

_OK = """
[harness.ok]
kind = "claude"
command = ["claude"]
"""


class HarnessListShowsProfiles(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # The listing asks git whether the file is ignored. Stated, so a temp directory that
        # sits inside a repository on somebody's machine is not read as belonging to it.
        self.enterContext(mock.patch.dict(
            os.environ, {"GIT_CEILING_DIRECTORIES": str(self.tmp.resolve().parent)}))

    def _list(self, text: str | None = None) -> str:
        if text is not None:
            (config.ROOT / "charter.local.toml").write_text(text)
        config.use(config.ROOT)
        err = io.StringIO()
        with redirect_stderr(err):
            self.assertEqual(commands_harness.cmd_harness_list(SimpleNamespace()), 0)
        return err.getvalue()

    @staticmethod
    def _profiles_block(text: str) -> str:
        # The profiles come first, and a blank line separates them from the kinds' ceilings,
        # whose `codex` and `opencode` rows would otherwise read as a built-in's.
        return text.split("\n\n", 1)[0]

    def _rows(self, text: str) -> dict[str, str]:
        rows = {}
        for line in self._profiles_block(text).splitlines():
            cells = line[2:].split()
            if cells:
                rows.setdefault(cells[0], line)
        return rows

    def _row(self, text: str, name: str) -> str:
        rows = self._rows(text)
        self.assertIn(name, rows, text)
        return rows[name]

    def test_every_built_in_is_listed_as_built_in(self):
        out = self._list()
        for name in ("claude", "codex", "opencode"):
            with self.subTest(name=name):
                self.assertTrue(self._row(out, name).rstrip().endswith("built-in"), out)
        self.assertNotIn("refused:", out)

    def test_built_ins_come_first_in_registry_order_then_declared_by_name(self):
        """The plan's order. A declared replacement keeps its kind's place, and a declared
        name that sorts early still comes after every built-in. Registry order is not
        alphabetical — `opencode` is registered before `codex` — so a listing that sorted
        built-ins by name would put `codex` second and fail here."""
        out = self._list('[harness.zzz-last]\nkind = "claude"\ncommand = ["claude"]\n'
                         '[harness.codex]\nkind = "codex"\ncommand = ["/opt/codex"]\n'
                         '[harness.aaa-first]\nkind = "claude"\ncommand = ["claude"]\n')
        names = [line[2:].split()[0] for line in self._profiles_block(out).splitlines()
                 if line[2:].split() and line[2:].split()[0] != "NAME"
                 and not line.startswith(("refused:", "!", "  refused"))]
        self.assertEqual(names, ["claude", "opencode", "codex", "aaa-first", "zzz-last"], out)

    def test_a_declared_profile_shows_its_command_env_and_file(self):
        row = self._row(self._list(_WORK), "claude-work")
        self.assertIn("CLAUDE_CONFIG_DIR=~/.claude-work claude", row)
        self.assertIn("charter.local.toml", row)

    def test_the_default_is_marked(self):
        out = self._list('[harness]\ndefault = "claude-work"\n' + _WORK)
        self.assertTrue(self._row(out, "claude-work").startswith("*"), out)
        self.assertFalse(self._row(out, "claude").startswith("*"), out)

    def test_a_refused_profile_is_listed_with_its_reason(self):
        out = self._list('[harness.broken]\nkind = "claude"\ncommand = "claude --resume"\n')
        block = self._profiles_block(out)
        self.assertIn("refused:", block)
        (line,) = [ln for ln in block.split("refused:", 1)[1].splitlines() if "broken" in ln]
        self.assertIn("never a shell string", line)
        self.assertNotIn("broken", self._rows(out))

    def test_a_profile_named_like_a_command_is_listed_as_refused(self):
        """The listing reads what every launch surface reads, clashes included."""
        out = self._list('[harness.doctor]\nkind = "claude"\ncommand = ["claude"]\n')
        self.assertNotIn("doctor", self._rows(out))
        self.assertIn("charter doctor", self._profiles_block(out).split("refused:", 1)[1])

    def test_a_committable_files_profile_is_listed_as_refused_not_as_a_row(self):
        """F1. `NOT_IGNORED` says the profiles in the file are refused, so the listing shows
        exactly that — not an ordinary row with a warning under it."""
        subprocess.run(["git", "-C", str(config.ROOT), "init", "-q"], check=True,
                       capture_output=True, env={**os.environ, **_gitguard.environment()})
        out = self._list(_WORK)
        self.assertNotIn("claude-work", self._rows(out))
        block = self._profiles_block(out)
        (line,) = [ln for ln in block.split("refused:", 1)[1].splitlines()
                   if ln.strip().startswith("claude-work:")]
        self.assertIn("git would commit", line)
        self.assertIn("charter reinit", block)

    def test_ruling_37_a_refused_replacement_is_listed_refused_and_its_built_in_is_not(self):
        """Pin. Ruling 37 in the listing: the name shows refused, with its reason, and no
        built-in row stands in for it."""
        out = self._list('[harness.claude]\nkind = "claude"\ncommand = ["claude"]\n'
                         'enviroment = { CLAUDE_CONFIG_DIR = "~/.claude-work" }\n')
        self.assertNotIn("claude", self._rows(out))
        (line,) = [ln for ln in self._profiles_block(out).split("refused:", 1)[1].splitlines()
                   if ln.strip().startswith("claude:")]
        self.assertIn("does not read", line)

    def test_a_committable_file_is_said(self):
        subprocess.run(["git", "-C", str(config.ROOT), "init", "-q"], check=True,
                       capture_output=True, env={**os.environ, **_gitguard.environment()})
        self.assertIn("charter reinit adds", self._list(_OK))

    def test_an_ignored_file_says_nothing_about_git(self):
        subprocess.run(["git", "-C", str(config.ROOT), "init", "-q"], check=True,
                       capture_output=True, env={**os.environ, **_gitguard.environment()})
        (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
        out = self._list(_OK)
        block = self._profiles_block(out)
        self.assertNotIn("git", block)
        self.assertEqual([ln for ln in block.splitlines() if ln.startswith("!")], [], out)

    def test_a_command_with_control_bytes_is_listed_escaped(self):
        out = self._list(r'''
[harness.sneaky]
kind = "claude"
command = ["claude\r\u001B[2Kharmless"]
''')
        self.assertNotIn("\r", out)
        self.assertNotIn("\x1b", out)
        self.assertIn(contain.readable("claude\r\x1b[2Kharmless"), out)

    def test_a_name_that_skipped_validation_is_still_listed_escaped(self):
        """Ruling 35 at the NAME cell itself. A declared name passes `NAME_RE` before it can
        reach a row, so today no name carries a control byte — but the listing does not lean
        on every future source of names validating them first. The rendering layer alone
        would turn a carriage return into a space, which is a different name; the escaped
        spelling is what says which one this is."""
        from charter import profiles
        odd = profiles.Profile("a\rb", "claude", "claude-code", ("claude",), (),
                               "charter.local.toml")
        unvalidated = profiles.ProfileSet({"a\rb": odd}, (), None, None, None)
        with mock.patch.object(profiles, "current", return_value=unvalidated):
            out = self._list()
        self.assertNotIn("\r", out)
        self.assertIn(contain.readable("a\rb"), out)

    def test_the_kinds_ceilings_are_still_listed(self):
        out = self._list()
        self.assertIn("↳", out.split("\n\n", 1)[1])


if __name__ == "__main__":
    unittest.main()
