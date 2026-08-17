"""The opencode plugin charter generates, and the restraint it is written with.

ADR 0015: there is no marketplace and no published package for opencode — `charter init`
writing this file IS the install. That makes it the one artifact charter ships and never
executes, so what it contains is asserted here rather than trusted.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from charter.harness import opencode


class EnsureShim(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-oc-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.root, True))
        self.shim = self.root / ".opencode" / "plugin" / "charter.ts"

    def test_a_plane_with_no_opencode_dir_gets_the_plugin_written(self):
        self.assertEqual(opencode.ensure_shim(self.root), "created")
        self.assertTrue(self.shim.is_file())

    def test_a_second_call_leaves_an_edited_plugin_alone(self):
        """IF ABSENT, never repair — the same restraint `_load_settings` keeps for
        `.claude/settings.json`. Someone who edited the shim gets to keep their edit; a
        writer that silently reverts a deliberate change is the thing this refuses."""
        opencode.ensure_shim(self.root)
        self.shim.write_text("// mine now\n")
        self.assertEqual(opencode.ensure_shim(self.root), "present")
        self.assertEqual(self.shim.read_text(), "// mine now\n")

    def test_the_plugin_names_the_harness_through_shell_env(self):
        """The shim's whole job in this piece: every shell opencode spawns answers
        `harness.current()` with "opencode"."""
        opencode.ensure_shim(self.root)
        src = self.shim.read_text()
        self.assertIn('"shell.env"', src)
        self.assertIn("CHARTER_HARNESS", src)
        self.assertIn("opencode", src)

    def test_the_plugin_passes_the_session_id_through_per_invocation(self):
        """opencode 1.18.18 hands `shell.env` ``{cwd, sessionID, callID}`` — checked
        against the binary, because the published example shows `cwd` alone. Read from
        `input` on every call rather than cached in a module variable: one opencode server
        hosts many sessions, so a module-level "current session" has no correct value."""
        opencode.ensure_shim(self.root)
        src = self.shim.read_text()
        self.assertIn("CHARTER_SESSION_ID", src)
        self.assertIn("input.sessionID", src)


if __name__ == "__main__":
    unittest.main()
