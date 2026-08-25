"""`init` wires BOTH harnesses, because privileging one is the lock-in this removes.

ADR 0015: opencode has no marketplace and no published charter package, so `init` writing
`.opencode/plugin/charter.ts` **is** the install there. Claude Code's half is one static
variable — settings.json's `env` "sets environment variables that apply to every session"
— so `harness.current()` has something to read on both.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config
from charter.harness import opencode
from tests import _envguard


class InitWiresBothHarnesses(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

        self.root = Path(tempfile.mkdtemp(prefix="charter-init-h-")).resolve()
        self.addCleanup(lambda: __import__("shutil").rmtree(self.root, True))
        import os
        home = self.root / "xdg"
        home.mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(home)}))
        args = SimpleNamespace(forge="github", owner="acme", host=None)
        with mock.patch.object(config, "ROOT", self.root):
            commands.cmd_init(args)

    def test_the_opencode_plugin_is_installed_not_dropped_in_the_plane(self):
        """`init` installs opencode's plugin where opencode reads it for every project.
        Writing it into the plane was the old shape, and it put a generated file in
        somebody's repo for no gain — opencode never read it from there anyway unless the
        session happened to start in that exact directory."""
        self.assertTrue((opencode.global_dir() / opencode.SHIM_PATH).is_file())
        self.assertFalse((self.root / ".opencode").exists())

    def test_claude_code_is_told_its_own_name(self):
        settings = json.loads((self.root / ".claude" / "settings.json").read_text())
        self.assertEqual(settings.get("env", {}).get("CHARTER_HARNESS"), "claude-code")

    def test_an_operators_own_env_block_is_not_disturbed(self):
        """Same restraint as the status line: charter adds its key, never rewrites a
        block someone else is using."""
        p = self.root / ".claude" / "settings.json"
        settings = json.loads(p.read_text())
        settings["env"]["OTEL_METRICS_EXPORTER"] = "otlp"
        settings["env"]["CHARTER_HARNESS"] = "hand-edited"
        p.write_text(json.dumps(settings))
        with mock.patch.object(config, "ROOT", self.root):
            commands.cmd_reinit(SimpleNamespace())
        again = json.loads(p.read_text())
        self.assertEqual(again["env"]["OTEL_METRICS_EXPORTER"], "otlp")
        self.assertEqual(again["env"]["CHARTER_HARNESS"], "hand-edited")


if __name__ == "__main__":
    unittest.main()
