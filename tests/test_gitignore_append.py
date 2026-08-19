"""The one writer every command that adds an ignore rule goes through.

`charter init` had the only implementation, inlined, and the next command that needed to
ignore a path it creates (`charter browser install`, for the live browser session under
`.playwright-cli/`) would otherwise have written a second one. Two writers is how a plane
ends up with the same rule twice, or with one of them silently reordering a file the other
appends to — and `.gitignore` here is not free-form: `workspace.set_live()` splices its
managed block at the literal anchor line `!/workspaces/.gitkeep`, so a writer that rewrites
rather than appends breaks a feature that has nothing to do with it.

What this pins is therefore the contract, not the caller: append-only, idempotent, and
existing content untouched.
"""
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from charter import util


class TestAppendGitignore(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-ignore-"))
        self.p = self.root / ".gitignore"

    def test_it_creates_the_file_when_the_plane_has_none(self):
        """A plane that has never needed one is the common case for a fresh clone, and
        "no .gitignore" must not read as "nothing to do"."""
        added = util.append_gitignore(self.root, [".playwright-cli/"], "by a test")
        self.assertEqual(added, [".playwright-cli/"])
        self.assertIn(".playwright-cli/", self.p.read_text().splitlines())

    def test_it_is_idempotent(self):
        """Every `browser install` re-runs this. A second run that appends again would
        grow the file without bound and make the diff of an unrelated PR unreadable."""
        util.append_gitignore(self.root, [".playwright-cli/"], "by a test")
        added = util.append_gitignore(self.root, [".playwright-cli/"], "by a test")
        self.assertEqual(added, [])
        self.assertEqual(self.p.read_text().count(".playwright-cli/"), 1)

    def test_it_never_touches_what_is_already_there(self):
        """The anchor `workspace.set_live()` greps for lives in this file. A writer that
        rewrote or reordered would break live workspaces from across the codebase."""
        before = "/workspaces/*/*\n!/workspaces/.gitkeep\n\n# mine\n*.log\n"
        self.p.write_text(before)
        util.append_gitignore(self.root, [".playwright-cli/"], "by a test")
        self.assertTrue(self.p.read_text().startswith(before.rstrip("\n")))
        self.assertIn("!/workspaces/.gitkeep", self.p.read_text())

    def test_it_appends_only_the_lines_that_are_missing(self):
        self.p.write_text("*.log\n.playwright-cli/\n")
        added = util.append_gitignore(self.root, [".playwright-cli/", "seen/"], "by a test")
        self.assertEqual(added, ["seen/"])

    def test_a_line_matches_only_as_a_whole_line(self):
        """A substring test would see `.playwright-cli/` inside `build/.playwright-cli/x`
        and skip the write — the exact bug `_ensure_gitignore`'s docstring records for
        `workspaces/`, where a wrong skip cost the live-workspace anchor."""
        self.p.write_text("build/.playwright-cli/output\n")
        added = util.append_gitignore(self.root, [".playwright-cli/"], "by a test")
        self.assertEqual(added, [".playwright-cli/"])

    def test_the_header_says_which_command_wrote_the_lines(self):
        """A rule with no provenance is a rule nobody dares delete."""
        util.append_gitignore(self.root, ["x/"], "added by `charter browser install`")
        self.assertIn("# added by `charter browser install`", self.p.read_text())


if __name__ == "__main__":
    unittest.main()
