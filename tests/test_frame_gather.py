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

import json
import subprocess
import time
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


class WorktreeCountPerRepo(ClonedRepoIso, unittest.TestCase):
    """Fix round 1, finding 2 (#385): `_repo_rows` gets its `⑂N` badge from a
    live `worktree.dirs_for(active, d.name)` call PER REPO, independent of
    `_detail_worktrees` (single-repo-only). That per-repo count was never
    folded into `scan()`, so `repos[i]["worktree_count"]` did not exist at all
    — a multi-repo workspace's cache carried no piece information whatsoever.
    These pin the field is now REAL, not merely present, in exactly the shape
    (2+ repos) the gap was reported against."""

    def setUp(self) -> None:
        super().setUp()
        self.repo2 = config.WORKSPACES_DIR / config.DEFAULT_WORKSPACE / "second"
        _init_repo(self.repo2)

    def _add_piece(self, repo: Path, piece: str) -> Path:
        """A worktree DIRECTORY at the path charter's own registry uses —
        `worktree.dirs_for` is filesystem-only (`base.iterdir()`, no
        subprocess — see its own docstring), and with two repos
        `_detail_worktrees` never selects either for full detail rows (its own
        single-repo rule), so nothing in `scan()` ever reads THIS directory as
        a tree of its own; a bare directory is the honest fixture for a count
        that only cares that it exists."""
        from charter import worktree
        p = worktree.path_for(config.DEFAULT_WORKSPACE, repo.name, piece)
        p.mkdir(parents=True, exist_ok=True)
        return p

    def test_a_repo_with_no_pieces_counts_zero(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        by_name = {r["name"]: r for r in data["repos"]}
        self.assertEqual(by_name["demo"]["worktree_count"], 0)
        self.assertEqual(by_name["second"]["worktree_count"], 0)

    def test_a_multi_repo_workspaces_piece_count_is_real_per_repo(self):
        """The exact gap: with TWO repos, `detail_wts`/`data["worktrees"]` is
        always `[]` (`_detail_worktrees`'s single-repo rule) — before this fix
        round, that meant `demo`'s two real pieces were invisible to `left`
        entirely, not merely uncounted."""
        self._add_piece(self.repo, "piece-a")
        self._add_piece(self.repo, "piece-b")
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        by_name = {r["name"]: r for r in data["repos"]}
        self.assertEqual(by_name["demo"]["worktree_count"], 2)
        self.assertEqual(by_name["second"]["worktree_count"], 0)
        # The gap this pins: `detail_wts` truly is empty here, so the count is
        # the ONLY place this fact survives into the cache at all.
        self.assertEqual(data["worktrees"], [])

    def test_a_broken_worktree_lookup_does_not_raise_and_zeroes_the_count(self):
        with mock.patch("charter.worktree.dirs_for", side_effect=RuntimeError("boom")):
            data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.tmp))
        by_name = {r["name"]: r for r in data["repos"]}
        self.assertEqual(by_name["demo"]["worktree_count"], 0)
        self.assertEqual(by_name["second"]["worktree_count"], 0)


class DivergedRepoIso(ClonedRepoIso):
    """`ClonedRepoIso`'s repo, but with real ahead/behind AND an upstream to
    diverge from — the `tests/test_worktree.py` `_add_upstream` pattern (a bare
    repo standing in for a remote, no network needed), extended with a second
    clone that pushes a commit of its own so `self.repo`'s ``fetch`` sees a
    remote-tracking ref it has not merged. Real divergence from ``git status
    --porcelain --branch``, never an asserted number — `_entry()`'s `ahead`/
    `behind` fields are pinned by nothing (fix round 1, finding 2) precisely
    because `RoundTrip`'s hand-built fixture never exercises the git-reading
    code that fills them in.

    Ahead and behind are made UNEQUAL (2 and 1) on purpose: the coordinator's
    own mutation probe swapped the `ahead`/`behind` key lookups in `_entry()`
    and the suite stayed green — which an equal-valued fixture (1 and 1) would
    have hidden even with a correct test, since swapping two equal numbers is
    invisible to `assertEqual`."""

    def setUp(self) -> None:
        super().setUp()
        origin = self.tmp / "origin.git"
        subprocess.run(["git", "init", "-q", "--bare", "-b", "main", str(origin)],
                       check=True, capture_output=True)
        _git(self.repo, "remote", "add", "origin", str(origin))
        _git(self.repo, "push", "-q", "-u", "origin", "main")

        # behind: someone else pushes to `origin`; `self.repo` fetches (updates
        # the remote-tracking ref) but never merges it into its own `main`.
        other = self.tmp / "other-clone"
        subprocess.run(["git", "clone", "-q", str(origin), str(other)],
                       check=True, capture_output=True)
        _git(other, "config", "user.email", "o@example.com")
        _git(other, "config", "user.name", "o")
        (other / "REMOTE.md").write_text("from elsewhere\n")
        _git(other, "add", "REMOTE.md")
        _git(other, "-c", "commit.gpgsign=false", "commit", "-qm", "remote change")
        _git(other, "push", "-q", "origin", "main")
        _git(self.repo, "fetch", "-q", "origin")

        # ahead: two commits of `self.repo`'s own, never pushed — 2 vs. `behind`'s
        # 1, deliberately unequal (see the class docstring).
        (self.repo / "LOCAL.md").write_text("local change\n")
        _git(self.repo, "add", "LOCAL.md")
        _git(self.repo, "-c", "commit.gpgsign=false", "commit", "-qm", "local change")
        (self.repo / "LOCAL.md").write_text("local change 2\n")
        _git(self.repo, "add", "LOCAL.md")
        _git(self.repo, "-c", "commit.gpgsign=false", "commit", "-qm", "local change 2")


class EntryFieldsAreReal(DivergedRepoIso, unittest.TestCase):
    """Fix round 1, finding 2: `tracked_dirty`, `ahead`, `behind`, `ci`, `change`
    and `sigil` were built by `_entry()` but exercised only through
    `RoundTrip`'s hand-built fixture, which proves `save`/`read` round-trip JSON
    and proves nothing about `_entry()`'s own construction — confirmed by
    mutation (swapping the `ahead`/`behind` lookups, or hardcoding `"ci": None`,
    both left the suite green). Each test below uses a fixture with a
    non-default value for the field it checks, so a swapped key or a dropped
    lookup has somewhere to be caught."""

    def test_ahead_and_behind_reflect_real_divergence(self):
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        entry = data["repos"][0]
        self.assertEqual(entry["ahead"], 2)
        self.assertEqual(entry["behind"], 1)

    def test_an_untracked_only_change_is_dirty_but_not_tracked_dirty(self):
        (self.repo / "scratch.txt").write_text("x\n")
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        entry = data["repos"][0]
        self.assertTrue(entry["dirty"])
        self.assertFalse(entry["tracked_dirty"])

    def test_a_tracked_edit_marks_both_dirty_and_tracked_dirty(self):
        (self.repo / "LOCAL.md").write_text("edited\n")
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        entry = data["repos"][0]
        self.assertTrue(entry["dirty"])
        self.assertTrue(entry["tracked_dirty"])

    def test_ci_change_and_sigil_come_from_glstate(self):
        """`glstate.read_for` only matches a cache entry whose recorded branch
        equals the tree's CURRENT branch (`glstate.py`'s own staleness rule) —
        seeded under ``"main"`` to match `self.repo`'s real branch, not a
        made-up one, so this proves the wiring rather than a coincidence."""
        cache_file = config.STATE_DIR / "cache" / "glstate.json"
        cache_file.parent.mkdir(parents=True, exist_ok=True)
        cache_file.write_text(json.dumps({
            str(self.repo): {"branch": "main", "ts": time.time(),
                              "change": 7, "ci": "failed", "sigil": "!"},
        }))
        data = gather.scan(workspace=config.DEFAULT_WORKSPACE, cwd=str(self.repo))
        entry = data["repos"][0]
        self.assertEqual(entry["ci"], "failed")
        self.assertEqual(entry["change"], 7)
        self.assertEqual(entry["sigil"], "!")


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


class WrongShapedCache(PersonaIso, unittest.TestCase):
    """Fix round 1, finding 1: `json.loads` succeeding proves only that the
    bytes were valid JSON, not that they are a scan. `42`, a bare string, a
    list, and a dict missing `repos`/`worktrees` all parse cleanly — and each
    used to come back from `read()` VERBATIM, so a renderer indexing
    `data["repos"]` on the `42` case got `TypeError: 'int' object is not
    subscriptable` instead of the degrade this module exists to provide."""

    def test_a_bare_int_falls_through_to_a_fresh_gather(self):
        self._assert_falls_through("42")

    def test_a_bare_string_falls_through_to_a_fresh_gather(self):
        self._assert_falls_through('"a string"')

    def test_a_bare_list_falls_through_to_a_fresh_gather(self):
        self._assert_falls_through("[1, 2, 3]")

    def test_a_dict_missing_repos_and_worktrees_falls_through_to_a_fresh_gather(self):
        self._assert_falls_through('{"foo": "bar"}')

    def _assert_falls_through(self, raw_json: str) -> None:
        d = state.frame_dir("f-1", create=True)
        (d / "gather.json").write_text(raw_json)
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


class Discard(PersonaIso, unittest.TestCase):
    """A new frame adopting a recycled pid inherits the directory of the frame that
    held that pid before it — #383's `state.reap` keeps a directory while its pid is
    live, and on a launch that pid is the launcher's own. `state.clear_exit` stops the
    dead frame's exit code coming with it; `discard` stops its cached SCAN coming with
    it, which `read` would otherwise serve to every panel (no freshness check, by
    design) until the session's first hook bump."""

    _SENTINEL = {"gathered_at": 0.0, "workspace": "sentinel", "current_repo": None,
                 "repos": [], "worktrees": []}

    def test_a_saved_scan_is_gone_afterwards(self):
        gather.save("f-1", {"gathered_at": 1.0, "workspace": "from-a-dead-frame",
                            "current_repo": None, "repos": [], "worktrees": []})
        gather.discard("f-1")
        with mock.patch.object(gather, "scan", return_value=self._SENTINEL) as scan_mock:
            got = gather.read("f-1")
        scan_mock.assert_called_once()
        self.assertEqual(got, self._SENTINEL,
                         "a dead frame's cached scan was served to the frame that "
                         "adopted its id")

    def test_the_version_a_panel_polls_is_left_alone(self):
        """`version` is a monotonic counter panels compare against their last reading;
        moving it — or removing it — is `state.bump`'s business, not this function's.
        Same division `state.clear_exit` makes."""
        state.bump("f-1")
        before = state.version("f-1")
        gather.discard("f-1")
        self.assertEqual(state.version("f-1"), before)

    def test_the_recorded_exit_code_is_left_alone(self):
        """The other file under the same directory has its own owner (`clear_exit`),
        called from the same two lines of `cmd_launch`. Neither may reach across."""
        state.record_exit("f-1", 42)
        gather.discard("f-1")
        self.assertEqual(state.exit_code("f-1"), 42)

    def test_discarding_a_frame_that_has_no_cache_creates_nothing(self):
        """It runs on the launch path against an id that usually has no directory at
        all — the ordinary first launch for a workspace. A launch must not mint one
        just to delete a file inside it, the same rule `read` follows."""
        gather.discard("never-existed")
        self.assertFalse(state.frame_dir("never-existed").exists())

    def test_discarding_a_hostile_fid_does_not_raise_or_create(self):
        gather.discard("../../escaped")  # must not raise
        self.assertFalse(state._root().exists())

    def test_discarding_survives_a_failing_unlink(self):
        """Nothing in this module raises: a launch is not worth failing over a file
        that could not be deleted."""
        gather.save("f-1", self._SENTINEL)
        with mock.patch("charter.frame.gather.Path.unlink",
                        side_effect=OSError("read-only")):
            gather.discard("f-1")  # must not raise


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
