"""The one installed plugin has to be kept current, or the install is a one-shot.

#233 fixed this for per-tree shims: a plugin an older charter wrote survived every upgrade
while `doctor` reported it wired. #241 moved the plugin to one global location and deleted
the per-tree machinery — and took the refresh path with it, leaving `refresh_shim`
orphaned and `doctor` printing a clean harness row over a 0.42.1 plugin under 0.43.0.

Same bug, one level up, reintroduced by the change that simplified everything else. The
mechanism that keeps a generated file honest has to move with the file.
"""

from __future__ import annotations

import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import __version__, doctor
from charter.harness import opencode, registry


class InstallingKeepsItCurrent(unittest.TestCase):
    def setUp(self) -> None:
        self.home = Path(tempfile.mkdtemp(prefix="charter-gsr-"))
        self.addCleanup(lambda: shutil.rmtree(self.home, True))
        self.enterContext(mock.patch.dict(os.environ, {"XDG_CONFIG_HOME": str(self.home)}))
        self.shim = opencode.global_dir() / opencode.SHIM_PATH

    def _plant(self, body: str) -> None:
        self.shim.parent.mkdir(parents=True, exist_ok=True)
        self.shim.write_text(body)

    def test_a_plugin_an_older_charter_wrote_is_replaced(self):
        opencode.ensure_shim(opencode.global_dir())
        self._plant(self.shim.read_text().replace(__version__, "0.40.0"))
        registry.get("opencode").wire(Path("/unused"))
        self.assertEqual(opencode.shim_version(opencode.global_dir()), __version__)

    def test_a_plugin_somebody_edited_is_left_alone(self):
        """Charter cannot tell a pre-stamp plugin from a rewritten one, and guessing wrong
        destroys work. It reports instead — which is what `doctor` is for."""
        self._plant("// mine\n")
        registry.get("opencode").wire(Path("/unused"))
        self.assertEqual(self.shim.read_text(), "// mine\n")

    def test_an_absent_plugin_is_written(self):
        registry.get("opencode").wire(Path("/unused"))
        self.assertEqual(opencode.shim_version(opencode.global_dir()), __version__)


class DoctorNamesAStalePlugin(unittest.TestCase):
    def setUp(self) -> None:
        self.home = Path(tempfile.mkdtemp(prefix="charter-gsr-d-"))
        self.addCleanup(lambda: shutil.rmtree(self.home, True))
        self.enterContext(mock.patch.dict(os.environ,
                                          {"XDG_CONFIG_HOME": str(self.home),
                                           "CHARTER_HARNESS": "opencode"}))

    def test_a_stale_plugin_is_not_a_clean_row(self):
        opencode.ensure_shim(opencode.global_dir())
        p = opencode.global_dir() / opencode.SHIM_PATH
        p.write_text(p.read_text().replace(__version__, "0.40.0"))
        r = doctor.check_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("0.40.0", r.detail)
        self.assertIn("reinit", r.hint)

    def test_an_edited_plugin_says_so_and_says_what_to_do(self):
        p = opencode.global_dir() / opencode.SHIM_PATH
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("// mine\n")
        r = doctor.check_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertTrue(r.hint)

    def test_a_current_plugin_is_a_clean_row(self):
        registry.get("opencode").wire(Path("/unused"))
        self.assertEqual(doctor.check_harness().status, doctor.OK)

    def test_no_plugin_at_all_is_not_reported_as_stale(self):
        """An uninstalled harness is a different sentence, and `harness` already says the
        plane is not wired — two rows saying the same thing teaches people to skim."""
        self.assertEqual(doctor.check_harness().status, doctor.OK)


if __name__ == "__main__":
    unittest.main()
