"""A harness with no SessionStart hook still gets the session's context.

Claude Code and Codex inject it: charter answers their `SessionStart` with
`additionalContext`. opencode has no such hook, so the same text is written to a file and
named in that tree's `instructions`, which opencode reads at startup.

The text comes from ONE function. Two renderings of "who are you and what is this
workspace" would drift, and the one nobody looks at would be the stale one.
"""

from __future__ import annotations

import json
import unittest
from pathlib import Path

from charter import hooks
from charter.harness import opencode, registry
from tests._isolation import PersonaIso


class ContextBlock(PersonaIso):
    def test_it_carries_the_active_personas_role(self):
        self.make_persona("steward", role="Control Plane Steward")
        (self.tmp / ".charter").mkdir(parents=True, exist_ok=True)
        Path(self.tmp / ".charter" / "active-persona").write_text("steward")
        block = hooks.context_block()
        self.assertIn("steward", block)

    def test_it_never_runs_the_version_autosync(self):
        """`sessionstart` conforms the machine to the plane's version lock. Writing a file
        must not: a `charter clone` that silently upgraded the guard binary because it
        regenerated some context would be an action nobody asked for."""
        calls = []
        orig = hooks._autosync_version_lock
        hooks._autosync_version_lock = lambda *a, **k: calls.append(1) or ""
        self.addCleanup(lambda: setattr(hooks, "_autosync_version_lock", orig))
        hooks.context_block()
        self.assertEqual(calls, [])


class WiringWritesIt(PersonaIso):
    """Written once, under opencode's global config dir — not into anybody's repo."""

    def _tree(self) -> Path:
        import os
        from unittest import mock
        home = self.tmp / "xdg"
        home.mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(home)}))
        registry.get("opencode").wire(self.tmp)
        return opencode.global_dir()

    def test_the_tree_gets_a_context_file(self):
        tree = self._tree()
        self.assertTrue((tree / opencode.CONTEXT_PATH).is_file())

    def test_opencode_is_told_to_read_it(self):
        tree = self._tree()
        doc = json.loads((tree / "opencode.json").read_text())
        self.assertIn(str(tree / opencode.CONTEXT_PATH), doc["instructions"])

    def test_the_instruction_is_not_added_twice(self):
        tree = self._tree()
        registry.get('opencode').wire(self.tmp)
        doc = json.loads((tree / "opencode.json").read_text())
        self.assertEqual(doc["instructions"].count(str(tree / opencode.CONTEXT_PATH)), 1)

    def test_the_context_is_REGENERATED_unlike_the_shim(self):
        """The shim is the operator's to edit and charter never repairs it. The context
        file is derived state — a stale one is a lie about which workspace you are in, so
        this is the one generated file charter overwrites on purpose."""
        tree = self._tree()
        (tree / opencode.CONTEXT_PATH).write_text("stale\n")
        registry.get('opencode').wire(self.tmp)
        self.assertNotEqual((tree / opencode.CONTEXT_PATH).read_text(), "stale\n")

    def test_someone_elses_instructions_survive(self):
        tree = self._tree()
        (tree / "opencode.json").write_text(json.dumps({"instructions": ["AGENTS.md"]}))
        registry.get('opencode').wire(self.tmp)
        doc = json.loads((tree / "opencode.json").read_text())
        self.assertIn("AGENTS.md", doc["instructions"])

    def test_a_malformed_config_is_left_alone(self):
        import os
        from unittest import mock
        home = self.tmp / "xdg2"
        (home / "opencode").mkdir(parents=True, exist_ok=True)
        with mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(home)}):
            cfg = opencode.global_dir() / "opencode.json"
            cfg.write_text("{not json")
            registry.get("opencode").wire(self.tmp)
            self.assertEqual(cfg.read_text(), "{not json")



class StatuslineOnDemand(PersonaIso):
    """opencode has no status-bar socket, so the plane state is a command you run.

    `command/<name>.md` under opencode's config dir is the location, and a command body
    may embed shell output — so `/charter` shows the real status line rather than asking a
    model to describe it. Installed once, like the plugin beside it.
    """

    def test_installing_writes_the_charter_command(self):
        import os
        from unittest import mock
        home = self.tmp / "xdg-cmd"
        home.mkdir(parents=True, exist_ok=True)
        self.enterContext(mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(home)}))
        registry.get("opencode").wire(self.tmp)
        cmd = opencode.global_dir() / opencode.COMMAND_PATH
        self.assertTrue(cmd.is_file())
        body = cmd.read_text()
        self.assertIn("charter statusline", body)
        self.assertIn("!`", body, "the command must embed shell output, not describe it")

if __name__ == "__main__":
    unittest.main()
