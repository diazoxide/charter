"""`charter doctor` reports when the plane root is being worked in.

The plane root — the directory holding `charter.toml` — holds the control plane:
personas, inventory, workspaces, config, and nothing anyone is meant to edit or switch
branches in. Work happens in a workspace's clones. Nothing in the filesystem makes that
true (ADR 0008), and the failure it invites is invisible in exactly the surface a user
would check: two sessions both sitting in the plane root share one working tree and one
HEAD and thrash each other's branches, while charter reports two different workspaces and
lists no tree that would hint at why. In the session that produced the ADR that was six
branches and a `git checkout main` that silently reverted in-flight work out of the tree.

This is the preflight half of that signal, and the replacement for the deleted
`check_embedded_worktrees` — the check that guarded the shape ADR 0007 removed. Removing a
guard is fine; leaving the *new* failure mode unwatched in the file built to watch for
failure modes is not.

Real git in a temp dir throughout (the `tests/test_worktree.py` pattern): "is this tree
dirty" and "which branch is this repo's default" are answers only git has, and against a
mocked git these tests would prove nothing about how that answer is read.
"""

from __future__ import annotations

import shutil
import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import config, doctor, util
from tests._isolation import PersonaIso


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


def init_repo(path: Path, branch: str = "main") -> Path:
    """A real git repo with one commit, so HEAD and `git status` both have answers.

    Identity and signing are set locally rather than inherited: whoever runs the suite
    may have `commit.gpgsign = true` globally, and a signer that waits on a hardware
    prompt would hang the test rather than fail it.
    """
    subprocess.run(["git", "init", "-q", "-b", branch, str(path)],
                   check=True, capture_output=True)
    git(path, "config", "user.email", "t@example.com")
    git(path, "config", "user.name", "t")
    # Background maintenance races the teardown. `git maintenance run --auto` fires after
    # ordinary commands, takes `.git/maintenance.lock`, and releases it — while
    # `shutil.rmtree` is walking that directory, which then dies on a name that existed
    # when it was listed and not when it was unlinked. Seen on CI as
    # `FileNotFoundError: 'maintenance.lock'` in a test that only deletes a fixture, on one
    # Python job while the others passed the same commit.
    #
    # Disabled at the source rather than tolerated in each teardown: a fixture repo has
    # nothing to maintain, and `ignore_errors=True` on the rmtree would hide real breakage
    # in tests whose whole subject is what is on disk.
    git(path, "config", "gc.auto", "0")
    git(path, "config", "maintenance.auto", "false")
    git(path, "config", "commit.gpgsign", "false")
    git(path, "add", "-A")
    git(path, "commit", "-qm", "init")
    return path


class PlaneRootCheck(PersonaIso):
    """A plane root that is its own git repo, on `main`, with everything committed."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        init_repo(self.tmp)

    # ------------------------------------------------------------------ healthy
    def test_a_clean_plane_root_on_its_default_branch_is_healthy(self):
        """The whole point of the row: when nobody is working in the root it must say
        so in green. A check that is yellow in the normal case is a check people stop
        reading — the same reason `check_memory_indexes` refuses to nag."""
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("main", r.detail)

    def test_untracked_files_in_the_plane_root_are_not_work_in_progress(self):
        """Memory defaults to `share = "local"` — written to disk and deliberately never
        committed — so a long-lived plane always carries untracked files under
        `personas/*/memory/`. Counting those as dirt would make this row permanently
        yellow within a day of use, which teaches people to ignore the preflight and
        costs the two findings that actually matter."""
        (self.tmp / "personas" / "note.md").write_text("a memory nobody committed\n")
        self.assertEqual(doctor.check_plane_root().status, doctor.OK)

    # ------------------------------------------------------------------- dirty
    def test_an_edit_to_a_tracked_file_in_the_plane_root_needs_attention(self):
        """An edit here is product work in the one directory that isolates nothing."""
        (self.tmp / "charter.toml").write_text("schema = 1\n# edited in the root\n")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("uncommitted", r.detail)

    def test_a_staged_change_is_still_work_in_the_plane_root(self):
        """`git add` is not a fix for it, so staging must not make the row go green."""
        (self.tmp / "feature.py").write_text("print('product work')\n")
        git(self.tmp, "add", "feature.py")
        self.assertEqual(doctor.check_plane_root().status, doctor.WARN)

    def test_the_dirty_message_says_where_the_work_belongs_instead(self):
        """"Your root is dirty" is a statement; a preflight has to be an instruction.
        Both routes out are named because both are legitimate: control-plane content
        gets committed, product work moves to a clone."""
        (self.tmp / "charter.toml").write_text("schema = 1\n# edited\n")
        hint = doctor.check_plane_root().hint
        self.assertIn("charter save", hint)
        self.assertIn("charter workspace create", hint)

    # ------------------------------------------------------------------ branch
    def test_a_plane_root_on_a_non_default_branch_needs_attention(self):
        git(self.tmp, "checkout", "-q", "-b", "feature/x")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("feature/x", r.detail)
        self.assertIn("main", r.detail)

    def test_the_branch_message_says_how_to_put_the_root_back(self):
        """The observed harm was a `git checkout main` in the root that reverted
        in-flight work; the hint has to name the branch to return to and the place the
        branch work should have happened."""
        git(self.tmp, "checkout", "-q", "-b", "feature/x")
        hint = doctor.check_plane_root().hint
        self.assertIn("checkout main", hint)
        self.assertIn("charter workspace create", hint)

    def test_a_detached_head_in_the_plane_root_needs_attention(self):
        """Detached is not a branch, so the branch comparison would call it "on HEAD" —
        true only in the sense that a wrong answer is a string. It is also unambiguously
        someone doing git surgery in the root, which is the thing being watched for."""
        git(self.tmp, "checkout", "-q", "--detach")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("detached", r.detail)

    def test_the_remotes_own_default_beats_the_main_master_guess(self):
        """`origin/HEAD` is the remote's own answer to "what is default here", recorded
        by clone. A repo whose default is `trunk` can still have a stale `main` lying
        around, so guessing before asking would warn about the correct branch."""
        git(self.tmp, "checkout", "-q", "-b", "trunk")
        git(self.tmp, "update-ref", "refs/remotes/origin/trunk", "HEAD")
        git(self.tmp, "symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/trunk")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK, r.detail)
        self.assertIn("trunk", r.detail)

    def test_a_root_with_no_discoverable_default_is_never_warned_about(self):
        """No `origin/HEAD`, no `main`, no `master`: charter has no idea what this
        repo's default is, and inventing one would fire a warning at every session of a
        plane whose only sin is naming its branch something else. Silence beats a
        confident wrong answer — the dirt half of the check still applies."""
        git(self.tmp, "branch", "-m", "main", "wip")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK, r.detail)

    # -------------------------------------------------------------- degradation
    def test_a_plane_root_that_is_not_a_git_repository_is_not_a_finding(self):
        """`charter init` in a fresh directory does not run `git init`, and that is the
        README's own 60-second path. A plane with no history has no branch to be on and
        no dirt to carry."""
        shutil.rmtree(self.tmp / ".git")
        self.assertEqual(doctor.check_plane_root().status, doctor.OK)

    def test_a_plane_nested_inside_another_repo_is_not_judged_by_that_repo(self):
        """A `charter.toml` in a subdirectory of some larger repo means the enclosing
        repo is not the plane's — its branch is whatever that project is working on and
        its dirt is that project's work in progress. Reporting on it would produce a
        warning about a repo the plane does not own, every session, for a state that is
        correct."""
        nested = self.tmp / "plane"
        nested.mkdir()
        (nested / "charter.toml").write_text("schema = 1\n")
        config.ROOT = nested
        (self.tmp / "charter.toml").write_text("schema = 1\n# the outer project's work\n")
        git(self.tmp, "checkout", "-q", "-b", "their-feature")
        self.assertEqual(doctor.check_plane_root().status, doctor.OK)

    def test_no_control_plane_at_all_is_not_a_finding(self):
        """With no plane there is no plane root; `check_control_plane_config` already
        says that loudly, and a second row saying it again is noise."""
        config.HAS_CONTROL_PLANE = False
        self.assertEqual(doctor.check_plane_root().status, doctor.OK)

    def test_an_unreadable_root_degrades_to_a_report_rather_than_raising(self):
        """doctor runs from the SessionStart hook and is the command you run *because*
        something is wrong. It may not be the thing that breaks."""
        with mock.patch("charter.doctor.util.run", side_effect=OSError("no git here")):
            r = doctor.check_plane_root()
        # WARN since #171, not OK: it still degrades to a report rather than raising, which
        # is what this test is about — but "not checked" is the absence of information, and
        # a green glyph over it is read as health by anyone scanning the column.
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("not checked", r.detail)

    def test_a_git_that_never_answers_degrades_to_a_report(self):
        """A plane root on a stalled network mount hangs `git status`, and the preflight
        has a hook timeout: the check must come back with something rather than eat the
        whole budget and print nothing."""
        with mock.patch("charter.doctor.util.run",
                        side_effect=util.ProcTimeout(["git", "status"], 5.0)):
            r = doctor.check_plane_root()
        # WARN since #171, not OK: it still degrades to a report rather than raising, which
        # is what this test is about — but "not checked" is the absence of information, and
        # a green glyph over it is read as health by anyone scanning the column.
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("not checked", r.detail)

    # -------------------------------------------------------------- integration
    def test_it_warns_and_never_fails(self):
        """FAIL is doctor's "you cannot work" list — it is what makes `charter doctor`
        exit 1, which is what makes the SessionStart wrapper print the preflight-failed
        banner. A root being worked in is a smell that gets expensive later, not a
        broken plane, and ADR 0008 chose signal over refusal deliberately."""
        (self.tmp / "charter.toml").write_text("schema = 1\n# edited\n")
        git(self.tmp, "checkout", "-q", "-b", "feature/x")
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("uncommitted", r.detail)
        self.assertIn("feature/x", r.detail)

    def test_it_is_registered_in_the_preflight(self):
        """An unregistered check reports drift only to someone who already suspected
        drift — the exact way `persona lint` sat unrun for months."""
        self.assertIn("plane root", [r.name for r in doctor.run_all()])


if __name__ == "__main__":
    unittest.main()
