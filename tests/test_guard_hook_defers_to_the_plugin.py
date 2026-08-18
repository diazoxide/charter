"""`init` stops declaring a hook the enabled plugin already declares.

`doctor.check_guard_wired` treats an enabled charter plugin as wired — it says so:
"wired (enabled plugin charter@charter)". `_ensure_guard_hook` never asked, and wrote its
own `PreToolUse` entry into `.claude/settings.json` anyway. Both are then live, and
`charter hook pretooluse` runs twice for every Bash call.

That is the Codex bug on Claude Code. There it was two installers not knowing about each
other; here it is one writer and one checker disagreeing about what "wired" means, in the
same file, in the same process. Nothing is broken either time — everything is doubled,
which is the harder failure to see.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from charter import commands


class _Plane(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-guardhook-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, True))
        self.settings = self.root / ".claude" / "settings.json"
        self.settings.parent.mkdir(parents=True, exist_ok=True)

    def _write(self, doc: dict) -> None:
        self.settings.write_text(json.dumps(doc, indent=2) + "\n")

    def _hooks(self) -> dict:
        return json.loads(self.settings.read_text()).get("hooks", {})


class WhenThePluginIsEnabled(_Plane):
    def test_no_hook_is_declared(self):
        self._write({"enabledPlugins": {"charter@charter": True}})
        status, _ = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "present")
        self.assertEqual(self._hooks(), {},
                         "the plugin declares pretooluse; declaring it here too runs "
                         "charter twice per Bash call")

    def test_a_disabled_plugin_is_not_a_declaration(self):
        """`enabled: false` means the operator turned it off. The hook is then the only
        thing standing between the plane root and an unguarded branch move."""
        self._write({"enabledPlugins": {"charter@charter": False}})
        commands._ensure_guard_hook(self.root)
        self.assertIn("PreToolUse", self._hooks())

    def test_somebody_elses_plugin_does_not_count(self):
        self._write({"enabledPlugins": {"other@marketplace": True}})
        commands._ensure_guard_hook(self.root)
        self.assertIn("PreToolUse", self._hooks())


class WhenThereIsNoPlugin(_Plane):
    def test_the_hook_is_still_written(self):
        self._write({"statusLine": {"type": "command", "command": "charter statusline"}})
        self.assertEqual(commands._ensure_guard_hook(self.root)[0], "created")
        self.assertIn("PreToolUse", self._hooks())

    def test_a_plane_with_no_settings_at_all_gets_the_hook(self):
        self.settings.unlink(missing_ok=True)
        self.assertEqual(commands._ensure_guard_hook(self.root)[0], "created")
        self.assertIn("PreToolUse", self._hooks())

    def test_an_existing_charter_hook_is_not_duplicated(self):
        commands._ensure_guard_hook(self.root)
        self.assertEqual(commands._ensure_guard_hook(self.root)[0], "present")
        self.assertEqual(len(self._hooks()["PreToolUse"]), 1)


if __name__ == "__main__":
    unittest.main()
