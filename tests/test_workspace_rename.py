"""Workspace rename: move workspaces/<old>/ → workspaces/<new>/ (clones + memory +
manifest come along), fix the manifest ``name``, move the liveness gitignore block, and
repoint the active session/terminal pointer + lock. The LIVE commit/push path (glab) is
not exercised here — only the pure fs/pointer/liveness logic and the command's guards."""

from __future__ import annotations

import io
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import config, workspace, worktree
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


class RenameCase(PersonaIso):
    def _mk(self, name, *, live=False, manifest=True, clone="repoA"):
        wd = workspace.workspace_dir(name)
        (wd / "memory").mkdir(parents=True, exist_ok=True)
        if clone:
            (wd / clone).mkdir(parents=True, exist_ok=True)
            (wd / clone / "f.txt").write_text("x")
        if manifest:
            workspace.write_manifest(name, {"name": name,
                                            "repos": [{"name": clone, "branch": "main"}]})
        if live:
            (config.ROOT / ".gitignore").write_text("/workspaces/*/*\n")
            workspace.set_live(name, True)
        return wd

    def _args(self, old, new, message=None):
        return SimpleNamespace(old=old, new=new, message=message)

    # --- workspace.rename: pure fs + manifest + pointers + liveness ---
    def test_moves_dir_clones_and_manifest_name(self):
        self._mk("old")
        workspace.rename("old", "new")
        self.assertFalse(workspace.workspace_dir("old").exists())
        self.assertTrue((workspace.workspace_dir("new") / "repoA" / "f.txt").exists())
        self.assertEqual(workspace.read_manifest("new")["name"], "new")

    def test_repoints_active_pointer_and_lock_only_for_matching_value(self):
        self._mk("old")
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        (config.SESSIONS_DIR / "sid.workspace").write_text("old\n")
        (config.SESSIONS_DIR / "sid.lock").write_text("old\n")
        (config.SESSIONS_DIR / "other.workspace").write_text("keepme\n")
        workspace.rename("old", "new")
        self.assertEqual((config.SESSIONS_DIR / "sid.workspace").read_text().strip(), "new")
        self.assertEqual((config.SESSIONS_DIR / "sid.lock").read_text().strip(), "new")
        self.assertEqual((config.SESSIONS_DIR / "other.workspace").read_text().strip(), "keepme")

    def test_moves_liveness_block(self):
        self._mk("old", live=True)
        self.assertTrue(workspace.is_live("old"))
        workspace.rename("old", "new")
        self.assertFalse(workspace.is_live("old"))
        self.assertTrue(workspace.is_live("new"))
        gi = (config.ROOT / ".gitignore").read_text()
        self.assertIn("!/workspaces/new/workspace.json", gi)
        self.assertNotIn("!/workspaces/old/", gi)

    def test_local_workspace_liveness_stays_off(self):
        self._mk("old")  # not live
        workspace.rename("old", "new")
        self.assertFalse(workspace.is_live("new"))

    # --- cmd_workspace_rename: LOCAL success (no git push) + guards ---
    def test_command_local_success(self):
        self._mk("old")
        rc = cw.cmd_workspace_rename(self._args("old", "new"))
        self.assertEqual(rc, 0)
        self.assertTrue(workspace.workspace_dir("new").exists())
        self.assertFalse(workspace.workspace_dir("old").exists())

    def test_refuse_missing_old(self):
        self.assertEqual(cw.cmd_workspace_rename(self._args("ghost", "new")), 1)

    def test_refuse_existing_new(self):
        self._mk("old")
        self._mk("taken")
        self.assertEqual(cw.cmd_workspace_rename(self._args("old", "taken")), 1)
        self.assertTrue(workspace.workspace_dir("old").exists())  # left untouched

    def test_refuse_invalid_new_name(self):
        self._mk("old")
        self.assertEqual(cw.cmd_workspace_rename(self._args("old", "Bad Name")), 1)
        self.assertTrue(workspace.workspace_dir("old").exists())

    def test_refuse_same_name(self):
        self._mk("old")
        self.assertEqual(cw.cmd_workspace_rename(self._args("old", "old")), 1)


def _git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)


class RenameRelinksWorktrees(RenameCase):
    """A rename moves linked worktrees with their clone, and git has to be told (#963).

    Both halves of a linked worktree's link are absolute paths — the worktree's `.git` file
    names the clone's admin directory, and that directory's `gitdir` names the worktree back
    — so a plain directory move leaves git calling a live worktree prunable, and
    `worktree.list_for` answering "No worktrees." for a workspace that has one.
    """

    def setUp(self) -> None:
        super().setUp()
        self.clone = workspace.workspace_dir("old") / "api"
        self.clone.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.clone)],
                       check=True, capture_output=True)
        (self.clone / "README.md").write_text("hello\n")
        _git(self.clone, "add", "README.md")
        self.assertEqual(_git(self.clone, "commit", "-qm", "init").returncode, 0)

    def _link(self, path: Path, branch: str) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        proc = _git(self.clone, "worktree", "add", "-q", str(path), "-b", branch)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return path

    def _rename(self) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cw.cmd_workspace_rename(self._args("old", "new"))
        return rc, out.getvalue() + err.getvalue()

    def assertLinked(self, tree: Path, clone: Path) -> None:
        """Both directions, as git itself reads them."""
        # worktree -> clone: git run inside the tree finds the clone's object store.
        proc = _git(tree, "rev-parse", "--git-common-dir")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        common = Path(proc.stdout.strip())
        if not common.is_absolute():
            common = tree / common
        self.assertEqual(common.resolve(), (clone / ".git").resolve())
        # clone -> worktree: the clone lists it at this path, and not as prunable.
        rows = worktree.parse_porcelain(_git(clone, "worktree", "list", "--porcelain").stdout)
        mine = [r for r in rows if Path(r["path"]).resolve() == tree.resolve()]
        self.assertEqual(len(mine), 1, rows)
        self.assertFalse(mine[0]["prunable"], mine[0])

    def test_a_piece_is_still_a_worktree_of_its_clone_after_the_rename(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        clone = workspace.workspace_dir("new") / "api"
        self.assertLinked(worktree.path_for("new", "api", "piece"), clone)
        self.assertEqual([r["piece"] for r in worktree.list_for(clone, "new")], ["piece"])

    def test_a_worktree_made_by_hand_inside_the_workspace_is_relinked_too(self):
        # Not under `.worktrees/`, so charter did not make it — but it moved all the same.
        self._link(workspace.workspace_dir("old") / "by-hand", "by-hand")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertLinked(workspace.workspace_dir("new") / "by-hand",
                          workspace.workspace_dir("new") / "api")

    def test_a_worktree_outside_the_workspace_follows_its_clone(self):
        # It did not move, but its `.git` file names the clone's OLD admin directory, so git
        # run inside it answered "not a git repository" while the clone still listed it fine.
        far = Path(tempfile.mkdtemp(prefix="edm-far-")) / "far"
        self.addCleanup(shutil.rmtree, far.parent, ignore_errors=True)
        self._link(far, "far")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertLinked(far, workspace.workspace_dir("new") / "api")

    def test_a_worktree_gone_before_the_rename_is_not_blamed_on_it(self):
        gone = self._link(worktree.path_for("old", "api", "gone"), "gone")
        shutil.rmtree(gone)
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertNotIn("/gone", said)
        self.assertLinked(worktree.path_for("new", "api", "piece"),
                          workspace.workspace_dir("new") / "api")

    def _run_with(self, fake) -> tuple[int, str]:
        real = cw.util.run

        def run(cmd, *a, **kw):
            answer = fake(list(cmd))
            return real(cmd, *a, **kw) if answer is None else answer
        with mock.patch.object(cw.util, "run", run):
            return self._rename()

    def test_a_repair_that_did_not_take_is_named_with_the_command_that_finishes_it(self):
        """A repair whose exit code says success proves nothing: what counts is git reading
        the link back. Stubbed to succeed and do nothing, so every link stays broken."""
        piece = self._link(worktree.path_for("old", "api", "piece"), "piece")
        far = Path(tempfile.mkdtemp(prefix="edm-far-")) / "far"
        self.addCleanup(shutil.rmtree, far.parent, ignore_errors=True)
        self._link(far, "far")
        rc, said = self._run_with(lambda cmd: subprocess.CompletedProcess(cmd, 0, "", "")
                                  if cmd[3:5] == ["worktree", "repair"] else None)
        # Resolved: the command names real paths, and `/tmp` is a symlink on macOS.
        far = far.resolve()
        # The rename stands: undoing it would not make the links any safer.
        self.assertEqual(rc, 0, said)
        self.assertTrue(workspace.workspace_dir("new").is_dir())
        self.assertFalse(workspace.workspace_dir("old").exists())
        clone = (workspace.workspace_dir("new") / "api").resolve()
        moved = worktree.path_for("new", "api", "piece").resolve()
        self.assertIn(f"git -C {clone} worktree repair {moved}", said)
        # The outside tree is listed fine from the clone; only git inside it is broken.
        self.assertIn(f"git -C {clone} worktree repair {far}", said)
        self.assertNotIn("linked to their clone", said)
        # And the command it printed is the one that works.
        proc = _git(clone, "worktree", "repair", str(moved), str(far))
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertLinked(moved, clone)
        self.assertLinked(far, clone)
        self.assertFalse(piece.exists())

    def test_a_tree_only_half_repaired_is_still_named(self):
        # The tree's own `.git` file mended and the clone's `gitdir` left naming the old path:
        # git inside the tree works, and the clone still calls it prunable.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        clone = (workspace.workspace_dir("new") / "api").resolve()
        moved = worktree.path_for("new", "api", "piece").resolve()

        def half(cmd):
            if cmd[3:5] != ["worktree", "repair"]:
                return None
            (moved / ".git").write_text(f"gitdir: {(clone / '.git').resolve()}/worktrees/piece\n")
            return subprocess.CompletedProcess(cmd, 0, "", "")
        rc, said = self._run_with(half)
        self.assertEqual(rc, 0, said)
        self.assertEqual(_git(moved, "status", "--short").returncode, 0)
        self.assertIn(f"git -C {clone} worktree repair {moved}", said)

    def test_a_clone_git_could_not_list_is_named_not_passed_over(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        clone = (workspace.workspace_dir("new") / "api").resolve()
        rc, said = self._run_with(lambda cmd: subprocess.CompletedProcess(cmd, 128, "", "boom")
                                  if cmd[3:5] == ["worktree", "list"] else None)
        self.assertEqual(rc, 0, said)
        self.assertIn(f"git could not list the worktrees of {clone}", said)
        self.assertIn(f"git -C {clone} worktree repair", said)

    def test_a_clone_with_no_linked_worktree_is_not_asked(self):
        # Nothing to relink, so a git that cannot list it has nothing to report either.
        rc, said = self._run_with(lambda cmd: subprocess.CompletedProcess(cmd, 128, "", "boom")
                                  if cmd[3:5] == ["worktree", "list"] else None)
        self.assertEqual(rc, 0, said)
        self.assertNotIn("could not list", said)

    def test_a_rename_run_from_a_git_hook_still_relinks_its_own_clone(self):
        # A hook exports GIT_DIR, and `-C <clone>` does not override it: every git below would
        # otherwise list, repair and read back the hook's repository instead.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        other = self.tmp / "hook-repo"
        subprocess.run(["git", "init", "-q", str(other)], check=True, capture_output=True)
        with mock.patch.dict(os.environ, {"GIT_DIR": str(other / ".git")}):
            rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertLinked(worktree.path_for("new", "api", "piece"),
                          workspace.workspace_dir("new") / "api")


if __name__ == "__main__":
    unittest.main()
