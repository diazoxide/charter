"""`charter harness install codex` — the opt-in that reaches outside the plane.

`init` will not do this. Codex keeps hooks only in `~/.codex/config.toml`, so arming them
touches every repo on the machine; running the command IS the consent, the same two-step
shape ADR 0003 uses for `charter report` and `init` uses for its first clone.
"""

from __future__ import annotations

import os
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from charter.harness import codex


class CodexHome(unittest.TestCase):
    def test_codex_home_is_honoured(self):
        """Verified against the binary: `CODEX_HOME=<dir> codex mcp list` reads that dir's
        config.toml. Writing to `~/.codex` regardless would miss anyone who sets it."""
        with mock.patch.dict(os.environ, {"CODEX_HOME": "/somewhere"}, clear=True):
            self.assertEqual(codex.config_path(), Path("/somewhere/config.toml"))

    def test_it_falls_back_to_the_default_home(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(codex.config_path(), Path.home() / ".codex" / "config.toml")


class Install(unittest.TestCase):
    def setUp(self) -> None:
        self.home = Path(tempfile.mkdtemp(prefix="charter-codexhome-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.home, True))
        self.env = mock.patch.dict(os.environ, {"CODEX_HOME": str(self.home)}, clear=False)
        self.env.start()
        self.addCleanup(self.env.stop)
        self.cfg = self.home / "config.toml"

    def test_a_machine_with_no_config_gets_one(self):
        self.assertEqual(codex.install()[0], "created")
        self.assertTrue(self.cfg.is_file())

    def test_what_it_writes_is_valid_toml_in_the_shape_the_parser_accepts(self):
        codex.install()
        doc = tomllib.loads(self.cfg.read_text())
        self.assertEqual(doc["shell_environment_policy"]["set"]["CHARTER_HARNESS"], "codex")
        entry = doc["hooks"]["PreToolUse"][0]["hooks"][0]
        self.assertEqual(entry["type"], "command")
        self.assertIn("charter hook pretooluse", entry["command"])

    def test_a_second_install_changes_nothing(self):
        codex.install()
        before = self.cfg.read_text()
        self.assertEqual(codex.install()[0], "present")
        self.assertEqual(self.cfg.read_text(), before)

    def test_an_operators_own_hooks_are_never_merged_into(self):
        """Charter appends whole tables or nothing. Merging into someone's existing
        `[hooks]` means rewriting TOML charter did not author — and `_load_settings`
        already refuses that for the file charter half-owns, let alone this one."""
        self.cfg.write_text('[hooks]\n[[hooks.SessionStart]]\nmatcher = "mine"\n')
        before = self.cfg.read_text()
        status, detail = codex.install()
        self.assertEqual(status, "present")
        self.assertEqual(self.cfg.read_text(), before)
        self.assertIn("hooks", detail)

    def test_an_unparseable_config_is_left_completely_alone(self):
        self.cfg.write_text("this is not = = toml\n")
        before = self.cfg.read_text()
        self.assertEqual(codex.install()[0], "malformed")
        self.assertEqual(self.cfg.read_text(), before)


if __name__ == "__main__":
    unittest.main()
