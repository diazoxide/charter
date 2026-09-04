"""`doctor` reports a guard declared twice, not just one declared nowhere.

`check_guard_wired` was built for the absence case (#168): nothing dispatches the hook and
the row said health. The opposite state arrived by charter's own hand — `_ensure_guard_hook`
wrote a `PreToolUse` entry into `.claude/settings.json` without asking whether the charter
plugin was already enabled in that same file. Both were live, so `charter hook pretooluse`
ran twice for every Bash call.

0.43.1 stopped `init` adding it. That does nothing for the planes that already have it, and
`doctor` calls them wired — the same "looks fine from the outside" state, arrived at from
the other direction. A guard running twice is not broken, which is exactly why nobody finds
it: two denials for one command read as one stubborn denial.

Charter reports and does not delete. The file is the operator's, git-tracked, and holds keys
charter has no business touching — the restraint `_ensure_guard_hook` has kept since it was
written.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import guardseen, config, doctor


class _Plane(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-doubled-"))
        self.addCleanup(lambda: shutil.rmtree(self.root, True))
        (self.root / ".claude").mkdir(parents=True)
        self.settings = self.root / ".claude" / "settings.json"
        self.enterContext(mock.patch.object(config, "ROOT", self.root))
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        # STATE_DIR follows ROOT in `config.derive`, so redirecting ROOT alone leaves it
        # pointing at the developer's real `.charter/` — and `guardseen.mark` below
        # WRITES there (`guardseen.path()` reads `config.STATE_DIR` at call time).
        # Measured: running this module rewrote the real sighting file, which is the one
        # input `doctor.check_guard_wired` trusts about the developer's own plane.
        self.enterContext(mock.patch.object(config, "STATE_DIR", self.root / ".charter"))
        # Asserted rather than assumed: the patch above is only worth anything if the
        # function that does the writing actually resolves through it, and `guardseen`
        # reads `config.STATE_DIR` at call time precisely so that it can.
        self.assertTrue(guardseen.path().is_relative_to(self.root),
                        f"guard sightings would be written to {guardseen.path()}, "
                        f"outside this test's own throwaway plane ({self.root})")
        # No ambient plugin install and no plugin-owned process: each test says which.
        self.enterContext(mock.patch.dict("os.environ", {}, clear=True))
        self.dispatching = self.enterContext(
            mock.patch.object(doctor, "_plugin_declaring_guard", return_value=None))
        self.enterContext(mock.patch.object(doctor, "_settings_files",
                                            return_value=[self.settings]))

    def _write(self, doc: dict) -> None:
        self.settings.write_text(json.dumps(doc, indent=2) + "\n")

    _HOOK = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
        {"type": "command", "command": "charter hook pretooluse", "timeout": 10}]}]}}


class DeclaredTwice(_Plane):
    def test_a_plugin_and_a_settings_hook_is_reported(self):
        self.dispatching.return_value = "charter@charter"
        self._write(dict(self._HOOK))
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("twice", r.detail.lower())

    def test_it_names_the_file_to_edit_and_does_not_edit_it(self):
        self.dispatching.return_value = "charter@charter"
        self._write(dict(self._HOOK))
        before = self.settings.read_text()
        r = doctor.check_guard_wired()
        self.assertIn("settings.json", r.hint)
        self.assertEqual(self.settings.read_text(), before,
                         "doctor reports; it never edits the operator's file")


class DeclaredOnce(_Plane):
    def test_the_plugin_alone_is_a_clean_row(self):
        self.dispatching.return_value = "charter@charter"
        self._write({})
        # #261: enabled is not loaded — a plugin's hooks arrive at session start, so the
        # tick now needs proof the declaration actually fired. This test's own reasoning
        # already rested on that ("the guard demonstrably fired"); it is now checkable.
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("plugin", r.detail)

    def test_a_settings_hook_alone_is_a_clean_row(self):
        self._write(dict(self._HOOK))
        self.assertEqual(doctor.check_guard_wired().status, doctor.OK)

    def test_a_plugin_that_dispatches_nothing_beside_a_hook_is_not_doubled(self):
        """Enabled is not dispatched (#177). A plugin from before the guard existed wires
        nothing, so the hook is the only guard there is — calling that doubled would send
        someone to delete their only protection."""
        self._write(dict(self._HOOK))          # `_plugin_declaring_guard` stays None
        self.assertEqual(doctor.check_guard_wired().status, doctor.OK)

    def test_neither_is_still_the_warning_this_check_was_built_for(self):
        self._write({"statusLine": {"type": "command", "command": "charter statusline"}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("NOT refused", r.detail)


if __name__ == "__main__":
    unittest.main()
