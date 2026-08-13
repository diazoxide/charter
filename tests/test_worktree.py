"""Worktrees of a workspace's clone. Git is the only registry, so these run against
REAL git (the tests/test_git_policy.py pattern) — a mocked git would prove nothing."""
from __future__ import annotations

import io
import shutil
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from tests._isolation import PersonaIso
from charter import worktree


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


class WorktreeIso(PersonaIso):
    """A workspace containing one real git clone with a single commit on `main`."""

    def setUp(self) -> None:
        super().setUp()
        # `cmd_worktree_*` prints progress via `util.ok`/`util.info` (stderr) and some
        # commands print to stdout too — route both to a throwaway buffer by default so
        # they don't leak onto the real test-run output. A test that needs to inspect the
        # output enters its own nested redirect, which captures correctly (these nest).
        self.enterContext(redirect_stdout(io.StringIO()))
        self.enterContext(redirect_stderr(io.StringIO()))
        self.ws = "demo"
        self.clone = self.tmp / "workspaces" / self.ws / "iam-service"
        self.clone.mkdir(parents=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.clone)],
                       check=True, capture_output=True)
        git(self.clone, "config", "user.email", "t@example.com")
        git(self.clone, "config", "user.name", "t")
        (self.clone / "README.md").write_text("hello\n")
        git(self.clone, "add", "README.md")
        git(self.clone, "-c", "commit.gpgsign=false", "commit", "-qm", "init")

    def add_worktree(self, piece: str) -> Path:
        """Create a worktree with PLAIN git, at the path edm would use."""
        p = worktree.path_for(self.ws, self.clone.name, piece)
        p.parent.mkdir(parents=True, exist_ok=True)
        git(self.clone, "worktree", "add", "-q", str(p), "-b", piece)
        return p

    def _add_upstream(self) -> Path:
        """A bare repo standing in for a remote — no network needed, and it gives
        `main` a real `@{u}` to count against. Shared across test classes: a worktree
        created off this clone shares its remote config (worktrees split HEAD/index but
        not remotes), so a worktree branch just needs its own `push -u` on top of this."""
        bare = self.tmp / "remote.git"
        subprocess.run(["git", "init", "-q", "--bare", str(bare)],
                       check=True, capture_output=True)
        git(self.clone, "remote", "add", "origin", str(bare))
        git(self.clone, "push", "-q", "-u", "origin", "main")
        return bare


class TestPaths(WorktreeIso):
    def test_path_is_workspace_level_not_inside_the_clone(self):
        p = worktree.path_for(self.ws, "iam-service", "spi-schema")
        self.assertEqual(p, self.tmp / "workspaces" / self.ws / ".worktrees"
                         / "iam-service" / "spi-schema")
        self.assertNotIn(str(self.clone), str(p))


class TestPorcelain(WorktreeIso):
    def test_parses_branch_and_detached(self):
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/heads/main\n\n"
            "worktree /b\nHEAD def\ndetached\n\n")
        self.assertEqual(rows[0]["path"], "/a")
        self.assertEqual(rows[0]["branch"], "main")
        self.assertFalse(rows[0]["detached"])
        self.assertIsNone(rows[1]["branch"])
        self.assertTrue(rows[1]["detached"])

    def test_slashed_branch_is_not_truncated(self):
        """Branch names legitimately contain slashes — only the `refs/heads/` prefix
        is stripped, not everything up to the last slash."""
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/heads/feature/spi-schema\n\n")
        self.assertEqual(rows[0]["branch"], "feature/spi-schema")

    def test_a_ref_without_the_refs_heads_prefix_is_left_as_is(self):
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/tags/v1.0\n\n")
        self.assertEqual(rows[0]["branch"], "refs/tags/v1.0")

    def test_a_worktree_made_by_plain_git_is_listed(self):
        """The registry is git itself — nothing edm wrote."""
        self.add_worktree("spi-schema")
        rows = worktree.list_for(self.clone, self.ws)
        self.assertEqual([r["piece"] for r in rows], ["spi-schema"])
        self.assertEqual(rows[0]["branch"], "spi-schema")

    def test_the_clone_itself_is_not_listed_as_a_worktree(self):
        self.add_worktree("spi-schema")
        paths = [r["path"] for r in worktree.list_for(self.clone, self.ws)]
        self.assertNotIn(str(self.clone), paths)

    def test_a_normal_worktree_is_not_prunable(self):
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/heads/main\n\n")
        self.assertFalse(rows[0]["prunable"])

    def test_a_bare_prunable_line_is_flagged(self):
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/heads/main\nprunable\n\n")
        self.assertTrue(rows[0]["prunable"])

    def test_a_prunable_line_with_a_reason_carries_the_reason(self):
        """Deleting a worktree dir without `git worktree prune` leaves this exact line —
        verified against real git 2.50.1 output, not just the docs."""
        rows = worktree.parse_porcelain(
            "worktree /a\nHEAD abc\nbranch refs/heads/main\n"
            "prunable gitdir file points to non-existent location\n\n")
        self.assertEqual(rows[0]["prunable"],
                         "gitdir file points to non-existent location")


class TestGitState(WorktreeIso):
    def test_head_of_reports_branch(self):
        self.assertEqual(worktree.head_of(self.clone), ("main", False))

    def test_head_of_reports_detached(self):
        sha = git(self.clone, "rev-parse", "HEAD").stdout.strip()
        git(self.clone, "checkout", "-q", sha)
        label, detached = worktree.head_of(self.clone)
        self.assertTrue(detached)
        self.assertTrue(sha.startswith(label))

    def test_is_dirty(self):
        self.assertFalse(worktree.is_dirty(self.clone))
        (self.clone / "README.md").write_text("changed\n")
        self.assertTrue(worktree.is_dirty(self.clone))

    def test_unpushed_is_none_without_upstream(self):
        """No upstream means the commits exist nowhere else — the conservative case."""
        self.assertIsNone(worktree.unpushed(self.clone))

    def test_unpushed_is_zero_with_upstream_and_no_new_commits(self):
        self._add_upstream()
        self.assertEqual(worktree.unpushed(self.clone), 0)

    def test_unpushed_counts_commits_ahead_of_upstream(self):
        self._add_upstream()
        for i in range(2):
            (self.clone / f"file{i}.txt").write_text("x\n")
            git(self.clone, "add", f"file{i}.txt")
            git(self.clone, "-c", "commit.gpgsign=false", "commit", "-qm", f"commit {i}")
        self.assertEqual(worktree.unpushed(self.clone), 2)

    def test_branch_exists(self):
        self.assertTrue(worktree.branch_exists(self.clone, "main"))
        self.assertFalse(worktree.branch_exists(self.clone, "nope"))


class TestCloneVsWorktree(WorktreeIso):
    def test_is_clone_distinguishes_by_dot_git_kind(self):
        from charter import workspace
        wt = self.add_worktree("spi-schema")
        self.assertTrue((self.clone / ".git").is_dir())
        self.assertTrue((wt / ".git").is_file())    # git's own discriminator
        self.assertTrue(workspace.is_clone(self.clone))
        self.assertFalse(workspace.is_clone(wt))

    def test_clones_does_not_count_a_worktree(self):
        """A worktree parked at a workspace's top level must not show up as a repo."""
        from charter import workspace
        stray = self.tmp / "workspaces" / self.ws / "stray-piece"
        git(self.clone, "worktree", "add", "-q", str(stray), "-b", "stray")
        names = [c.name for c in workspace.clones(self.ws)]
        self.assertIn("iam-service", names)
        self.assertNotIn("stray-piece", names)


class _Args:
    """argparse.Namespace stand-in — only the attributes the handlers read."""
    def __init__(self, **kw):
        self.workspace = None
        self.repo = None
        self.piece = None
        self.branch = None
        self.force = False
        self.delete_branch = False
        self.__dict__.update(kw)


class TestAdd(WorktreeIso):
    def setUp(self) -> None:
        super().setUp()
        from charter import commands_worktree
        self.cw = commands_worktree

    def test_add_creates_the_worktree_on_a_new_branch_off_head(self):
        rc = self.cw.cmd_worktree_add(_Args(workspace=self.ws, repo="iam-service",
                                            piece="spi-schema"))
        self.assertEqual(rc, 0)
        p = worktree.path_for(self.ws, "iam-service", "spi-schema")
        self.assertTrue((p / "README.md").exists())
        self.assertEqual(worktree.head_of(p), ("spi-schema", False))
        # branched off the clone's HEAD, so it carries the clone's commit
        self.assertEqual(git(p, "rev-parse", "HEAD").stdout,
                         git(self.clone, "rev-parse", "main").stdout)

    def test_add_refuses_an_uncloned_repo(self):
        self.assertEqual(self.cw.cmd_worktree_add(
            _Args(workspace=self.ws, repo="nope", piece="x")), 1)

    def test_add_refuses_an_invalid_piece_name(self):
        self.assertEqual(self.cw.cmd_worktree_add(
            _Args(workspace=self.ws, repo="iam-service", piece="../escape")), 1)

    def test_add_refuses_an_existing_branch_without_dash_dash_branch(self):
        git(self.clone, "branch", "taken")
        self.assertEqual(self.cw.cmd_worktree_add(
            _Args(workspace=self.ws, repo="iam-service", piece="taken")), 1)

    def test_add_reuses_an_existing_branch_with_dash_dash_branch(self):
        git(self.clone, "branch", "taken")
        rc = self.cw.cmd_worktree_add(_Args(workspace=self.ws, repo="iam-service",
                                            piece="slice", branch="taken"))
        self.assertEqual(rc, 0)
        p = worktree.path_for(self.ws, "iam-service", "slice")
        self.assertEqual(worktree.head_of(p), ("taken", False))

    def test_add_refuses_a_duplicate_piece(self):
        """Refused with `CLAIM_TAKEN`, not the generic 1 this asserted before #96. A
        duplicate piece IS a lost claim, and the code is what lets a worker take the next
        name from its plan without reading the message."""
        a = _Args(workspace=self.ws, repo="iam-service", piece="spi-schema")
        self.assertEqual(self.cw.cmd_worktree_add(a), 0)
        self.assertEqual(self.cw.cmd_worktree_add(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema")),
            self.cw.CLAIM_TAKEN)

    def test_add_never_writes_inside_the_clone(self):
        before = sorted(p.name for p in self.clone.iterdir())
        self.cw.cmd_worktree_add(_Args(workspace=self.ws, repo="iam-service",
                                       piece="spi-schema"))
        self.assertEqual(sorted(p.name for p in self.clone.iterdir()), before)

    @staticmethod
    def _run(args) -> tuple[int, str, str]:
        """Same rationale as TestList._run: util.info/ok/warn write to stderr, so both
        streams must be captured to see everything the handler prints."""
        from charter import commands_worktree
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_worktree.cmd_worktree_add(args)
        return rc, out.getvalue(), err.getvalue()

    def test_add_prints_the_base_it_used(self):
        """The mitigation for the spec's "surprising base" risk starts with telling the
        user what base a new piece actually branched off — pin that it reaches output."""
        rc, out, err = self._run(_Args(workspace=self.ws, repo="iam-service",
                                       piece="spi-schema"))
        self.assertEqual(rc, 0)
        combined = out + err
        self.assertIn("base:", combined)
        self.assertIn("main", combined)

    def test_add_warns_when_the_clone_is_dirty(self):
        """Uncommitted changes in the clone are NOT carried into the new worktree —
        `add` must warn about that surprising-base risk, not silently proceed."""
        (self.clone / "README.md").write_text("uncommitted change\n")
        rc, out, err = self._run(_Args(workspace=self.ws, repo="iam-service",
                                       piece="spi-schema"))
        self.assertEqual(rc, 0)
        self.assertIn("uncommitted changes", (out + err).lower())

    def test_add_warns_when_the_clone_is_on_a_detached_head(self):
        """A piece branched off a detached HEAD is branched off a raw commit, not a
        branch — another surprising-base risk `add` must call out."""
        sha = git(self.clone, "rev-parse", "HEAD").stdout.strip()
        git(self.clone, "checkout", "-q", sha)
        rc, out, err = self._run(_Args(workspace=self.ws, repo="iam-service",
                                       piece="spi-schema"))
        self.assertEqual(rc, 0)
        self.assertIn("detached head", (out + err).lower())


class TestList(WorktreeIso):
    def setUp(self) -> None:
        super().setUp()
        from charter import commands_worktree
        self.cw = commands_worktree

    def test_list_with_no_repo_covers_every_clone(self):
        self.add_worktree("spi-schema")
        self.assertEqual(self.cw.cmd_worktree_list(_Args(workspace=self.ws, repo=None)), 0)

    def test_list_reports_a_worktree_made_by_plain_git(self):
        """Git is the registry: edm never recorded this one."""
        self.add_worktree("spi-schema")
        rows = worktree.list_for(self.clone, self.ws)
        self.assertEqual([(r["piece"], r["branch"]) for r in rows],
                         [("spi-schema", "spi-schema")])

    def test_list_is_empty_when_none_exist(self):
        self.assertEqual(worktree.list_for(self.clone, self.ws), [])
        self.assertEqual(self.cw.cmd_worktree_list(_Args(workspace=self.ws, repo=None)), 0)

    def test_list_refuses_an_uncloned_repo(self):
        self.assertEqual(self.cw.cmd_worktree_list(
            _Args(workspace=self.ws, repo="nope")), 1)

    @staticmethod
    def _run(args) -> tuple[int, str, str]:
        """Call the handler with real stdout/stderr captured — util.info/ok/warn/err
        write to stderr (verified against edm/util.py), plain print() (the actual
        table rows) goes to stdout, so both must be captured to see everything the
        handler prints."""
        from charter import commands_worktree
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_worktree.cmd_worktree_list(args)
        return rc, out.getvalue(), err.getvalue()

    def test_list_prints_the_piece_name_and_branch(self):
        """The piece name and its branch (deliberately different names here) must both
        actually reach printed output — asserting only against worktree.list_for()
        would bypass the handler entirely and miss a broken/empty print."""
        p = worktree.path_for(self.ws, self.clone.name, "spi-schema")
        p.parent.mkdir(parents=True, exist_ok=True)
        git(self.clone, "worktree", "add", "-q", str(p), "-b", "feature/spi-schema")

        rc, out, _err = self._run(_Args(workspace=self.ws, repo=None))
        self.assertEqual(rc, 0)
        self.assertIn("spi-schema", out)
        self.assertIn("feature/spi-schema", out)

    def test_list_prints_no_worktrees_message_when_a_clone_has_none(self):
        rc, out, err = self._run(_Args(workspace=self.ws, repo=None))
        self.assertEqual(rc, 0)
        self.assertIn("No worktrees", out + err)

    def test_a_pruned_worktree_is_shown_as_missing_not_clean(self):
        """Reproduces the review finding: deleting a worktree dir directly (without
        `git worktree prune`) leaves its record in `git worktree list --porcelain`
        with a `prunable` line. The old code ignored that line, then ran
        `git -C <missing-path> status --porcelain` — which exits 128 with EMPTY
        stdout (the error goes to stderr) — so `bool("")` is False and the row
        printed as "clean". It must print a distinct "missing" state instead, and
        must not silently call is_dirty() on the missing path."""
        p = self.add_worktree("spi-schema")
        self.assertTrue(p.exists())
        shutil.rmtree(p)  # delete the worktree dir WITHOUT `git worktree prune`
        self.assertFalse(p.exists())

        rc, out, _err = self._run(_Args(workspace=self.ws, repo=None))
        self.assertEqual(rc, 0)
        line = next(l for l in out.splitlines() if "spi-schema" in l)
        self.assertIn("missing", line)
        self.assertNotIn("clean", line)


class TestRemove(WorktreeIso):
    def setUp(self) -> None:
        super().setUp()
        from charter import commands_worktree
        self.cw = commands_worktree
        self.cw.cmd_worktree_add(_Args(workspace=self.ws, repo="iam-service",
                                       piece="spi-schema"))
        self.wt = worktree.path_for(self.ws, "iam-service", "spi-schema")

    def test_remove_refuses_a_dirty_worktree(self):
        (self.wt / "README.md").write_text("work in progress\n")
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema")), 1)
        self.assertTrue(self.wt.exists())

    def test_force_removes_a_dirty_worktree(self):
        (self.wt / "README.md").write_text("work in progress\n")
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema",
                  force=True)), 0)
        self.assertFalse(self.wt.exists())

    def test_remove_refuses_commits_that_exist_nowhere_else(self):
        """No upstream => unpushed. Deleting would destroy the only copy."""
        (self.wt / "new.txt").write_text("x\n")
        git(self.wt, "add", "new.txt")
        git(self.wt, "-c", "commit.gpgsign=false", "commit", "-qm", "piece work")
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema")), 1)
        self.assertTrue(self.wt.exists())

    @staticmethod
    def _run(args) -> tuple[int, str, str]:
        """Same rationale as TestList._run: util.err/info write to stderr, so both
        streams must be captured to see the refusal message."""
        from charter import commands_worktree
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_worktree.cmd_worktree_remove(args)
        return rc, out.getvalue(), err.getvalue()

    def test_remove_refuses_commits_ahead_of_a_real_upstream(self):
        """The third refusal branch — unpushed() returns a positive int because the
        branch HAS an upstream but is ahead of it — was previously only exercised at
        the worktree.unpushed() unit level, never through the handler. A regression
        that dropped this `if ahead:` branch (as opposed to the `ahead is None` one)
        would silently allow destroying pushed-branch work that's still ahead of what
        was actually pushed."""
        self._add_upstream()  # wires "origin" for the whole repo (shared across worktrees)
        git(self.wt, "push", "-q", "-u", "origin", "spi-schema")  # 0 ahead so far
        (self.wt / "new.txt").write_text("x\n")
        git(self.wt, "add", "new.txt")
        git(self.wt, "-c", "commit.gpgsign=false", "commit", "-qm", "piece work")
        self.assertEqual(worktree.unpushed(self.wt), 1)  # sanity: genuinely ahead

        rc, out, err = self._run(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema"))
        self.assertEqual(rc, 1)
        self.assertTrue(self.wt.exists())
        self.assertIn("unpushed", (out + err).lower())

    def test_delete_branch_drops_the_branch_the_worktree_is_actually_on(self):
        """`remove` deliberately deletes the branch the worktree was CHECKED OUT ON
        (captured via worktree.head_of(path) before removal), not the piece name — a
        worktree made with `--branch feature/x` sits on feature/x, not on its piece
        name. Only verified manually before; pin it here."""
        git(self.clone, "branch", "feature/x")
        self.assertEqual(self.cw.cmd_worktree_add(
            _Args(workspace=self.ws, repo="iam-service", piece="other-piece",
                  branch="feature/x")), 0)
        # No branch named after the piece was ever created.
        self.assertFalse(worktree.branch_exists(self.clone, "other-piece"))

        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="other-piece",
                  force=True, delete_branch=True)), 0)
        self.assertFalse(worktree.branch_exists(self.clone, "feature/x"))
        self.assertFalse(worktree.branch_exists(self.clone, "other-piece"))

    def test_delete_branch_on_a_detached_worktree_warns_instead_of_erroring(self):
        """A worktree parked on a raw commit SHA (detached HEAD) has no branch to
        delete — `--delete-branch` must not attempt a nonsensical `git branch -D
        <sha>`; the handler warns and still returns 0."""
        sha = git(self.clone, "rev-parse", "HEAD").stdout.strip()
        p = worktree.path_for(self.ws, "iam-service", "detached-piece")
        p.parent.mkdir(parents=True, exist_ok=True)
        git(self.clone, "worktree", "add", "-q", "--detach", str(p), sha)
        self.assertEqual(worktree.head_of(p)[1], True)  # sanity: really detached

        rc, out, err = self._run(
            _Args(workspace=self.ws, repo="iam-service", piece="detached-piece",
                  force=True, delete_branch=True))
        self.assertEqual(rc, 0)
        self.assertFalse(p.exists())
        self.assertIn("detached", (out + err).lower())

    def test_remove_leaves_the_branch_by_default(self):
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema",
                  force=True)), 0)
        self.assertTrue(worktree.branch_exists(self.clone, "spi-schema"))

    def test_delete_branch_drops_it(self):
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema",
                  force=True, delete_branch=True)), 0)
        self.assertFalse(worktree.branch_exists(self.clone, "spi-schema"))

    def test_remove_refuses_an_unknown_piece(self):
        self.assertEqual(self.cw.cmd_worktree_remove(
            _Args(workspace=self.ws, repo="iam-service", piece="ghost")), 1)

    def test_removed_worktree_disappears_from_list(self):
        self.cw.cmd_worktree_remove(_Args(workspace=self.ws, repo="iam-service",
                                          piece="spi-schema", force=True))
        self.assertEqual(worktree.list_for(self.clone, self.ws), [])

    def test_remove_clears_a_missing_prunable_worktree(self):
        """F1: a worktree dir deleted without `git worktree prune` must still be
        removable. The old code checked `path.exists()` before anything else and
        errored out; plain `git worktree remove <path>` (no `--force`) actually
        succeeds on a prunable registration since there's no working tree left to
        lose — and the tree-safety checks must be skipped entirely, since
        `unpushed()` on a missing path returns `None` (no upstream here), which
        would otherwise trigger the "no upstream" refusal spuriously."""
        shutil.rmtree(self.wt)  # delete directly, without `git worktree prune`
        self.assertFalse(self.wt.exists())
        rows = worktree.list_for(self.clone, self.ws)
        self.assertTrue(rows[0]["prunable"])  # sanity: git still has the record

        rc, out, err = self._run(
            _Args(workspace=self.ws, repo="iam-service", piece="spi-schema"))
        self.assertEqual(rc, 0)
        self.assertEqual(worktree.list_for(self.clone, self.ws), [])  # registration cleared
        self.assertIn("stale", (out + err).lower())

    def test_remove_still_refuses_a_piece_that_never_existed(self):
        """The missing/prunable fast-path must not swallow the genuine "never
        registered" case."""
        rc, out, err = self._run(
            _Args(workspace=self.ws, repo="iam-service", piece="ghost"))
        self.assertEqual(rc, 1)
        self.assertIn("no worktree", (out + err).lower())


class TestDirsFor(WorktreeIso):
    def test_returns_empty_for_a_repo_with_no_worktrees(self):
        self.assertEqual(worktree.dirs_for(self.ws, "iam-service"), [])
        self.assertEqual(worktree.dirs_for(self.ws, "no-such-repo"), [])

    def test_orders_newest_first(self):
        import os
        a = self.add_worktree("older")
        b = self.add_worktree("newer")
        os.utime(a, (1_000_000, 1_000_000))
        os.utime(b, (2_000_000, 2_000_000))
        self.assertEqual([p.name for p in worktree.dirs_for(self.ws, "iam-service")],
                         ["newer", "older"])

    def test_costs_no_subprocess(self):
        """The status line renders every turn — this read must not spawn git."""
        import subprocess
        from unittest import mock
        self.add_worktree("spi-schema")
        with mock.patch.object(subprocess, "run",
                               side_effect=AssertionError("spawned a process")):
            self.assertEqual([p.name for p in worktree.dirs_for(self.ws, "iam-service")],
                             ["spi-schema"])


class TestStatusline(WorktreeIso):
    def _rows(self):
        from charter import statusline
        return [n.render(120)[0] for n in statusline._repo_rows(
            [self.clone], self.ws, None, {self.clone: {}},
            {self.clone: "main"}, {})]

    def test_repo_row_is_unchanged_without_worktrees(self):
        rows = self._rows()
        self.assertEqual(len(rows), 1)
        self.assertIn("iam-service", rows[0])
        self.assertNotIn("⑂", rows[0])

    def test_count_badge_and_one_summary_line(self):
        for piece in ("alpha", "beta"):
            self.add_worktree(piece)
        rows = self._rows()
        self.assertIn("⑂2", rows[0])
        self.assertEqual(len(rows), 2)          # repo + ONE summary line, never more
        self.assertIn("alpha", rows[1])
        self.assertIn("beta", rows[1])
        self.assertIn("·", rows[1])             # pieces are dot-separated on one line

    def test_nested_rows_never_have_a_whitespace_only_prefix(self):
        """Claude Code's footer COLLAPSES a whitespace-only prefix to column 0, so a
        classic tree indent (spaces under the last repo) silently drops the worktree rows
        to the left margin and they stop reading as children. The tree line must stay a
        visible glyph. Regression guard for a bug that raw stdout could not reveal."""
        import re as _re
        for piece in ("a", "b"):
            self.add_worktree(piece)
        rows = self._rows()
        for row in rows[1:]:                       # row 0 is the repo itself
            plain = _re.sub(r"\033\[[0-9;]*m", "", row)
            prefix = _re.split(r"[├└↳]", plain)[0]
            self.assertTrue(prefix.strip(),
                            f"worktree row has a whitespace-only prefix: {plain!r}")

    def test_many_worktrees_still_cost_exactly_one_line(self):
        """The whole point of the summary line: the footer cannot grow with worktrees."""
        for piece in ("a", "b", "c", "d", "e"):
            self.add_worktree(piece)
        rows = self._rows()
        self.assertIn("⑂5", rows[0])            # the badge carries the TRUE total
        self.assertEqual(len(rows), 2)          # repo + one line, same as with 2 pieces

    def test_overflowing_pieces_truncate_rather_than_vanish_silently(self):
        for piece in ("piece-with-a-long-name-%02d" % i for i in range(12)):
            self.add_worktree(piece)
        rows = self._rows()
        self.assertEqual(len(rows), 2)
        self.assertIn("…", rows[1])             # visibly cut, not quietly dropped


class TestStatuslineRowBudget(WorktreeIso):
    """F2: `_repo_rows` must bound the TOTAL rows it returns (repo rows + nested
    worktree rows + the trailing summary line), not just the repo count. The old
    `_MAX_REPO_LINES` cap only limited how many repos got a row while nested worktree
    rows were appended inside the same loop uncapped in aggregate, so a workspace with
    several worktree-heavy repos could emit far more than 14 lines into the footer.

    Filesystem-only setup (no real git worktrees needed): `worktree.dirs_for`, which
    `_repo_rows` calls per repo, only lists directories under `.worktrees/<repo>/` —
    see its own docstring ("Filesystem-only — no subprocess").
    """

    def _fake_repo(self, name: str, n_worktrees: int) -> Path:
        d = self.tmp / "workspaces" / self.ws / name
        d.mkdir(parents=True)
        for i in range(n_worktrees):
            (self.tmp / "workspaces" / self.ws / ".worktrees" / name
             / f"piece{i}").mkdir(parents=True)
        return d

    def _rendered(self, dirs) -> list[str]:
        from charter import statusline
        states = {d: {} for d in dirs}
        branches = {d: "main" for d in dirs}
        rows = statusline._repo_rows(dirs, self.ws, None, states, branches, {})
        return [n.render(120)[0] for n in rows]

    def test_total_rows_never_exceed_the_cap(self):
        """Reproduces the review's own math: 14 repos * (1 + 3 worktree rows) would be
        56 lines under the old per-repo-only cap. The fix must bound the TOTAL at
        `_MAX_REPO_LINES` (14) regardless."""
        from charter import statusline
        dirs = [self._fake_repo(f"repo{i}", 3) for i in range(14)]
        rendered = self._rendered(dirs)
        self.assertLessEqual(len(rendered), statusline._MAX_REPO_LINES)

    def test_repos_are_never_hidden_by_an_earlier_repos_worktrees(self):
        """5 repos * 3 worktrees each = 20 lines uncapped — past the 14-row budget —
        but every repo must still get its own row; only the LATER repos' nested
        worktree rows get squeezed out by what's left of the budget."""
        from charter import statusline
        dirs = [self._fake_repo(f"repo{i}", 3) for i in range(5)]
        rendered = self._rendered(dirs)
        self.assertLessEqual(len(rendered), statusline._MAX_REPO_LINES)
        for i in range(5):
            self.assertTrue(any(f"repo{i}" in r for r in rendered),
                            f"repo{i} missing from rendered rows")


class TestListWorkspacesVsLegacyClones(WorktreeIso):
    """F5: `list_workspaces()`/`legacy_flat_clones()` must use `is_clone` (git DIR),
    not the broader `is_git_repo` (any `.git`, file or dir) — otherwise a worktree
    parked directly at `workspaces/<name>` (the workspace root itself is a linked
    worktree, not a real clone) makes `<name>` both vanish from the workspace list
    AND wrongly appear as a legacy flat clone."""

    def test_a_worktree_at_workspace_top_level_still_counts_as_a_workspace(self):
        from charter import workspace
        stray_name = "wt-parked-workspace"
        stray = self.tmp / "workspaces" / stray_name
        git(self.clone, "worktree", "add", "-q", str(stray), "-b", "stray-branch")
        self.assertTrue((stray / ".git").is_file())  # sanity: really a worktree, not a clone

        self.assertIn(stray_name, workspace.list_workspaces())
        self.assertNotIn(stray, workspace.legacy_flat_clones())


if __name__ == "__main__":
    unittest.main()
