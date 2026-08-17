"""`charter harness` — see which harnesses charter knows, and arm the opt-in one."""

from __future__ import annotations

import io
import os
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands_harness


def _run(fn, args) -> tuple[int, str]:
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(args)
    return rc, out.getvalue() + err.getvalue()


class HarnessList(unittest.TestCase):
    def test_it_names_every_registered_harness_and_its_ceilings(self):
        rc, text = _run(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertEqual(rc, 0)
        for name in ("claude-code", "opencode", "codex"):
            self.assertIn(name, text)
        self.assertIn("status-bar", text)

    def test_it_marks_the_harness_this_session_is_in(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            _rc, text = _run(commands_harness.cmd_harness_list, SimpleNamespace())
        self.assertRegex(text, r"[*>•→].{0,4}opencode")


class HarnessInstall(unittest.TestCase):
    def setUp(self) -> None:
        self.home = Path(tempfile.mkdtemp(prefix="charter-codexhome-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.home, True))
        self.enterContext(mock.patch.dict(os.environ, {"CODEX_HOME": str(self.home)}))

    def test_installing_codex_writes_its_config_and_says_it_is_untrusted(self):
        """Codex trusts hooks by hash, so a written entry is inert until approved.
        Reporting success without saying so is the "looks wired and is not" failure
        (#177, #197) delivered by charter's own hand."""
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="codex"))
        self.assertEqual(rc, 0)
        self.assertTrue((self.home / "config.toml").is_file())
        self.assertIn("trust", text.lower())

    def test_a_harness_that_needs_no_opt_in_says_so_rather_than_failing(self):
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="opencode"))
        self.assertEqual(rc, 0)
        self.assertIn("charter init", text)

    def test_an_unknown_harness_is_refused_with_the_known_ones_named(self):
        rc, text = _run(commands_harness.cmd_harness_install,
                        SimpleNamespace(name="gemini-cli"))
        self.assertEqual(rc, 2)
        self.assertIn("codex", text)


if __name__ == "__main__":
    unittest.main()
