"""A **monorepo** control plane: `charter init` run inside the repo you already work in.

There is nothing to clone, so `workspaces/` holds no repos, and before this the status
line rendered

    ⬢ default · ws 0
    ▪ repos 0/0

in a checkout with a live branch, uncommitted work and an open PR — the one shape where
every column it draws has an answer and it showed none. Both halves of the fix are here:

* the plane's own tree is a repo row (`workspace.root_tree` → `_repo_trees`), counted,
  drawn in the plane's own cyan, and current whenever the cwd is inside it;
* with a single repo the ~12 unspent rows of `_MAX_REPO_LINES` go to that repo's
  worktrees as FULL rows — branch, dirty, ahead/behind — instead of one line of bare
  names. In a monorepo those worktrees are the whole parallel-task story, so they are
  what the left column is actually for.

Real git throughout (the tests/test_worktree.py pattern): a linked worktree's `.git` is a
FILE rather than a directory, and that difference is the thing under test in half of
these — a mocked git would prove nothing about it.
"""
from __future__ import annotations

import json
import os
import re
import subprocess
import unittest
from pathlib import Path

from charter import config, statusline, util, workspace
from tests._isolation import PersonaIso


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def init_repo(path: Path, branch: str = "main") -> Path:
    """A real git repo with one commit, so HEAD and `git status` both have answers."""
    path.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    git(path, "config", "user.email", "t@example.com")
    git(path, "config", "user.name", "t")
    (path / "README.md").write_text("hello\n")
    git(path, "add", "README.md")
    git(path, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
    return path


class MonorepoIso(PersonaIso):
    """An **embedded** control plane: ROOT is itself the git repo charter serves.

    Both halves are set up, because both are required — the declared shape AND the git
    repo. A fleet plane's root is usually a git repo too, so the declaration is what
    carries the meaning; `FleetPlaneIsUnchanged` below pins that.
    """

    def setUp(self) -> None:
        super().setUp()
        init_repo(self.tmp)
        (self.tmp / "charter.toml").write_text('schema = 1\n\n[plane]\nshape = "embedded"\n')
        config.HAS_CONTROL_PLANE = True
        config.PLANE_SHAPE = "embedded"

    def render(self, cwd: Path | None = None, width: int = 200) -> list[str]:
        """Content lines, ANSI and frame stripped — these tests are about which rows
        exist and what they say, not about the box drawn around them."""
        payload = {"session_id": "t"}
        if cwd is not None:
            payload["workspace"] = {"current_dir": str(cwd)}
        old = os.environ.get("COLUMNS")
        os.environ["COLUMNS"] = str(width)
        try:
            raw = _plain(statusline.render(payload)).split("\n")
        finally:
            if old is None:
                os.environ.pop("COLUMNS", None)
            else:
                os.environ["COLUMNS"] = old
        out = []
        for ln in raw:
            if not ln.strip() or set(ln.strip()) <= set("┌─┐└┘"):
                continue
            if ln.startswith("│ ") and ln.rstrip().endswith("│"):
                ln = ln[2:].rstrip()[:-1].rstrip()
            out.append(ln)
        return out

    def add_worktree(self, piece: str, repo: Path | None = None) -> Path:
        """A worktree at the path charter uses, created with PLAIN git — charter reads
        git's own registry rather than keeping one, so plain git is the honest fixture."""
        from charter import worktree
        repo = repo or self.tmp
        p = worktree.path_for("default", repo.name, piece)
        p.parent.mkdir(parents=True, exist_ok=True)
        git(repo, "worktree", "add", "-q", str(p), "-b", piece)
        return p


class RootTreeIsARepo(MonorepoIso):
    """The plane's own repo is a row, not an absence."""

    def test_root_tree_is_rendered_as_a_repo_row(self):
        lines = self.render()
        self.assertTrue(any(self.tmp.name in ln and "main" in ln for ln in lines),
                        f"root tree missing from rows: {lines}")

    def test_root_tree_is_counted(self):
        self.assertIn("repos 1", "\n".join(self.render()))

    def test_a_plane_whose_root_is_not_a_repo_has_no_root_row(self):
        """A fleet plane checked out as a plain directory keeps the old behaviour
        exactly — this must add a row only where there genuinely is a tree."""
        import shutil
        shutil.rmtree(self.tmp / ".git")
        self.assertIsNone(workspace.root_tree())
        self.assertIn("repos 0", "\n".join(self.render()))

    def test_dirty_state_of_the_root_tree_reaches_its_row(self):
        (self.tmp / "README.md").write_text("changed\n")
        row = [ln for ln in self.render() if self.tmp.name in ln][0]
        self.assertIn("main*", row, "root tree's dirty marker missing")


class FleetPlaneIsUnchanged(MonorepoIso):
    """The regression this shape check exists to prevent.

    A fleet plane's root is a git repo in the normal case — its personas carry committed
    memory, and docs/control-plane.md ships `exclude = ["this-control-plane"]` because the
    plane's own repo lives on the forge. Gating the trunk row on `ROOT/.git` alone would
    therefore have put EVERY existing fleet user's control-plane repo into their repo
    list, beside the clones they actually work in. Same filesystem as `MonorepoIso`; only
    the declaration differs.
    """

    def setUp(self) -> None:
        super().setUp()
        config.PLANE_SHAPE = "fleet"

    def test_a_git_root_alone_does_not_make_a_trunk(self):
        self.assertTrue((self.tmp / ".git").is_dir(), "fixture lost its repo")
        self.assertIsNone(workspace.root_tree())

    def test_the_plane_repo_is_not_listed_among_the_workspace_clones(self):
        clone = init_repo(config.WORKSPACES_DIR / "default" / "iam-service")
        self.assertEqual(workspace.repo_trees("default"), [clone])
        joined = "\n".join(self.render())
        self.assertIn("repos 1", joined)
        self.assertIn("iam-service", joined)
        self.assertNotIn(self.tmp.name, joined)

    def test_an_unknown_shape_falls_back_to_fleet(self):
        """A typo must cost a feature, not rearrange a working status line — the same
        fail-toward-no-change rule `share_of` uses."""
        from charter import instance
        self.assertEqual(instance.shape_of({"plane": {"shape": "embeded"}}), "fleet")
        self.assertEqual(instance.shape_of({}), "fleet")

    def test_the_shape_is_read_from_charter_toml(self):
        from charter import instance
        self.assertEqual(instance.shape_of(instance.load(self.tmp)), "embedded")


class CountDenominator(MonorepoIso):
    """`repos N/M` means "N of M in the inventory" — so `/M` needs an inventory."""

    def test_no_denominator_without_an_inventory(self):
        """`repos 1/0` states the opposite of the truth in a plane that will never run
        `discover`, because its one repo is already here."""
        joined = "\n".join(self.render())
        self.assertIn("repos 1", joined)
        self.assertNotIn("repos 1/0", joined)

    def test_denominator_returns_once_there_is_an_inventory(self):
        config.INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        config.INVENTORY.write_text(json.dumps({"count": 38, "repos": []}))
        self.assertIn("repos 1/38", "\n".join(self.render()))


class CurrentRepo(MonorepoIso):
    """Which row gets the you-are-here emphasis."""

    def test_cwd_inside_the_root_tree_marks_it_current(self):
        self.assertEqual(statusline._current({"cwd": str(self.tmp / "charter")}),
                         (None, self.tmp.name))

    def test_the_root_tree_belongs_to_no_single_workspace(self):
        """`None` for the workspace, not the active one's name: there is one root tree
        and it is in every workspace, so it is current whichever is active."""
        ws, repo = statusline._current({"cwd": str(self.tmp)})
        self.assertIsNone(ws)
        self.assertEqual(repo, self.tmp.name)

    def test_cwd_in_a_workspace_dir_does_not_mark_the_root_tree_current(self):
        """`workspaces/` lives UNDER the root, so a cwd there is also "inside" it.
        Falling through would bold the root row while you stand in a workspace."""
        (config.WORKSPACES_DIR / "demo").mkdir(parents=True)
        self.assertIsNone(statusline._current({"cwd": str(config.WORKSPACES_DIR / "demo")}))

    def test_cwd_outside_the_plane_entirely_is_not_current(self):
        self.assertIsNone(statusline._current({"cwd": str(self.tmp.parent)}))


class WorktreeBranches(MonorepoIso):
    """A linked worktree's `.git` is a FILE — `gitdir: <path>` — and its HEAD lives at
    that path. Readers that only handled the directory form returned `?` for every
    worktree, which in a monorepo plane is the entire branch column."""

    def test_branch_of_a_linked_worktree(self):
        wt = self.add_worktree("piece-a")
        self.assertFalse((wt / ".git").is_dir(), "fixture is not a linked worktree")
        self.assertEqual(util.branch_of(wt), "piece-a")

    def test_branch_of_a_clone_still_works(self):
        self.assertEqual(util.branch_of(self.tmp), "main")

    def test_branch_of_a_non_tree(self):
        self.assertEqual(util.branch_of(self.tmp / "nope"), "?")

    def test_branch_names_keep_their_slashes(self):
        wt = self.add_worktree("feat/deep/name")
        self.assertEqual(util.branch_of(wt), "feat/deep/name")


class WorktreeRows(MonorepoIso):
    """With one repo there are ~12 idle rows and nothing to compress — spend them."""

    def test_each_worktree_gets_its_own_row_with_its_branch(self):
        self.add_worktree("statusline")
        self.add_worktree("secrets")
        lines = self.render()
        for piece in ("statusline", "secrets"):
            row = [ln for ln in lines if re.search(rf"─ {piece}\b", ln)]
            self.assertTrue(row, f"no row for worktree {piece}: {lines}")
            self.assertIn(piece, row[0], "worktree row is missing its branch")

    def test_the_compact_summary_is_not_also_emitted(self):
        """The one-liner and the full rows say the same thing; both is duplication."""
        self.add_worktree("statusline")
        self.assertFalse([ln for ln in self.render() if "│   ╰─" in ln],
                         "compact summary rendered alongside full rows")

    def test_no_worktree_count_badge_when_every_piece_is_visible(self):
        """`⑂N` counts what you cannot see. Above two visible rows it restates them."""
        self.add_worktree("statusline")
        self.add_worktree("secrets")
        root_row = [ln for ln in self.render() if self.tmp.name in ln][0]
        self.assertNotIn("⑂", root_row)

    def test_a_repo_with_children_is_not_drawn_as_the_end_of_the_tree(self):
        self.add_worktree("statusline")
        root_row = [ln for ln in self.render() if self.tmp.name in ln][0]
        self.assertIn("├─", root_row)
        self.assertNotIn("└─", root_row)

    def test_the_last_child_uses_a_distinct_elbow(self):
        """NOT `└─`. `render`'s "the tree keeps going" rewrite searches backwards for the
        last `└─` to turn into `├─` when personas outnumber repos; a child sharing that
        marker gets rewritten instead of its repo. That bug is why `_TREE_WT` exists."""
        self.add_worktree("statusline")
        child = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertIn("╰─", child)
        self.assertNotIn("└─", child)

    def test_dirty_state_of_a_worktree_reaches_its_own_row(self):
        wt = self.add_worktree("statusline")
        (wt / "README.md").write_text("changed\n")
        row = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertIn("statusline*", row)

    def test_cwd_inside_a_worktree_is_not_reported_as_the_root_tree(self):
        """A worktree lives under `workspaces/`, so `_current` resolves it there and must
        not fall through to the root — otherwise every worktree bolds the wrong row."""
        wt = self.add_worktree("statusline")
        self.assertNotEqual(statusline._current({"cwd": str(wt)}), (None, self.tmp.name))


class MultiRepoKeepsTheSummary(MonorepoIso):
    """At two or more repos the trade flips back: a repo must never lose its row to
    another repo's worktrees, so their pieces go back to one compact line."""

    def setUp(self) -> None:
        super().setUp()
        self.clone = init_repo(config.WORKSPACES_DIR / "default" / "iam-service")

    def test_two_repos_render_two_rows(self):
        joined = "\n".join(self.render())
        self.assertIn("repos 2", joined)
        self.assertIn("iam-service", joined)

    def test_worktrees_collapse_back_to_the_compact_summary(self):
        self.add_worktree("piece-a", repo=self.clone)
        lines = self.render()
        self.assertTrue([ln for ln in lines if "│   ╰─" in ln],
                        f"compact summary missing with 2 repos: {lines}")
        self.assertFalse([ln for ln in lines if re.search(r"│  [├╰]─ piece-a", ln)],
                         "full worktree rows drawn with 2 repos")

    def test_the_badge_returns_when_pieces_are_summarised(self):
        self.add_worktree("piece-a", repo=self.clone)
        row = [ln for ln in self.render() if "iam-service" in ln][0]
        self.assertIn("⑂1", row)


class RepoTreesIsOneList(MonorepoIso):
    """`workspace.repo_trees` is the single answer to "which repos am I on?", so the
    status line cannot draw a repo whose forge state `gl-refresh` never fetched."""

    def test_root_tree_leads_the_list(self):
        clone = init_repo(config.WORKSPACES_DIR / "default" / "iam-service")
        self.assertEqual(workspace.repo_trees("default"), [self.tmp, clone])

    def test_a_plane_without_a_root_repo_is_just_its_clones(self):
        import shutil
        shutil.rmtree(self.tmp / ".git")
        clone = init_repo(config.WORKSPACES_DIR / "default" / "iam-service")
        self.assertEqual(workspace.repo_trees("default"), [clone])


class WorktreeCommandsReachTheRootTree(MonorepoIso):
    """In a monorepo plane `workspaces/` holds no clones, so without this every
    `charter worktree` command answers "isn't cloned in workspace X" about the one repo
    that is unmistakably present — the one you are standing in."""

    def test_clone_for_finds_the_root_tree(self):
        from charter import commands_worktree
        self.assertEqual(commands_worktree.clone_for("default", self.tmp.name), self.tmp)

    def test_an_unknown_name_is_still_unknown(self):
        from charter import commands_worktree
        self.assertIsNone(commands_worktree.clone_for("default", "not-a-repo"))

    def test_a_workspace_clone_wins_a_name_collision(self):
        """The clone was put in THIS workspace deliberately; the root tree is ambient.
        Preferring the ambient one would cut the worktree off the wrong repo."""
        from charter import commands_worktree
        clone = init_repo(config.WORKSPACES_DIR / "default" / self.tmp.name)
        self.assertEqual(commands_worktree.clone_for("default", self.tmp.name), clone)


if __name__ == "__main__":
    unittest.main()
