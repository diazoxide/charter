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

from unittest import mock

from charter import commands, doctor


class _Plane(unittest.TestCase):
    def setUp(self) -> None:
        # Mocked, never read from the machine: the real one inspects the DEVELOPER's
        # ~/.claude/plugins, so these tests passed or failed by who ran them.
        self.dispatching = self.enterContext(
            mock.patch.object(doctor, "_plugin_declaring_guard", return_value=None))
        self.root = Path(tempfile.mkdtemp(prefix="charter-guardhook-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, True))
        self.settings = self.root / ".claude" / "settings.json"
        self.settings.parent.mkdir(parents=True, exist_ok=True)

    def _write(self, doc: dict) -> None:
        self.settings.write_text(json.dumps(doc, indent=2) + "\n")

    def _hooks(self) -> dict:
        return json.loads(self.settings.read_text()).get("hooks", {})


class WhenAPluginDispatchesTheGuard(_Plane):
    def test_no_hook_is_declared(self):
        self.dispatching.return_value = "charter@charter"
        self._write({})
        status, _ = commands._ensure_guard_hook(self.root)
        self.assertEqual(status, "present")
        self.assertEqual(self._hooks(), {},
                         "the plugin declares pretooluse; declaring it here too runs "
                         "charter twice per Bash call")

    def test_a_plugin_that_dispatches_nothing_is_not_a_declaration(self):
        """Installed, enabled and wired are three states and only the third protects
        anything (#177). 0.43.1 read `enabledPlugins` and would have skipped the hook for
        a plugin from before the guard existed — leaving the plane root unguarded while
        looking configured."""
        self._write({"enabledPlugins": {"charter@charter": True}})   # enabled, wires nothing
        commands._ensure_guard_hook(self.root)
        self.assertIn("PreToolUse", self._hooks())


class WhenNothingDispatchesIt(_Plane):
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
