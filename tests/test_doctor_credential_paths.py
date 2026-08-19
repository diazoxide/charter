"""ADR 0017, enforced after the fact as well as at the moment of writing.

`charter browser install` now gitignores the live session directory it causes to exist. That
binds at install time and nowhere else — so a plane that ran the command *before* the fix,
or whose `.gitignore` line was dropped in a merge, sits with traces of authenticated runs
staged for the next `git add -A`, and nothing says so. A trace records network requests with
their headers and bodies, so tracing a bridged login writes the credential to disk even
though it never reached the transcript. Which is the complaint #278 was actually
about: the same thing being re-derived, per plane, in silence.

FAIL rather than WARN, deliberately. `vault add` already exits non-zero for the identical
condition — credential material inside the plane that git would take — and the two must not
disagree about how serious it is.
"""
from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import browser, config, doctor


def _repo() -> Path:
    root = Path(tempfile.mkdtemp(prefix="charter-credpath-")).resolve()
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    return root


class TestCredentialPathsAreIgnored(unittest.TestCase):
    def _check(self, root):
        with mock.patch.object(config, "ROOT", root), \
             mock.patch.object(config, "HAS_CONTROL_PLANE", True):
            return doctor.check_credential_paths()

    def test_a_path_that_does_not_exist_is_not_a_finding(self):
        """Most planes never open a browser. A check that shouts at them is a check people
        learn to scroll past."""
        r = self._check(_repo())
        self.assertEqual(r.status, doctor.OK)

    def test_an_output_dir_git_would_take_fails(self):
        root = _repo()
        (root / browser.OUTPUT_DIR).mkdir()
        r = self._check(root)
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn(str(browser.OUTPUT_DIR), r.detail)

    def test_the_failure_names_the_command_that_fixes_it(self):
        """A finding whose remedy the reader has to work out is half a finding."""
        root = _repo()
        (root / browser.OUTPUT_DIR).mkdir()
        self.assertIn("charter browser install", self._check(root).hint)

    def test_an_ignored_output_dir_is_fine(self):
        root = _repo()
        (root / browser.OUTPUT_DIR).mkdir()
        (root / ".gitignore").write_text(f"{browser.OUTPUT_DIR}/\n")
        self.assertEqual(self._check(root).status, doctor.OK)

    def test_git_is_the_authority_not_the_root_gitignore_file(self):
        """Asked via `git check-ignore`, so nested ignore files, negations and global
        excludes all count — the same reasoning `_unignored_plaintext` already records."""
        root = _repo()
        (root / browser.OUTPUT_DIR).mkdir()
        (root / ".git" / "info").mkdir(parents=True, exist_ok=True)
        (root / ".git" / "info" / "exclude").write_text(f"{browser.OUTPUT_DIR}/\n")
        self.assertEqual(self._check(root).status, doctor.OK)

    def test_a_plane_that_is_not_a_repo_is_not_a_finding(self):
        """Nothing to commit to means no risk to report — not a warning about a risk that
        cannot exist."""
        root = Path(tempfile.mkdtemp(prefix="charter-credpath-nogit-")).resolve()
        (root / browser.OUTPUT_DIR).mkdir()
        self.assertEqual(self._check(root).status, doctor.OK)

    def test_the_table_covers_the_browser_output_dir(self):
        """Pinned so dropping the row is a deliberate act with a failing test, not a
        silent regression to the state #278 was filed about."""
        self.assertIn(browser.OUTPUT_DIR, [p for p, _, _ in doctor.CREDENTIAL_PATHS])


if __name__ == "__main__":
    unittest.main()
