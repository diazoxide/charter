"""`charter harness install codex` stops installing hooks Codex already has.

Codex consumes charter's plugin — the same artifact Claude Code installs, from the same
marketplace. That was never established when this command was written: the survey stopped
at `config.toml`, found hooks there, and built a second installer for them.

The result was both live at once on a real machine — 3 hook declarations in `config.toml`
and 12 from the plugin — so charter ran twice on every SessionStart, UserPromptSubmit and
Bash call. Nothing was wrong; everything was doubled, which is worse than either.

What remains for the command to do is the one thing the plugin cannot: tell Codex sessions
which harness they are, via `shell_environment_policy.set`.
"""

from __future__ import annotations

import os
import shutil
import tempfile
import tomllib
import unittest
from pathlib import Path
from unittest import mock

from charter.harness import codex


class Install(unittest.TestCase):
    def setUp(self) -> None:
        self.home = Path(tempfile.mkdtemp(prefix="charter-codexhome-"))
        self.addCleanup(lambda: shutil.rmtree(self.home, True))
        self.enterContext(mock.patch.dict(os.environ, {"CODEX_HOME": str(self.home)}))
        self.cfg = self.home / "config.toml"

    def test_it_names_the_harness_and_declares_no_hooks(self):
        status, _detail = codex.install()
        self.assertEqual(status, "created")
        doc = tomllib.loads(self.cfg.read_text())
        self.assertEqual(doc["shell_environment_policy"]["set"]["CHARTER_HARNESS"], "codex")
        self.assertNotIn("hooks", doc,
                         "the plugin declares the hooks; declaring them again runs "
                         "charter twice per turn")

    def test_it_reports_a_legacy_block_rather_than_leaving_it_doubled(self):
        """An earlier charter wrote hooks here. Upgrading must not leave them running
        alongside the plugin's, and must not silently delete somebody's config either."""
        self.cfg.write_text(
            '[[hooks.PreToolUse]]\nmatcher = "Bash"\n'
            '[[hooks.PreToolUse.hooks]]\ntype = "command"\n'
            'command = "charter hook pretooluse"\n')
        status, detail = codex.install()
        self.assertEqual(status, "doubled")
        self.assertIn("hooks", detail)
        self.assertIn("charter hook", self.cfg.read_text(),
                      "charter must not delete the operator's config to fix this")

    def test_a_second_install_is_a_no_op(self):
        codex.install()
        before = self.cfg.read_text()
        self.assertEqual(codex.install()[0], "present")
        self.assertEqual(self.cfg.read_text(), before)


class TheCeilingThatWasNotReal(unittest.TestCase):
    def test_wiring_scope_is_no_longer_claimed(self):
        """`wiring-scope` said Codex's wiring is machine-wide and cannot be shared with a
        team. That described the config.toml route charter chose, not Codex: the plugin is
        versioned and installed like any other. A deficit that describes charter's own
        wrong turn teaches the reader something false."""
        from charter.harness import registry

        keys = {d.key for d in registry.get("codex").deficits}
        self.assertNotIn("wiring-scope", keys)


if __name__ == "__main__":
    unittest.main()
