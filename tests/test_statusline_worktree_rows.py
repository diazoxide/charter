"""Worktree rows in the status line, and the repo-count denominator.

With one repo cloned, the ~12 unspent rows of `_MAX_REPO_LINES` go to that repo's
worktrees as FULL rows — branch, dirty, ahead/behind — instead of one line of bare names.
Worktrees are the parallel-task story, so they are what the left column is actually for;
with several repos the compact summary returns.

Converted from a suite that set this up as an **embedded** plane, where the repo under test
was the plane's own root tree. That shape is gone (docs/adr/0007) and the plane root is no
longer a repo row at all, so the fixture is now an ordinary workspace clone — which is what
every plane has. The 17 cases that were specifically about the root-tree row went with the
shape; these 18 were never about it.

Real git throughout (the tests/test_worktree.py pattern): a linked worktree's `.git` is a
FILE rather than a directory, and that difference is the thing under test in half of these
— a mocked git would prove nothing about it.
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


class ClonedRepoIso(PersonaIso):
    """A plane with one repo cloned into the active workspace, and real git in it."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.repo = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "demo"
        self.repo.mkdir(parents=True, exist_ok=True)
        init_repo(self.repo)

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
            if not ln.strip() or set(ln.strip()) <= set("┌─┐└┘├┤"):
                continue                  # top/bottom border, or a zone divider
            if ln.startswith("│ ") and ln.rstrip().endswith("│"):
                ln = ln[2:].rstrip()[:-1].rstrip()
            out.append(ln)
        return out

    def add_worktree(self, piece: str, repo: Path | None = None) -> Path:
        """A worktree at the path charter uses, created with PLAIN git — charter reads
        git's own registry rather than keeping one, so plain git is the honest fixture."""
        from charter import worktree
        repo = repo or self.repo
        p = worktree.path_for(config.DEFAULT_WORKSPACE, repo.name, piece)
        p.parent.mkdir(parents=True, exist_ok=True)
        git(repo, "worktree", "add", "-q", str(p), "-b", piece)
        return p


class WorktreeBranches(ClonedRepoIso):
    """A linked worktree's `.git` is a FILE — `gitdir: <path>` — and its HEAD lives at
    that path. Readers that only handled the directory form returned `?` for every
    worktree, which in a monorepo plane is the entire branch column."""

    def test_branch_of_a_linked_worktree(self):
        wt = self.add_worktree("piece-a")
        self.assertFalse((wt / ".git").is_dir(), "fixture is not a linked worktree")
        self.assertEqual(util.branch_of(wt), "piece-a")

    def test_branch_of_a_clone_still_works(self):
        self.assertEqual(util.branch_of(self.repo), "main")

    def test_branch_of_a_non_tree(self):
        self.assertEqual(util.branch_of(self.tmp / "nope"), "?")

    def test_branch_names_keep_their_slashes(self):
        wt = self.add_worktree("feat/deep/name")
        self.assertEqual(util.branch_of(wt), "feat/deep/name")

class WorktreeRows(ClonedRepoIso):
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
        repo_row = [ln for ln in self.render() if self.repo.name in ln][0]
        self.assertNotIn("⑂", repo_row)

    def test_a_repo_with_children_is_not_drawn_as_the_end_of_the_tree(self):
        self.add_worktree("statusline")
        repo_row = [ln for ln in self.render() if self.repo.name in ln][0]
        self.assertIn("├─", repo_row)
        self.assertNotIn("└─", repo_row)

    def test_the_last_child_uses_a_distinct_elbow(self):
        """NOT `└─`. `render`'s "the tree keeps going" rewrite searches backwards for the
        last `└─` to turn into `├─` when personas outnumber repos; a child sharing that
        marker gets rewritten instead of its repo. That bug is why `_TREE_WT` exists."""
        self.add_worktree("statusline")
        child = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertIn("╰─", child)
        self.assertNotIn("└─", child)

    def test_dirty_state_of_a_worktree_reaches_its_own_row(self):
        """The marker survives an emptied branch cell: dirty is true of the tree, not of
        what its branch is called."""
        wt = self.add_worktree("statusline")
        (wt / "README.md").write_text("changed\n")
        row = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertIn("*", row.split("statusline", 1)[1])

    def test_a_branch_matching_the_piece_name_is_not_printed_twice(self):
        """`worktree add <repo> <piece>` names the branch after the piece, so the default
        row said the same word in two columns and spent _BRANCH_W doing it."""
        self.add_worktree("statusline")
        row = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertEqual(row.count("statusline"), 1, row)

    def test_a_branch_that_differs_from_the_piece_name_is_shown(self):
        """Which is the case the column is now FOR: this piece is not on the branch you
        would assume from its name."""
        wt = self.add_worktree("statusline")
        git(wt, "checkout", "-q", "-b", "feat/renamed")
        row = [ln for ln in self.render() if re.search(r"─ statusline\b", ln)][0]
        self.assertIn("feat/renamed", row)

    def test_cwd_inside_a_worktree_is_not_reported_as_the_root_tree(self):
        """A worktree lives under `workspaces/`, so `_current` resolves it there and must
        not fall through to the root — otherwise every worktree bolds the wrong row."""
        wt = self.add_worktree("statusline")
        self.assertNotEqual(statusline._current({"cwd": str(wt)}), (None, self.repo.name))

class MultiRepoKeepsTheSummary(ClonedRepoIso):
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

class CountDenominator(ClonedRepoIso):
    """`repos N/M` means "N of M in the inventory" — so `/M` needs an inventory."""

    def test_no_denominator_without_an_inventory(self):
        """`repos 1/0` states the opposite of the truth in a plane that will never run
        `discover`, because its one repo is already here."""
        joined = "\n".join(self.render())
        self.assertIn("repos 1", joined)
        self.assertNotIn("repos 1/0", joined)

    def test_denominator_returns_once_there_is_an_inventory(self):
        """Records, not a bare count. `inventory.save` always writes
        ``count == len(repos)``, so a document claiming 38 with none listed is a state
        charter cannot produce — and the denominator is now what is actually clonable
        (`inventory.repos()`), which includes the plane repo and so cannot be read off
        `count` alone."""
        config.INVENTORY.parent.mkdir(parents=True, exist_ok=True)
        records = [{"name": f"r{i}", "path_with_namespace": f"acme/r{i}",
                    "forge": "github"} for i in range(38)]
        config.INVENTORY.write_text(json.dumps({"count": len(records), "repos": records}))
        self.assertIn("repos 1/38", "\n".join(self.render()))


if __name__ == "__main__":
    unittest.main()
