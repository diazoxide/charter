"""Harnesses are registered, the way forges are.

`charter/forge/registry.py` records why: iterating `KINDS` means "a new forge kind is
covered automatically the day it's registered — never a hardcoded literal". Harnesses have
the same multiplicity and the same problem, so they get the same shape. The test that
matters is the last one: adding Codex must be adding a class, not editing `cmd_init`.
"""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config
from charter.harness import base, registry


class Registry(unittest.TestCase):
    def test_both_harnesses_are_registered_under_their_own_name(self):
        self.assertEqual({h.name for h in registry.all()}, {"claude-code", "opencode"})

    def test_a_harness_is_looked_up_by_the_name_it_puts_in_the_environment(self):
        self.assertEqual(registry.get("opencode").name, "opencode")
        self.assertIsNone(registry.get("codex"))

    def test_an_unknown_harness_still_resolves(self):
        """The harness is the authority on its own identity. A `$CHARTER_HARNESS` charter
        has never heard of is reported verbatim rather than swallowed — an unrecognised
        harness is information, and discarding it would render as "no harness"."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}, clear=True):
            self.assertEqual(registry.current(), "codex")

    def test_native_evidence_names_the_harness_when_the_variable_is_absent(self):
        with mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": "/p"}, clear=True):
            self.assertEqual(registry.current(), "claude-code")


class InitIsHarnessAgnostic(unittest.TestCase):
    def test_a_newly_registered_harness_is_wired_without_touching_init(self):
        """The Codex test. `cmd_init` must not name a harness; it must ask the registry."""

        class FakeHarness(base.Harness):
            name = "fake"

            def wire(self, root):
                (Path(root) / ".fake-wiring").write_text("wired\n")
                return [("created", ".fake-wiring")]

        root = Path(tempfile.mkdtemp(prefix="charter-fake-h-")).resolve()
        self.addCleanup(lambda: __import__("shutil").rmtree(root, True))
        with mock.patch.dict(registry.KINDS, {"fake": FakeHarness}), \
                mock.patch.object(config, "ROOT", root):
            commands.cmd_init(SimpleNamespace(forge="github", owner="acme", host=None))
        self.assertTrue((root / ".fake-wiring").is_file(),
                        "cmd_init wired only the harnesses it names by hand")


if __name__ == "__main__":
    unittest.main()
