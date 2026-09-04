"""A fork inherits the repos the workspace actually has, not only the ones it recorded
(issue #81).

charter has two answers to "which repos are in this workspace":

* `status` and `workspace list` **scan the directory** — always current, never portable;
* `workspace.json` is a **snapshot** — portable and committed, but written only by
  `charter workspace snapshot`, which refuses while a repo has unpushed work so that a
  recorded branch can actually be restored.

Nothing reconciled them and `fork` read the manifest alone. Reported live: a workspace
with nine clones on disk and no snapshot, where `status` and `list` both said `(9 cloned)`
and `fork` said *"No repos recorded"* and inherited nothing. Nine of ten workspaces on
that machine had no manifest at all, so the divergence was the normal case, hidden
everywhere a human looked by the directory scan.

The fix takes the union rather than picking a winner, because each source is the only one
that works in a case the other cannot reach — and the manifest wins on branch, since only
it was written under a guarantee that the branch was pushed.
"""
from __future__ import annotations

import io
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands_workspace as cw
from charter import config, workspace

# `GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` are neutralised, not just the identity: a
# developer with `commit.gpgsign = true` and `gpg.format = ssh` would otherwise have every
# fixture commit block on the signing agent — the test hangs rather than fails, which is
# the worst way for a fixture to be wrong. Same reason CI needs the identity: a fixture
# must not depend on the machine's git config.
_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e", "GIT_COMMITTER_NAME": "t",
        "GIT_COMMITTER_EMAIL": "t@e",
        "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}


def _git(*a, cwd):
    import os
    subprocess.run(["git", *a], cwd=cwd, check=True, capture_output=True,
                   env={**os.environ, **_ENV})


class TestTheMerge(unittest.TestCase):
    """The pure half — no git, no filesystem."""

    def merge(self, manifest, disk):
        return workspace.merge_repo_rows(manifest, disk)

    def test_clones_with_no_snapshot_are_inherited(self):
        """The reported bug, reduced: nine on disk, nothing recorded."""
        rows, disk_only = self.merge([], [{"name": "a", "branch": "main"}])
        self.assertEqual(rows, [{"name": "a", "branch": "main"}])
        self.assertEqual(disk_only, ["a"])

    def test_a_snapshot_with_no_clones_is_inherited(self):
        """The portable case, and why the disk cannot simply become the source of truth:
        a teammate who has just cloned the plane has no repos on disk at all."""
        rows, disk_only = self.merge([{"name": "a", "branch": "release"}], [])
        self.assertEqual(rows, [{"name": "a", "branch": "release"}])
        self.assertEqual(disk_only, [])

    def test_a_repo_known_to_both_appears_once(self):
        rows, _ = self.merge([{"name": "a", "branch": "release"}],
                             [{"name": "a", "branch": "scratch"}])
        self.assertEqual(len(rows), 1)

    def test_the_snapshot_branch_wins_over_whatever_is_checked_out(self):
        """`snapshot` refuses to record an unpushed branch, so its answer is restorable.
        The working tree's current branch carries no such promise."""
        rows, _ = self.merge([{"name": "a", "branch": "release"}],
                             [{"name": "a", "branch": "scratch"}])
        self.assertEqual(rows[0]["branch"], "release")

    def test_disjoint_sources_are_unioned_and_sorted(self):
        rows, disk_only = self.merge([{"name": "b", "branch": "main"}],
                                     [{"name": "a", "branch": "main"}])
        self.assertEqual([r["name"] for r in rows], ["a", "b"])
        self.assertEqual(disk_only, ["a"])

    def test_nothing_anywhere_is_empty(self):
        self.assertEqual(self.merge([], []), ([], []))

    def test_a_row_without_a_branch_still_produces_one(self):
        """`restore` clones by branch; a row with none would fail there instead of here."""
        rows, _ = self.merge([{"name": "a"}], [])
        self.assertEqual(rows[0]["branch"], "HEAD")

    def test_nameless_rows_are_dropped(self):
        rows, _ = self.merge([{"branch": "main"}, {"name": "  "}], [])
        self.assertEqual(rows, [])


class ForkCase(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-fork-"))
        self._orig = {k: getattr(config, k) for k in ("WORKSPACES_DIR", "ROOT")}
        config.WORKSPACES_DIR = self.tmp / "workspaces"
        config.ROOT = self.tmp
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._restore)

    def _restore(self):
        for k, v in self._orig.items():
            setattr(config, k, v)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _clone(self, ws: str, name: str, branch: str = "main"):
        """A real clone on disk — what `status` counts and what `fork` used to miss."""
        wd = workspace.workspace_dir(ws)
        wd.mkdir(parents=True, exist_ok=True)
        c = wd / name
        c.mkdir()
        _git("init", "-q", "-b", branch, ".", cwd=c)
        (c / "f.txt").write_text("x")
        _git("add", "-A", cwd=c)
        _git("commit", "-q", "-m", "init", cwd=c)
        return c

    def fork(self, src, new, **kw):
        out = io.StringIO()
        args = SimpleNamespace(src=src, new=new, restore=False, live=False, **kw)
        with redirect_stdout(out), redirect_stderr(out):
            rc = cw.cmd_workspace_fork(args)
        return rc, out.getvalue()


class TestForkingAnUnsnapshottedWorkspace(ForkCase):
    """The live report: nine clones, nothing recorded, `fork` inherits nothing."""

    def setUp(self):
        super().setUp()
        workspace.ensure("src")
        self._clone("src", "repoA")
        self._clone("src", "repoB")

    def test_the_premise_holds_the_manifest_records_no_repos(self):
        """The premise moved with #884 and the defect it describes did not.

        This used to read `read_manifest("src") == {}` — there was no file at all, because
        `snapshot` was the only thing that wrote one. Since #884 every workspace has a
        manifest from birth, and these clones were made by the fixture rather than by
        `charter clone`, so the manifest is present and its `repos` is empty. That is the
        same divergence #81 is about, and the same one `merge_repo_rows` closes: what the
        workspace HAS and what it RECORDED are two lists, and forking from the second alone
        inherits nothing.
        """
        self.assertEqual(workspace.read_manifest("src").get("repos"), [])
        self.assertEqual(len(workspace.clones("src")), 2)

    def test_the_fork_inherits_the_clones(self):
        rc, _ = self.fork("src", "dst")
        self.assertEqual(rc, 0)
        self.assertEqual([r["name"] for r in workspace.read_manifest("dst")["repos"]],
                         ["repoA", "repoB"])

    def test_it_no_longer_claims_no_repos_are_recorded(self):
        _, out = self.fork("src", "dst")
        self.assertNotIn("No repos", out)

    def test_it_says_they_came_from_disk_rather_than_a_snapshot(self):
        """Their branch is whatever is checked out, which `restore` on another machine
        may not find. Saying so at the point of use is the whole of the honesty here."""
        _, out = self.fork("src", "dst")
        self.assertIn("not from a snapshot", out)
        self.assertIn("snapshot src", out)


class TestForkingASnapshotWithNothingOnDisk(ForkCase):
    """A teammate who has just cloned the plane. The manifest is all they have."""

    def test_the_fork_inherits_the_snapshot(self):
        workspace.ensure("src")
        workspace.write_manifest("src", {"name": "src",
                                         "repos": [{"name": "repoA", "branch": "release"}]})
        rc, _ = self.fork("src", "dst")
        self.assertEqual(rc, 0)
        self.assertEqual(workspace.read_manifest("dst")["repos"],
                         [{"name": "repoA", "branch": "release"}])

    def test_nothing_is_attributed_to_disk(self):
        workspace.ensure("src")
        workspace.write_manifest("src", {"name": "src",
                                         "repos": [{"name": "repoA", "branch": "release"}]})
        _, out = self.fork("src", "dst")
        self.assertNotIn("not from a snapshot", out)


class TestForkingWithBothSources(ForkCase):
    def setUp(self):
        super().setUp()
        workspace.ensure("src")
        self._clone("src", "repoA", branch="scratch")
        self._clone("src", "repoB")
        workspace.write_manifest("src", {"name": "src",
                                         "repos": [{"name": "repoA", "branch": "release"},
                                                   {"name": "repoC", "branch": "main"}]})

    def test_every_repo_from_either_source_is_inherited(self):
        self.fork("src", "dst")
        self.assertEqual([r["name"] for r in workspace.read_manifest("dst")["repos"]],
                         ["repoA", "repoB", "repoC"])

    def test_the_snapshot_branch_wins_for_a_repo_in_both(self):
        self.fork("src", "dst")
        rows = {r["name"]: r["branch"] for r in workspace.read_manifest("dst")["repos"]}
        self.assertEqual(rows["repoA"], "release")

    def test_only_the_unsnapshotted_one_is_attributed_to_disk(self):
        _, out = self.fork("src", "dst")
        self.assertIn("repoB", out.split("come from clones on disk")[1])
        self.assertNotIn("repoC", out.split("come from clones on disk")[1].split("\n")[0])


class TestForkingATrulyEmptyWorkspace(ForkCase):
    def test_it_still_says_there_is_nothing(self):
        workspace.ensure("src")
        rc, out = self.fork("src", "dst")
        self.assertEqual(rc, 0)
        self.assertIn("No repos", out)

    def test_it_names_both_sources_it_checked(self):
        """The old message said 'No repos recorded', which was true of the manifest and
        read as true of the workspace. It has to name what it actually looked at."""
        workspace.ensure("src")
        _, out = self.fork("src", "dst")
        self.assertIn("snapshot", out)
        self.assertIn("disk", out)


if __name__ == "__main__":
    unittest.main()
