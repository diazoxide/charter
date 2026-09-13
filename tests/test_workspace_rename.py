"""Workspace rename: move workspaces/<old>/ → workspaces/<new>/ (clones + memory +
manifest come along), fix the manifest ``name``, move the liveness gitignore block, and
repoint the active session/terminal pointer + lock. The LIVE commit/push path (glab) is
not exercised here — only the pure fs/pointer/liveness logic and the command's guards."""

from __future__ import annotations

import errno
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


class LinkedWorktreeCase(RenameCase):
    """A workspace `old` whose clone `api` is a real repository, on a plane behind a symlink."""

    def setUp(self) -> None:
        super().setUp()
        # The plane is reached through a symlink, so every path charter builds from `config`
        # is a spelling git never writes: git records a worktree's REAL path. Without this the
        # suite only normalised on macOS, where the temp dir itself sits behind `/var` ->
        # `/private/var`; on a Linux runner every `.resolve()` below could be deleted green.
        real = self.tmp / "real"
        real.mkdir()
        via = self.tmp / "via-link"
        via.symlink_to(real, target_is_directory=True)
        self.plane = via
        config.use(via)
        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
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

    def assertRelinked(self, said: str, count: int) -> None:
        """The command's own account agrees with git's: every tree counted, none named.

        Asserting git's state alone passed with every tree reported unrepaired — the success
        path has to be seen being recognised as one."""
        self.assertIn(f"git reads {count} linked worktree(s) as linked to their clone", said)
        self.assertNotIn("git does not read", said)
        self.assertNotIn("could not list", said)

    def _run_with(self, fake) -> tuple[int, str]:
        real = cw.util.run

        def run(cmd, *a, **kw):
            answer = fake(list(cmd))
            return real(cmd, *a, **kw) if answer is None else answer
        with mock.patch.object(cw.util, "run", run):
            return self._rename()


class RenameRelinksWorktrees(LinkedWorktreeCase):
    """A rename moves linked worktrees with their clone, and git has to be told (#963).

    Both halves of a linked worktree's link are absolute paths — the worktree's `.git` file
    names the clone's admin directory, and that directory's `gitdir` names the worktree back
    — so a plain directory move leaves git calling a live worktree prunable, and
    `worktree.list_for` answering "No worktrees." for a workspace that has one.
    """

    def test_a_piece_is_still_a_worktree_of_its_clone_after_the_rename(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        clone = workspace.workspace_dir("new") / "api"
        self.assertLinked(worktree.path_for("new", "api", "piece"), clone)
        self.assertEqual([r["piece"] for r in worktree.list_for(clone, "new")], ["piece"])
        self.assertRelinked(said, 1)

    def test_a_worktree_made_by_hand_inside_the_workspace_is_relinked_too(self):
        # Not under `.worktrees/`, so charter did not make it — but it moved all the same.
        self._link(workspace.workspace_dir("old") / "by-hand", "by-hand")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertLinked(workspace.workspace_dir("new") / "by-hand",
                          workspace.workspace_dir("new") / "api")
        self.assertRelinked(said, 1)

    def test_a_worktree_outside_the_workspace_follows_its_clone(self):
        # It did not move, but its `.git` file names the clone's OLD admin directory, so git
        # run inside it answered "not a git repository" while the clone still listed it fine.
        far = Path(tempfile.mkdtemp(prefix="edm-far-")) / "far"
        self.addCleanup(shutil.rmtree, far.parent, ignore_errors=True)
        self._link(far, "far")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertLinked(far, workspace.workspace_dir("new") / "api")
        self.assertRelinked(said, 1)

    def test_a_worktree_gone_before_the_rename_is_not_blamed_on_it(self):
        gone = self._link(worktree.path_for("old", "api", "gone"), "gone")
        shutil.rmtree(gone)
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertNotIn("/gone", said)
        self.assertLinked(worktree.path_for("new", "api", "piece"),
                          workspace.workspace_dir("new") / "api")
        self.assertRelinked(said, 1)

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
        self.assertRelinked(said, 1)

    def test_a_read_back_git_could_not_answer_confirms_nothing(self):
        # The repair runs; the listing asked afterwards fails. Nothing was read back, so no tree
        # may be counted as relinked — each is named, even though git did in fact mend it.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        clone = (workspace.workspace_dir("new") / "api").resolve()
        moved = worktree.path_for("new", "api", "piece").resolve()
        lists = []

        def second_list_fails(cmd):
            if cmd[3:5] != ["worktree", "list"]:
                return None
            lists.append(cmd)
            return subprocess.CompletedProcess(cmd, 128, "", "boom") if len(lists) > 1 else None
        rc, said = self._run_with(second_list_fails)
        self.assertEqual(rc, 0, said)
        self.assertEqual(len(lists), 2)
        self.assertIn(f"git -C {clone} worktree repair {moved}", said)
        self.assertNotIn("linked to their clone", said)


class RenameMovesWorktreesKeptOutsideThePlane(LinkedWorktreeCase):
    """With `$CHARTER_WORKTREES` or `[plane] worktrees`, a piece lives at
    `<root>/<ws>/<repo>/<piece>`, and that path is named after the workspace too (#1027).

    Moving `workspaces/<old>` alone left every piece at `<root>/<old>/…`, where nothing keyed
    on the new name looks — `wt list` found none, and the next `wt add` cut a second worktree
    beside the first.
    """

    def setUp(self) -> None:
        super().setUp()
        # Through a symlink as well, for the plane's reason: git writes real paths.
        real = self.tmp / "worktrees-real"
        real.mkdir()
        via = self.tmp / "worktrees-via-link"
        via.symlink_to(real, target_is_directory=True)
        env = mock.patch.dict(os.environ, {"CHARTER_WORKTREES": str(via)})
        env.start()
        self.addCleanup(env.stop)
        config.use(self.plane)
        self.root = via
        self.assertIsNotNone(config.WORKTREES_ROOT)

    def test_a_piece_follows_the_rename_to_the_new_name_under_the_root(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        clone = workspace.workspace_dir("new") / "api"
        moved = worktree.path_for("new", "api", "piece")
        self.assertEqual(moved.resolve(), (self.root / "new" / "api" / "piece").resolve())
        self.assertFalse((self.root / "old").exists())
        self.assertLinked(moved, clone)
        self.assertEqual([r["piece"] for r in worktree.list_for(clone, "new")], ["piece"])
        self.assertRelinked(said, 1)

    def assertNothingRenamed(self, rc: int, said: str, taken: Path) -> None:
        self.assertEqual(rc, 1, said)
        self.assertIn(f"{config.WORKTREES_ROOT / 'new'} already exists", said)
        self.assertIn("nothing was renamed", said)
        self.assertTrue(workspace.workspace_dir("old").is_dir())
        self.assertFalse(workspace.workspace_dir("new").exists())
        self.assertLinked(worktree.path_for("old", "api", "piece"),
                          workspace.workspace_dir("old") / "api")
        self.assertTrue(os.path.lexists(taken))

    def test_a_taken_directory_under_the_root_refuses_before_anything_moves(self):
        # Moving onto it would mix the pieces with whatever is there, and not moving would
        # leave `<root>/new` read as the renamed workspace's pieces — so neither half runs.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        taken = self.root / "new" / "api" / "stale"
        taken.mkdir(parents=True)
        rc, said = self._rename()
        self.assertNothingRenamed(rc, said, taken)

    def test_a_dangling_link_under_the_root_is_taken_too(self):
        # `exists()` follows the link and answers False, and a directory cannot be renamed
        # over a link — so this would be found only after `workspaces/<old>` had moved.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        taken = self.root / "new"
        taken.symlink_to(self.tmp / "nowhere", target_is_directory=True)
        rc, said = self._rename()
        self.assertNothingRenamed(rc, said, taken)

    def test_the_root_is_checked_even_with_no_worktrees_of_its_own_there(self):
        # What `<root>/new` holds would read as the renamed workspace's pieces all the same.
        (self.root / "new").mkdir()
        rc, said = self._rename()
        self.assertEqual(rc, 1, said)
        self.assertIn("nothing was renamed", said)
        self.assertTrue(workspace.workspace_dir("old").is_dir())

    def test_a_workspace_with_nothing_under_the_root_renames_as_before(self):
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        self.assertTrue(workspace.workspace_dir("new").is_dir())
        self.assertFalse(os.path.lexists(self.root / "new"))
        self.assertNotIn("!", said)
        self.assertNotIn("worktrees moved", said)

    def test_the_move_under_the_root_is_said(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        root = config.WORKTREES_ROOT
        self.assertIn(f"Its worktrees moved with it: {root / 'old'} → {root / 'new'}.", said)

    def test_pieces_under_both_roots_follow_the_rename(self):
        # A plane that has just declared a root still has yesterday's pieces in `.worktrees/`
        # (`worktree.locate`), and those move with the workspace directory itself.
        inside = self._link(workspace.workspace_dir("old") / worktree.DIR_NAME / "api" / "early",
                            "early")
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        rc, said = self._rename()
        self.assertEqual(rc, 0, said)
        clone = workspace.workspace_dir("new") / "api"
        self.assertLinked(workspace.workspace_dir("new") / inside.relative_to(
            workspace.workspace_dir("old")), clone)
        self.assertLinked(worktree.path_for("new", "api", "piece"), clone)
        self.assertRelinked(said, 2)

    def _move_fails(self, err: int):
        """`os.rename` refusing the root's `<old>` alone, the way a mount point or a directory
        the operator cannot write refuses it; `workspaces/<old>` still moves."""
        real = os.rename
        target = os.path.realpath(self.root / "old")

        def rename(src, dst, *a, **kw):
            if os.path.realpath(src) == target:
                raise OSError(err, os.strerror(err), os.fspath(src), None, os.fspath(dst))
            return real(src, dst, *a, **kw)
        return mock.patch.object(os, "rename", rename)

    def test_a_move_under_the_root_that_fails_is_named_with_the_command_that_finishes_it(self):
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        root = config.WORKTREES_ROOT
        with self._move_fails(errno.EXDEV):
            rc, said = self._rename()
        # Not rolled back: `workspaces/<old>` moved, as #963 decided. Not a success either.
        self.assertEqual(rc, 1, said)
        self.assertTrue(workspace.workspace_dir("new").is_dir())
        self.assertFalse(workspace.workspace_dir("old").exists())
        self.assertNotIn("✓", said)
        self.assertIn(f"but not its worktrees: {root / 'old'} is still at the old name "
                      f"({os.strerror(errno.EXDEV)})", said)
        clone = (workspace.workspace_dir("new") / "api").resolve()
        # Where it was left, git still reads it: the clone moved, and the relink followed it.
        self.assertLinked(root / "old" / "api" / "piece", clone)
        finish = (f"Finish the rename: mv {root / 'old'} {root / 'new'} && "
                  f"git -C {clone} worktree repair {root / 'new' / 'api' / 'piece'}")
        self.assertIn(finish, said)
        # And the command it printed is the one that finishes it.
        proc = subprocess.run(["sh", "-c", finish.removeprefix("Finish the rename: ")],
                              capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertLinked(worktree.path_for("new", "api", "piece"), clone)
        self.assertEqual([r["piece"] for r in worktree.list_for(clone, "new")], ["piece"])

    def test_the_finishing_command_names_only_the_pieces_left_behind(self):
        # The in-plane piece moved with `workspaces/<old>` and is relinked there; repairing it
        # at a path under the root would name a directory that never exists.
        inside = self._link(workspace.workspace_dir("old") / worktree.DIR_NAME / "api" / "early",
                            "early")
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        root = config.WORKTREES_ROOT
        with self._move_fails(errno.EACCES):
            rc, said = self._rename()
        self.assertEqual(rc, 1, said)
        clone = (workspace.workspace_dir("new") / "api").resolve()
        self.assertIn(f"Finish the rename: mv {root / 'old'} {root / 'new'} && git -C {clone} "
                      f"worktree repair {root / 'new' / 'api' / 'piece'}\n", said)
        self.assertLinked(workspace.workspace_dir("new") / inside.relative_to(
            workspace.workspace_dir("old")), clone)
        self.assertRelinked(said, 2)

    def test_a_live_workspace_still_commits_its_move_and_exits_non_zero(self):
        # The tracked move did happen and is owed its commit; the pieces left behind are what
        # the exit code is about.
        self._link(worktree.path_for("old", "api", "piece"), "piece")
        (config.ROOT / ".gitignore").write_text("/workspaces/*/*\n")
        workspace.set_live("old", True)
        with self._move_fails(errno.EXDEV), \
                mock.patch.object(cw, "commit_push", return_value=0) as commit:
            rc, said = self._rename()
        self.assertEqual(rc, 1, said)
        commit.assert_called_once()
        self.assertIn("but not its worktrees", said)


if __name__ == "__main__":
    unittest.main()
