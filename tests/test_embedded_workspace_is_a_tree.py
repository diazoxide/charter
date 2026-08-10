"""In an embedded plane, a workspace IS a working tree.

Embedded means the codebase cannot be cloned apart, so a workspace that materialised
nothing isolated nothing: `repo_trees` returned the same root tree for every workspace, so
`ws use alpha` then `ws use beta` changed the memory namespace and not one file being
edited. Two agents in two workspaces were editing the same files on the same branch, while
charter's central promise is "never mix workspaces".

The model now reads the same in both shapes — *a workspace is a set of working trees* —
with one repo collapsing the fleet's "set" to exactly one:

    default      → the root tree (a solo user never learns a second directory)
    <other>      → <worktrees-root>/<ws>/<repo>/<ws>, on its own branch
"""
from __future__ import annotations

import io
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands_workspace as cw
from charter import config, workspace, worktree
from tests._isolation import PersonaIso
from tests.test_statusline_monorepo import MonorepoIso, git


def _create(name, **kw):
    args = SimpleNamespace(name=name, use=kw.pop("use", False), force=False,
                           live=False, vision=None, repos=[],
                           branch=kw.pop("branch", None))
    with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
        return cw.cmd_workspace_create(args)


def _use(name, **kw):
    args = SimpleNamespace(name=name, force=kw.pop("force", False),
                           create=kw.pop("create", False))
    err = io.StringIO()
    with redirect_stdout(io.StringIO()), redirect_stderr(err):
        rc = cw.cmd_workspace_use(args)
    return rc, err.getvalue()


class AWorkspaceOwnsATree(MonorepoIso):
    def setUp(self) -> None:
        super().setUp()
        git(self.tmp, "add", "-A")
        git(self.tmp, "-c", "commit.gpgsign=false", "commit", "-qm", "plane")

    def test_default_owns_the_root_tree(self):
        """The simplest case must not become the strangest one: `charter init`, then work
        in your repo, with no extra directory to learn."""
        self.assertEqual(workspace.own_tree("default"), self.tmp)

    def test_another_workspace_has_no_tree_until_it_is_created(self):
        self.assertIsNone(workspace.own_tree("feature-a"))

    def test_creating_one_materialises_its_tree(self):
        self.assertEqual(_create("feature-a"), 0)
        own = workspace.own_tree("feature-a")
        self.assertIsNotNone(own)
        self.assertTrue((own / ".git").exists())

    def test_the_tree_is_on_its_own_branch(self):
        _create("feature-a")
        own = workspace.own_tree("feature-a")
        self.assertEqual(
            subprocess.run(["git", "-C", str(own), "branch", "--show-current"],
                           capture_output=True, text=True).stdout.strip(), "feature-a")

    def test_the_branch_can_be_named(self):
        _create("feature-a", branch="feat/a")
        own = workspace.own_tree("feature-a")
        self.assertEqual(
            subprocess.run(["git", "-C", str(own), "branch", "--show-current"],
                           capture_output=True, text=True).stdout.strip(), "feat/a")

    def test_two_workspaces_edit_different_files(self):
        """The whole point, stated as the failure it replaces."""
        _create("feature-a")
        _create("feature-b")
        a, b = workspace.own_tree("feature-a"), workspace.own_tree("feature-b")
        (a / "code.txt").write_text("v-A\n")
        (b / "code.txt").write_text("v-B\n")
        self.assertEqual((a / "code.txt").read_text(), "v-A\n")
        self.assertEqual((b / "code.txt").read_text(), "v-B\n")
        self.assertNotEqual(a, b)

    def test_repo_trees_differs_per_workspace(self):
        """It returned the same root for every workspace, which is what made them all one
        workspace."""
        _create("feature-a")
        self.assertNotEqual(workspace.repo_trees("default"),
                            workspace.repo_trees("feature-a"))

    def test_a_clone_in_an_embedded_workspace_still_counts(self):
        """Nothing stops `charter clone` here, and a hybrid — the codebase plus a vendored
        dependency — is reasonable to have."""
        from tests.test_statusline_monorepo import init_repo
        clone = init_repo(config.WORKSPACES_DIR / "default" / "vendored")
        self.assertIn(clone, workspace.repo_trees("default"))
        self.assertIn(self.tmp, workspace.repo_trees("default"))


class SelectingAWorkspaceWithoutATreeIsRefused(MonorepoIso):
    def setUp(self) -> None:
        super().setUp()
        git(self.tmp, "add", "-A")
        git(self.tmp, "-c", "commit.gpgsign=false", "commit", "-qm", "plane")

    def test_a_treeless_workspace_is_refused(self):
        (config.WORKSPACES_DIR / "orphan" / "memory").mkdir(parents=True)
        rc, err = _use("orphan")
        self.assertEqual(rc, 1)
        self.assertIn("no working tree of its own", err)

    def test_the_refusal_says_how_to_fix_it(self):
        (config.WORKSPACES_DIR / "orphan" / "memory").mkdir(parents=True)
        _rc, err = _use("orphan")
        self.assertIn("charter workspace create orphan", err)

    def test_one_with_a_tree_selects_fine(self):
        _create("feature-a")
        rc, _ = _use("feature-a")
        self.assertEqual(rc, 0)

    def test_default_is_always_selectable(self):
        """It is the always-present workspace; `ensure` creates it on demand. The
        unknown-name guard refused it in a plane that had never made one — every fresh
        plane."""
        rc, err = _use("default")
        self.assertEqual(rc, 0, err)


class TheCwdDecidesTheWorkspace(MonorepoIso):
    """A workspace's trees live at paths that name the workspace, so being inside one is
    not a hint — it is the fact, and no pointer can contradict it. That also removes this
    shape from the pointer-sharing class of bug entirely."""

    def setUp(self) -> None:
        super().setUp()
        git(self.tmp, "add", "-A")
        git(self.tmp, "-c", "commit.gpgsign=false", "commit", "-qm", "plane")
        _create("feature-a")
        self.own = workspace.own_tree("feature-a")

    def test_inside_a_workspace_tree_resolves_to_it(self):
        self.assertEqual(workspace.from_path(self.own), "feature-a")

    def test_deep_inside_resolves_too(self):
        deep = self.own / "src" / "pkg"
        deep.mkdir(parents=True)
        self.assertEqual(workspace.from_path(deep), "feature-a")

    def test_the_plane_root_is_not_a_workspace_tree(self):
        self.assertIsNone(workspace.from_path(self.tmp))

    def test_it_outranks_a_pointer_that_disagrees(self):
        """The pointer-sharing bug, made unreachable for this shape: whatever any pointer
        says, the tree you are standing in wins."""
        import os
        workspace.set_active("default", session_id="s1")
        old = os.getcwd()
        os.chdir(self.own)
        try:
            self.assertEqual(workspace.resolve(session_id="s1"), "feature-a")
            self.assertEqual(workspace.source(session_id="s1"), "cwd")
        finally:
            os.chdir(old)

    def test_an_explicit_flag_still_wins(self):
        self.assertEqual(workspace.resolve(explicit="other"), "other")

    def test_a_fleet_clone_path_resolves_too(self):
        """Same rule, other shape: `workspaces/<ws>/<repo>` names its workspace."""
        d = config.WORKSPACES_DIR / "alpha" / "svc"
        d.mkdir(parents=True)
        self.assertEqual(workspace.from_path(d), "alpha")

    def test_the_workspace_container_alone_is_not_a_tree(self):
        d = config.WORKSPACES_DIR / "alpha"
        d.mkdir(parents=True)
        self.assertIsNone(workspace.from_path(d))


class FleetIsUnchanged(PersonaIso):
    def test_own_tree_is_meaningless_in_a_fleet_plane(self):
        config.PLANE_SHAPE = "fleet"
        self.assertIsNone(workspace.own_tree("anything"))

    def test_repo_trees_still_returns_root_plus_clones(self):
        from tests.test_statusline_monorepo import init_repo
        config.PLANE_SHAPE = "fleet"
        clone = init_repo(config.WORKSPACES_DIR / "default" / "svc")
        self.assertEqual(workspace.repo_trees("default"), [clone])


if __name__ == "__main__":
    unittest.main()
