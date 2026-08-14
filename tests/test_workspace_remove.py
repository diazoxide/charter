"""`charter workspace remove` — what it warns about, and what it refuses over.

The two are deliberately different sets. Removal refuses to discard **work at risk** —
uncommitted or unpushed commits, which nothing can recover. Open todos are reported and
never block: a workspace whose todos are all abandoned is precisely the one worth deleting,
and making that case need `--force` teaches the habit of reaching for `--force`, which is
how a guard stops protecting the commits it exists for.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import todos, workspace, worktree
from charter import commands_workspace as cw
from tests._isolation import PersonaIso

#: `git init` has to work regardless of who runs the suite, so identity comes from the
#: call rather than from the developer's ~/.gitconfig.
_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


class RemoveCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")

    def remove(self, name: str = "alpha", force: bool = False):
        """Returns (rc, stdout + stderr) — util.err/warn write to stderr."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cw.cmd_workspace_remove(SimpleNamespace(name=name, force=force))
        return rc, out.getvalue() + err.getvalue()

    def make_dirty_clone(self, name: str = "alpha", repo: str = "svc"):
        """A real clone inside the workspace with an uncommitted file — genuine work at
        risk, the thing removal exists to guard."""
        d = workspace.workspace_dir(name) / repo
        d.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", str(d)], check=True, capture_output=True,
                       env={**os.environ, **_GIT_ENV})
        (d / "unsaved.txt").write_text("work nobody has committed\n")
        return d

    def git(self, cwd, *args: str):
        return subprocess.run(["git", "-C", str(cwd), *args], check=True,
                              capture_output=True, text=True,
                              env={**os.environ, **_GIT_ENV})

    def commit(self, cwd, message: str):
        """`commit.gpgsign=false` per `tests/test_worktree.py` — a developer whose global
        config signs commits gets an interactive signer, which hangs the suite forever."""
        self.git(cwd, "add", "-A")
        self.git(cwd, "-c", "commit.gpgsign=false", "commit", "-qm", message)

    def make_clone_with_worktree(self, name: str = "alpha", repo: str = "svc",
                                 piece: str = "slice"):
        """A clone carrying one commit, plus a linked worktree on its own branch.

        The worktree's ``.git`` is a FILE, which is precisely why `workspace.clones()`
        cannot see it — and why removal used to delete it unguarded.
        """
        env = {**os.environ, **_GIT_ENV}
        clone = workspace.workspace_dir(name) / repo
        clone.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(clone)], check=True,
                       capture_output=True, env=env)
        (clone / "README").write_text("base\n")
        self.commit(clone, "base")

        wt = worktree.path_for(name, repo, piece)
        wt.parent.mkdir(parents=True, exist_ok=True)
        self.git(clone, "worktree", "add", "-q", "-b", piece, str(wt))
        return clone, wt


class TestTodosAreReportedOnRemoval(RemoveCase):
    def test_removal_says_how_many_open_todos_it_discards(self):
        """Todos are deleted with the workspace and nothing else holds them, so a silent
        removal is the one case where the developer learns what was lost afterwards."""
        todos.add("alpha", "prove the live gh path")
        todos.add("alpha", "rewrite the status line frame")
        rc, out = self.remove()
        self.assertEqual(rc, 0)
        self.assertIn("2", out)
        self.assertIn("todo", out)

    def test_removing_a_workspace_with_no_todos_reports_nothing_extra(self):
        """An empty list is not news. Reporting "0 open todos" on every removal is how a
        message stops being read at all, including on the removal that mattered."""
        _, out = self.remove()
        self.assertNotIn("todo", out)


class TestTodosDoNotBlockRemoval(RemoveCase):
    def test_todos_alone_do_not_require_force(self):
        todos.add("alpha", "prove the live gh path")
        rc, _ = self.remove()
        self.assertEqual(rc, 0)

    def test_the_workspace_is_actually_gone(self):
        """Reporting the todos and then declining to remove would be the same failure as
        blocking outright — the guard is what it does, not what it prints."""
        todos.add("alpha", "prove the live gh path")
        self.remove()
        self.assertFalse(workspace.workspace_dir("alpha").exists())

    def test_todos_are_not_in_the_work_at_risk_set(self):
        """Asserted against the guard itself, not just its outcome: folding todos in here
        is the overreach the ticket exists to prevent, and it would be invisible in a
        workspace that has real work at risk anyway."""
        todos.add("alpha", "prove the live gh path")
        self.assertEqual(cw._work_at_risk("alpha"), [])


class TestWorkAtRiskStillBlocks(RemoveCase):
    def test_uncommitted_work_still_refuses_removal(self):
        self.make_dirty_clone()
        rc, _ = self.remove()
        self.assertNotEqual(rc, 0)
        self.assertTrue(workspace.workspace_dir("alpha").exists())

    def test_uncommitted_work_refuses_even_with_todos_present(self):
        """The todo count must not become an escape hatch: "it only had todos" is a
        judgement about the todos, never about the commits beside them."""
        todos.add("alpha", "prove the live gh path")
        self.make_dirty_clone()
        rc, _ = self.remove()
        self.assertNotEqual(rc, 0)
        self.assertTrue(workspace.workspace_dir("alpha").exists())

    def test_force_still_removes_work_at_risk(self):
        self.make_dirty_clone()
        rc, _ = self.remove(force=True)
        self.assertEqual(rc, 0)
        self.assertFalse(workspace.workspace_dir("alpha").exists())


class TestWorktreesAreGuardedToo(RemoveCase):
    """#91 — the exclusion that keeps a worktree from being *counted* as a clone also kept
    it from being *guarded*, while `shutil.rmtree` took it anyway. `charter worktree remove`
    refuses to lose this work; `charter workspace remove` said `✓ Removed workspace`.

    The guard follows `worktree remove`'s stance rather than the clone rule beside it: a
    worktree is at risk when it holds commits reachable from **no other ref**. That
    difference is the whole point — a parallel agent's branch is unpushed almost by
    definition, so the clone rule would keep missing exactly the case this exists for.

    That rule started as "has no upstream" (#91) and was narrowed to unique commits (#104),
    because the first version also refused over freshly created pieces that had nothing to
    lose. Both guards moved together, and must continue to: they answer one question, and a
    workspace that removed what a worktree refused to would be the original bug again.
    """

    def test_uncommitted_work_in_a_worktree_refuses_removal(self):
        _, wt = self.make_clone_with_worktree()
        (wt / "unsaved.txt").write_text("an agent's work, never committed\n")
        rc, _ = self.remove()
        self.assertNotEqual(rc, 0)
        self.assertTrue(workspace.workspace_dir("alpha").exists())

    def test_commits_that_exist_nowhere_else_refuse_removal(self):
        """The fleet case. The tree is clean, so a dirt check alone passes it — but the
        branch has no upstream, so deleting the workspace is the only copy going away."""
        _, wt = self.make_clone_with_worktree()
        (wt / "feature.txt").write_text("committed, pushed nowhere\n")
        self.commit(wt, "work")
        rc, _ = self.remove()
        self.assertNotEqual(rc, 0)
        self.assertTrue(workspace.workspace_dir("alpha").exists())

    def test_the_piece_is_named_in_the_refusal(self):
        """A refusal that cannot say *which* piece is holding work sends the reader to
        check every worktree by hand — ADR 0009's complaint, in the guard rather than the
        error classifier."""
        _, wt = self.make_clone_with_worktree(piece="slice")
        (wt / "unsaved.txt").write_text("work\n")
        _, out = self.remove()
        self.assertIn("slice", out)

    def test_the_worktree_appears_in_the_work_at_risk_set(self):
        """Asserted against the guard itself, so a future refactor that reports worktrees
        without actually guarding them cannot pass."""
        _, wt = self.make_clone_with_worktree()
        (wt / "unsaved.txt").write_text("work\n")
        self.assertTrue(any("slice" in r for r in cw._work_at_risk("alpha")))

    def test_a_pushed_clean_worktree_does_not_block(self):
        """Specificity: the guard must protect work, not merely notice worktrees. A piece
        whose branch is clean and on a remote survives deletion of the workspace, so
        refusing over it would teach the `--force` habit the clone guard avoids."""
        clone, wt = self.make_clone_with_worktree()
        # Outside the workspace, so removal cannot take the remote with it.
        bare = workspace.workspace_dir("alpha").parent.parent / "origin.git"
        subprocess.run(["git", "init", "-q", "--bare", str(bare)], check=True,
                       capture_output=True, env={**os.environ, **_GIT_ENV})
        self.git(wt, "remote", "add", "origin", str(bare))
        self.git(wt, "push", "-q", "-u", "origin", "slice")
        rc, out = self.remove()
        self.assertEqual(rc, 0, out)
        self.assertFalse(workspace.workspace_dir("alpha").exists())

    def test_a_fresh_worktree_does_not_block(self):
        """The inversion #104 was filed for. This asserted the opposite when #91 landed:
        the rule was "has no upstream", which a just-created piece satisfies from the
        moment it exists — so removal refused over a worktree with nothing to lose.

        A guard that fires on the harmless common case is how `--force` becomes a habit,
        and fleets create worktrees constantly, so the harmless case was about to become
        the usual one. The rule is now "commits reachable from no other ref", which a fresh
        piece scores zero on because its base's branch still reaches its tip.
        """
        self.make_clone_with_worktree()
        rc, out = self.remove()
        self.assertEqual(rc, 0, out)
        self.assertFalse(workspace.workspace_dir("alpha").exists())

    def test_work_that_landed_on_another_branch_does_not_block(self):
        """The rule is *reachable from somewhere else*, not *pushed*. A piece whose commits
        were merged into the clone's own branch is safe to delete with no remote involved
        at all — which the old no-upstream rule got wrong, and which is the ordinary end of
        a piece's life once its work is integrated.
        """
        clone, wt = self.make_clone_with_worktree()
        (wt / "feature.txt").write_text("done and merged\n")
        self.commit(wt, "work")
        self.git(clone, "merge", "--no-edit", "-q", "slice")
        rc, out = self.remove()
        self.assertEqual(rc, 0, out)
        self.assertFalse(workspace.workspace_dir("alpha").exists())

    def test_force_still_removes_a_worktree_holding_work(self):
        _, wt = self.make_clone_with_worktree()
        (wt / "unsaved.txt").write_text("work\n")
        rc, _ = self.remove(force=True)
        self.assertEqual(rc, 0)
        self.assertFalse(workspace.workspace_dir("alpha").exists())


if __name__ == "__main__":
    unittest.main()
