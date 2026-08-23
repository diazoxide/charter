"""One scan of the plane's repo state, cached under the frame's own directory.

``gather.scan()`` is ``statusline.render``'s pre-layout gather (``_repo_trees``,
``_repo_states``, ``_branch``, ``_current``, ``glstate.read_for``), reused rather
than reimplemented so a fix to a repo row's git-status logic lands in both
surfaces at once. ``save``/``read`` are the cache's write/read pair a hook (Task
2) and a panel (Tasks 3/4) will use; ``refresh`` is the two composed.

Real git fixtures for the data-correctness tests (the
``tests/test_statusline_worktree_rows.py`` pattern) — a mocked git would prove
nothing about whether ``scan()`` actually reads what ``_repo_states``/``_branch``
read. The cache-behaviour tests (round-trip, corrupt, missing) mock ``scan()``
itself instead, so they pin the CACHING contract independent of whatever git
happens to be installed, and independent of the data-correctness tests above.
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import config
from charter.frame import gather, state

from tests._isolation import PersonaIso


def _git(repo: Path, *args: str) -> None:
    subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)


def _init_repo(path: Path, branch: str = "main") -> Path:
    """A real git repo with one commit — the same fixture shape
    ``tests/test_statusline_worktree_rows.py`` uses, so a repo ``scan()`` reads
    and a repo ``statusline.render()`` reads are built identically."""
    path.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    _git(path, "config", "user.email", "t@example.com")
    _git(path, "config", "user.name", "t")
    _git(path, "config", "gc.auto", "0")
    _git(path, "config", "maintenance.auto", "false")
    (path / "README.md").write_text("hello\n")
    _git(path, "add", "README.md")
    _git(path, "-c", "commit.gpgsign=false", "commit", "-qm", "init")
    return path


class ClonedRepoIso(PersonaIso):
    """A plane with one repo cloned into the active workspace, real git in it."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.repo = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "demo"
        _init_repo(self.repo)


class ScanReusesStatuslineHelpers(ClonedRepoIso, unittest.TestCase):
    """Pins that `scan()`'s fields are the REAL values `_repo_states`/`_branch`
    read, not placeholders. A field that always reported the same thing whatever
    the repo's actual state is exactly the "vacuous test" failure mode named in
    this task's brief — except here it would be a vacuous *production field*, and
    these tests exist to make that impossible to ship unnoticed."""

    def test_lists_the_workspace_clone_with_its_real_branch(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual([r["name"] for r in data["repos"]], ["demo"])
        self.assertEqual(data["repos"][0]["branch"], "main")

    def test_a_clean_repo_is_not_dirty(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertFalse(data["repos"][0]["dirty"])

    def test_an_untracked_file_marks_the_repo_dirty(self):
        (self.repo / "scratch.txt").write_text("x\n")
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertTrue(data["repos"][0]["dirty"])

    def test_standing_inside_the_repo_marks_it_current(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        self.assertTrue(data["repos"][0]["current"])
        self.assertEqual(data["current_repo"], "demo")

    def test_standing_at_the_plane_root_does_not_mark_it_current(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertFalse(data["repos"][0]["current"])
        self.assertIsNone(data["current_repo"])


class ScanWithNoRepos(PersonaIso, unittest.TestCase):
    def test_an_empty_workspace_scans_to_an_empty_repo_list_without_raising(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual(data["repos"], [])
        self.assertEqual(data["worktrees"], [])


class ScanNeverRaises(PersonaIso, unittest.TestCase):
    """Every step in `scan()` is wrapped individually — Task 2 calls this from a
    hook, where "a hook may cost a session its briefing, never its turn"."""

    def test_a_broken_workspace_resolve_does_not_raise(self):
        with mock.patch("charter.workspace.resolve", side_effect=RuntimeError("boom")):
            data = gather.scan(cwd=str(self.tmp))  # must not raise
        self.assertEqual(data["repos"], [])

    def test_a_broken_repo_trees_lookup_does_not_raise(self):
        with mock.patch("charter.statusline._repo_trees", side_effect=RuntimeError("boom")):
            data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual(data["repos"], [])

    def test_a_broken_glstate_read_does_not_raise(self):
        with mock.patch("charter.glstate.read_for", side_effect=RuntimeError("boom")):
            data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        self.assertEqual(data["workspace"], config.DEFAULT_WORKSPACE)


class RoundTrip(PersonaIso, unittest.TestCase):
    def test_a_saved_scan_comes_back_unchanged_without_gathering_again(self):
        """The strong form of "round-trips": not merely that `read()` returns
        equal data, but that it comes from the FILE — `scan()` is mocked to
        raise, so a `read()` that silently re-gathered instead of trusting a
        valid cache would fail this test, not just return a different value."""
        data = {"gathered_at": 1.0, "workspace": "w", "current_repo": "demo",
                "repos": [{"name": "demo", "branch": "main", "dirty": False,
                           "tracked_dirty": False, "ahead": 0, "behind": 1,
                           "ci": "success", "change": 3, "sigil": "!",
                           "current": True}],
                "worktrees": []}
        gather.save("f-1", data)
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("must not gather again")):
            got = gather.read("f-1")
        self.assertEqual(got, data)


class CorruptCache(PersonaIso, unittest.TestCase):
    def test_a_corrupt_cache_degrades_to_a_fresh_gather(self):
        d = state.frame_dir("f-1", create=True)
        (d / "gather.json").write_text("{not valid json")
        sentinel = {"gathered_at": 0.0, "workspace": "sentinel", "current_repo": None,
                    "repos": [], "worktrees": []}
        with mock.patch.object(gather, "scan", return_value=sentinel) as scan_mock:
            got = gather.read("f-1")
        scan_mock.assert_called_once()
        self.assertEqual(got, sentinel)


class MissingCache(PersonaIso, unittest.TestCase):
    def test_a_never_bumped_frame_is_not_an_error(self):
        sentinel = {"gathered_at": 0.0, "workspace": "sentinel", "current_repo": None,
                    "repos": [], "worktrees": []}
        self.assertFalse(state.frame_dir("never-bumped").exists())
        with mock.patch.object(gather, "scan", return_value=sentinel) as scan_mock:
            got = gather.read("never-bumped")
        scan_mock.assert_called_once()
        self.assertEqual(got, sentinel)

    def test_a_hostile_fid_does_not_raise(self):
        gather.read("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())


class SaveIsHardened(PersonaIso, unittest.TestCase):
    def test_saving_against_a_hostile_fid_does_not_raise_or_create(self):
        gather.save("../../escaped", {"repos": []})  # must not raise
        self.assertFalse(state._root().exists())


class RefreshComposesScanAndSave(ClonedRepoIso, unittest.TestCase):
    def test_refresh_writes_a_cache_read_can_then_find_without_gathering_again(self):
        data = gather.refresh("f-1", workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        self.assertEqual(data["repos"][0]["name"], "demo")
        cache_file = state.frame_dir("f-1") / "gather.json"
        self.assertTrue(cache_file.exists())
        with mock.patch.object(gather, "scan",
                               side_effect=AssertionError("must not gather again")):
            self.assertEqual(gather.read("f-1"), data)


if __name__ == "__main__":
    unittest.main()
