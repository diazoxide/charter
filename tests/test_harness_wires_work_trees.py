"""A harness must be wired where sessions actually START, not where the plane lives.

opencode does **not** search parent directories for plugins — verified by putting one in a
parent and booting `opencode serve` from a nested directory, where it never loaded. Work
happens in a clone or a worktree (ADR 0008: the plane root is not a work tree), so a shim
written only at the plane root is inert exactly where it matters: a mechanism that looks
wired and is not, which is what #177 and #197 already cost this repo.
"""

from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import doctor
from charter.harness import opencode, registry


def _tmp(case, git: bool = False) -> Path:
    p = Path(tempfile.mkdtemp(prefix="charter-tree-"))
    case.addCleanup(lambda: __import__("shutil").rmtree(p, True))
    if git:
        subprocess.run(["git", "init", "-q", "-b", "main", str(p)], check=True)
    return p


class WiringAWorkTree(unittest.TestCase):
    def test_opencode_puts_its_plugin_in_the_tree(self):
        tree = _tmp(self)
        registry.get("opencode").wire_tree(tree)
        self.assertTrue((tree / opencode.SHIM_PATH).is_file())

    def test_the_other_harnesses_leave_a_work_tree_alone(self):
        """Claude Code's plugin is installed once for the machine and its guard wiring is
        the plane's own `.claude/settings.json`; Codex's config is machine-wide. Neither
        has any business writing into somebody's repo."""
        for name in ("claude-code", "codex"):
            with self.subTest(harness=name):
                tree = _tmp(self)
                self.assertEqual(registry.get(name).wire_tree(tree), [])
                self.assertEqual(list(tree.iterdir()), [])

    def test_the_plugin_is_excluded_locally_rather_than_left_in_git_status(self):
        """`.git/info/exclude` is per-checkout and untracked — charter can keep its own
        file out of the way without editing a `.gitignore` the repo's owners maintain."""
        tree = _tmp(self, git=True)
        registry.get("opencode").wire_tree(tree)
        self.assertIn(".opencode/", (tree / ".git" / "info" / "exclude").read_text())
        out = subprocess.run(["git", "-C", str(tree), "status", "--short"],
                             capture_output=True, text=True).stdout
        self.assertNotIn(".opencode", out)

    def test_a_tree_without_git_is_still_wired(self):
        tree = _tmp(self)
        self.assertEqual(registry.get("opencode").wire_tree(tree)[0][0], "created")


class DoctorNamesUnwiredTrees(unittest.TestCase):
    def test_a_clone_missing_the_plugin_is_reported_with_the_fix(self):
        wired, bare = _tmp(self), _tmp(self)
        registry.get("opencode").wire_tree(wired)
        with mock.patch.object(doctor, "_work_trees", return_value=[wired, bare]):
            r = doctor.check_harness_trees()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(bare.name, r.detail)
        self.assertNotIn(wired.name, r.detail)
        self.assertIn("reinit", r.hint)

    def test_every_tree_wired_is_a_clean_row(self):
        wired = _tmp(self)
        registry.get("opencode").wire_tree(wired)
        with mock.patch.object(doctor, "_work_trees", return_value=[wired]):
            self.assertEqual(doctor.check_harness_trees().status, doctor.OK)

    def test_no_trees_is_not_reported_as_a_problem(self):
        with mock.patch.object(doctor, "_work_trees", return_value=[]):
            self.assertEqual(doctor.check_harness_trees().status, doctor.OK)



class WiringRefusesToInventTrees(unittest.TestCase):
    def test_a_path_that_does_not_exist_is_not_created(self):
        """`wire_work_tree` must never bring a tree into being. It ran on a clone path
        before the clone existed and left `a/`, `r0/`… in whatever directory the process
        happened to be standing in — a writer that creates its own target cannot tell a
        real tree from a typo."""
        from charter import commands

        parent = _tmp(self)
        ghost = parent / "never-cloned"
        self.assertEqual(commands.wire_work_tree(ghost), [])
        self.assertFalse(ghost.exists())


class LinkedWorktrees(unittest.TestCase):
    """A linked worktree's `.git` is a FILE, not a directory.

    The first version tested `.git/info`'s parent with `is_dir()`, so every worktree
    silently skipped the exclude — leaving charter's generated plugin as an untracked
    change, which made `charter worktree remove` refuse to remove a piece that had done
    nothing. Wiring a tree must not make that tree look dirty.
    """

    def _clone_with_worktree(self) -> tuple[Path, Path]:
        clone = _tmp(self, git=True)
        (clone / "f.txt").write_text("x\n")
        # `commit.gpgsign=false` is not optional here. A contributor may have signing on
        # globally, and a 1Password/hardware signer then makes this commit fail (exit 128)
        # or block on a prompt no test can answer — so the suite is green on CI, which has
        # no signer, and red on the machines that actually write the code. Every other
        # fixture in this suite pins it for the same reason.
        for cmd in (["add", "-A"], ["-c", "user.email=t@t", "-c", "user.name=t",
                                    "-c", "commit.gpgsign=false",
                                    "commit", "-qm", "init"]):
            subprocess.run(["git", "-C", str(clone), *cmd], check=True,
                           capture_output=True)
        wt = clone.parent / (clone.name + "-wt")
        self.addCleanup(lambda: __import__("shutil").rmtree(wt, True))
        subprocess.run(["git", "-C", str(clone), "worktree", "add", "-q", str(wt),
                        "-b", "piece"], check=True, capture_output=True)
        return clone, wt

    def test_its_dot_git_really_is_a_file(self):
        _clone, wt = self._clone_with_worktree()
        self.assertTrue((wt / ".git").is_file())

    def test_wiring_it_leaves_it_clean(self):
        _clone, wt = self._clone_with_worktree()
        registry.get("opencode").wire_tree(wt)
        self.assertTrue((wt / opencode.SHIM_PATH).is_file())
        out = subprocess.run(["git", "-C", str(wt), "status", "--short"],
                             capture_output=True, text=True).stdout
        self.assertEqual(out.strip(), "", f"wiring made the worktree dirty:\n{out}")

    def test_a_config_the_repo_already_had_is_not_hidden(self):
        """Charter hides what charter generated. An `opencode.json` the repo already
        carries is somebody's file — possibly one they are about to commit — and making
        it vanish from `git status` would be charter deciding that for them."""
        _clone, wt = self._clone_with_worktree()
        (wt / "opencode.json").write_text("{}\n")
        registry.get("opencode").wire_tree(wt)
        common = subprocess.run(["git", "-C", str(wt), "rev-parse", "--git-common-dir"],
                                capture_output=True, text=True).stdout.strip()
        exclude = (Path(common) / "info" / "exclude").read_text()
        self.assertIn(".opencode/", exclude)
        self.assertNotIn("\nopencode.json\n", exclude)


if __name__ == "__main__":
    unittest.main()
