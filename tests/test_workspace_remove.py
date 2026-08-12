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

from charter import todos, workspace
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


if __name__ == "__main__":
    unittest.main()
