"""Proof that the suite writes its harness config somewhere disposable.

Written because it already happened: a test called `wire()` without redirecting
`$XDG_CONFIG_HOME`, and charter installed a plugin, a command and a generated context file
into the developer's real `~/.config/opencode/`. Nothing was destroyed — but a fixture
plane's context sat in a live config, and nothing in the suite would ever have said so.
"""

from __future__ import annotations

import os
import tempfile
import unittest
from pathlib import Path

from charter.harness import codex, opencode, registry


class ConfigDirsArePointedSomewhereDisposable(unittest.TestCase):
    def test_opencodes_config_dir_is_not_the_developers(self):
        self.assertNotEqual(opencode.global_dir(), Path.home() / ".config" / "opencode")
        self.assertTrue(str(opencode.global_dir()).startswith(tempfile.gettempdir()),
                        opencode.global_dir())

    def test_codexs_config_is_not_the_developers(self):
        self.assertNotEqual(codex.config_path(), Path.home() / ".codex" / "config.toml")

    def test_wiring_opencode_writes_only_inside_the_sandbox(self):
        """The exact call that leaked. It must be safe to make anywhere in the suite."""
        registry.get("opencode").wire(Path(tempfile.mkdtemp()))
        written = opencode.global_dir() / opencode.SHIM_PATH
        self.assertTrue(written.is_file())
        self.assertNotIn(str(Path.home() / ".config"), str(written))

    def test_the_redirect_survives_a_developer_who_sets_it_themselves(self):
        """Inheriting a real `$XDG_CONFIG_HOME` would keep the hole open for exactly the
        people who customise their machine."""
        self.assertTrue(os.environ["XDG_CONFIG_HOME"].startswith(tempfile.gettempdir()))


if __name__ == "__main__":
    unittest.main()
