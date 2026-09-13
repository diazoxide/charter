"""Wiring a worktree hides nothing of yours through the `info/exclude` it shares — #1072.

A clone and its linked worktrees read ONE `.git/info/exclude`, the common git directory's.
Charter hides what it writes by listing each path there. So when a piece is wired, the line for
its `.claude/settings.json` hides that path in the clone as well — and an untracked
`.claude/settings.json` of the operator's own in the clone, one charter never wrote and rightly
left alone, vanished from the clone's `git status`. Nothing was deleted; work nobody had committed
stopped being visible, which is how it gets lost.

Every worktree here is cut by the real `cmd_worktree_add` (`test_a_worktree_gets_the_layer`'s
fixture: a real clone in a plane reached through a symlink).
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_workspace, commands_worktree, doctor, workspace

from tests.test_a_clone_gets_the_layer_and_hides_it import _git, _repo
from tests.test_a_restrictive_rule_reaches_a_workspace import _status
from tests.test_a_worktree_gets_the_layer import AGENT, LOCAL, MARKER, SHARED, WorktreeLayer

#: Somebody's own settings, untracked in the clone.
YOURS = '{"env": {"MINE": "1"}}\n'


class YourFileInTheClone(WorktreeLayer):
    """The clone holds an untracked `.claude/settings.json` of the operator's own, there before
    charter first wired it — so charter left it alone and listed no line for it."""

    def setUp(self) -> None:
        super().setUp()
        # Back to a clone charter has never wired, then the operator's file, then the wire that
        # finds it there: the order in which the file is somebody's and not charter's.
        workspace.unwire_guest(self.clone)
        (self.clone / ".claude").mkdir(exist_ok=True)
        (self.clone / SHARED).write_text(YOURS)
        workspace.wire_harnesses(self.ws)
        self.assertIn(f"?? {SHARED}\n", _status(self.clone), "fixture: your file is not visible")
        # Git lists a worktree by its resolved path, and that is how the sentence names it.
        self.yours = os.path.realpath(self.clone / SHARED)

    def test_wt_add_leaves_your_file_visible_in_the_clone(self):
        self.added()
        self.assertIn(f"?? {SHARED}\n", _status(self.clone))
        self.assertEqual((self.clone / SHARED).read_text(), YOURS)

    def test_the_worktree_still_gets_the_planes_rules(self):
        wt = self.added()
        self.assertEqual(json.loads((wt / SHARED).read_text())["permissions"]["deny"],
                         ["Bash(rm -rf /)"])

    def test_wt_add_names_your_file_and_what_clears_it(self):
        _rc, said, _wt = self.add()
        self.assertIn(f"charter's {SHARED} is not hidden there, because {self.yours} is an "
                      f"untracked file charter did not write", said)
        self.assertIn(f"commit or move {self.yours}", said)
        # The announcement's promise is two claims, and one of them no longer holds.
        self.assertNotIn("`git status` there is unaffected", said)

    def test_doctors_row_names_your_file_and_what_clears_it(self):
        self.added()
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[".worktrees/svc/p1/.git/info/exclude"], "unhidden")
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("api/.worktrees/svc/p1/.git/info/exclude (unhidden)", r.detail)
        self.assertIn(f"{self.yours} is an untracked file charter did not write", r.hint)
        self.assertIn(f"commit or move {self.yours}", r.hint)
        self.assertNotIn("charter workspace reinit --all", r.hint.split("   ")[0])

    def test_reinit_names_your_file_and_what_clears_it(self):
        self.by_hand()
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        said = out.getvalue() + err.getvalue()
        self.assertIn(f"commit or move {self.yours}", said)
        self.assertNotIn("wrote .worktrees/svc/p1/.git/info/exclude", said)
        self.assertNotIn("nothing to do", said)
        self.assertIn(f"?? {SHARED}\n", _status(self.clone))

    def test_a_launch_keeps_your_file_visible(self):
        self.added()
        workspace.ensure(self.ws)
        workspace.wire_harnesses(self.ws)
        self.assertIn(f"?? {SHARED}\n", _status(self.clone))

    def test_once_you_commit_it_reinit_hides_charters(self):
        """What the sentence says clears it. A tracked file stays in `git status` whatever the
        exclude lists, so the line hides nothing of yours once yours is committed."""
        wt = self.added()
        self.assertIn(f"?? {SHARED}\n", _status(wt), "fixture: charter's file is hidden already")
        _git(self.clone, "add", SHARED)
        _git(self.clone, "commit", "-qm", "my settings")
        commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        self.assertEqual(_status(wt), "")
        self.assertEqual(_status(self.clone), "")
        self.assertEqual(dict(workspace.harness_layer(self.ws))[
            ".worktrees/svc/p1/.git/info/exclude"], "ok")

    def test_a_committed_marker_claiming_your_file_does_not_make_it_charters(self):
        """#1062: a `.charter-generated` git tracks is committed content, not charter's record, so
        it cannot vouch for your file as charter's and have it hidden."""
        marker = json.loads((self.clone / MARKER).read_text())
        marker[SHARED] = workspace.content_digest(YOURS)
        (self.clone / MARKER).write_text(json.dumps(marker))
        _git(self.clone, "add", "-f", MARKER)
        _git(self.clone, "commit", "-qm", "a marker")
        self.added()
        self.assertIn(f"?? {SHARED}\n", _status(self.clone))

    def test_removing_the_worktree_leaves_the_clones_block_right(self):
        """Hidden once, your file stayed hidden after the worktree went: a line stays while its path
        is there in a checkout charter wires (#942), and the clone is one."""
        self.added()
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            # `--force`: charter's own file shows in the piece, which `wt remove` counts as work.
            rc = commands_worktree.cmd_worktree_remove(SimpleNamespace(
                workspace=self.ws, repo="svc", piece="p1", force=True, delete_branch=False))
        self.assertEqual(rc, 0, out.getvalue())
        self.assertEqual(_status(self.clone), f"?? {SHARED}\n")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.clone), f"?? {SHARED}\n")
        self.assertEqual(doctor.check_workspace_harness().detail.count("(unhidden)"), 0)


class CharterFilesStillHiddenEverywhere(WorktreeLayer):
    def test_a_piece_wired_before_its_clone_hides_charters_files_in_both(self):
        """Charter's own file at the path in another checkout is no reason to leave a line out. A
        block somebody emptied is written again by the next piece cut, before the clone is wired."""
        self.added("p1")
        workspace.git_exclude_file(self.clone).write_text("")
        wt = self.added("p2")
        self.assertEqual(_status(wt), "")
        self.assertEqual(dict(workspace.harness_layer(self.ws))[
            ".worktrees/svc/p2/.git/info/exclude"], "ok")


@unittest.skipIf(os.geteuid() == 0, "root lists a mode-000 directory")
class AClonesWorktreesGitCannotList(WorktreeLayer):
    """Git keeps a clone's worktrees in `.git/worktrees/`. Unreadable, `git worktree list` lists
    the clone alone and exits 0 (measured, git 2.50.1), so its pieces got no layer, no repair and
    no row, and nothing said they had not been checked."""

    def setUp(self) -> None:
        super().setUp()
        self.wt = self.by_hand()
        self.admin = self.clone / ".git" / "worktrees"
        self.admin.chmod(0)
        self.addCleanup(self.admin.chmod, 0o755)
        self.fix = f"restoring read access to {self.admin} clears this"

    def test_the_layer_rows_name_the_clone(self):
        self.assertEqual(dict(workspace.harness_layer(self.ws))["svc/.git/worktrees"], "unlisted")

    def test_doctor_names_the_clone_and_what_clears_it(self):
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("api/svc/.git/worktrees (unlisted)", r.detail)
        self.assertIn(f"api/svc: {self.fix}", r.hint)

    def test_reinit_names_the_clone_and_what_clears_it(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        said = out.getvalue() + err.getvalue()
        self.assertIn(f"'api': git could not list the worktrees of svc", said)
        self.assertIn(self.fix, said)
        self.assertNotIn("nothing to do", said)

    def test_a_clone_wire_names_the_clone_and_what_clears_it(self):
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            commands._wire_clones(self.ws)
        self.assertIn("svc: git could not list the worktrees of that clone", out.getvalue())
        self.assertIn(self.fix, out.getvalue())


class AClonesWorktreesDirectoryIsALoop(WorktreeLayer):
    def test_doctor_names_the_loop_rather_than_read_access(self):
        self.by_hand()
        admin = self.clone / ".git" / "worktrees"
        moved = self.tmp / "worktrees-kept"
        admin.rename(moved)
        self.addCleanup(lambda: (admin.unlink(), moved.rename(admin)))
        admin.symlink_to(admin.name)
        self.assertIn(f"api/svc: fix the symlink loop at {admin}",
                      doctor.check_workspace_harness().hint)


class AGitThatFailsNamesTheClone(WorktreeLayer):
    """The other ways a listing is not given: git exits non-zero, or cannot be run at all."""

    def rows_with(self, answer):
        self.by_hand()
        real = workspace.util.run

        def _run(cmd, *a, **k):
            if "worktree" in cmd:
                if isinstance(answer, Exception):
                    raise answer
                return answer
            return real(cmd, *a, **k)

        with mock.patch.object(workspace.util, "run", _run):
            return dict(workspace.harness_layer(self.ws))

    def test_a_git_that_exits_non_zero(self):
        failed = subprocess.CompletedProcess([], 128, stdout="", stderr="fatal: nope")
        self.assertEqual(self.rows_with(failed)["svc/.git/worktrees"], "unlisted")

    def test_a_git_that_cannot_be_run(self):
        self.assertEqual(self.rows_with(FileNotFoundError("git"))["svc/.git/worktrees"],
                         "unlisted")

    def test_a_repository_is_named_once_for_the_checkouts_that_share_it(self):
        _git(self.clone, "worktree", "add", "-q", "-b", "flat",
             str(workspace.workspace_dir(self.ws) / "svc-flat"))
        failed = subprocess.CompletedProcess([], 128, stdout="", stderr="fatal: nope")
        named = [rel for rel, status in self.rows_with(failed).items() if status == "unlisted"]
        self.assertEqual(named, ["svc/.git/worktrees"])

    def test_a_clone_whose_listing_git_gives_is_not_named(self):
        self.by_hand()
        self.assertNotIn("svc/.git/worktrees", dict(workspace.harness_layer(self.ws)))

    def test_a_marker_charter_cannot_read_in_the_clone_is_still_charters(self):
        """An untracked `.charter-generated` is charter's own even torn (#1062), and no file of
        yours: the piece's marker is hidden beside it."""
        self.added("p1")
        workspace.git_exclude_file(self.clone).write_text("")
        (self.clone / MARKER).write_text("{torn")
        wt = self.added("p2")
        self.assertNotIn(MARKER, _status(wt))

    def test_a_file_of_yours_git_already_ignores_does_not_stop_the_line(self):
        """Hidden with or without charter's line — as a global ignore hides Claude Code's own
        `settings.local.json` on many machines — so the line hides nothing more of yours."""
        workspace.unwire_guest(self.clone)
        (self.clone / ".claude").mkdir(exist_ok=True)
        (self.clone / SHARED).write_text(YOURS)
        ignore = self.tmp / "your-ignore"
        ignore.write_text(f"{SHARED}\n")
        _git(self.clone, "config", "core.excludesFile", str(ignore))
        wt = self.added()
        # `_status` sets the ignore aside, so what hides charter's file in the piece is its line.
        self.assertEqual(_status(wt), "")


class GitIsAskedAboutAPathOnlyWhereAFileIsThere(WorktreeLayer):
    def git_status_of(self, paths) -> list[list[str]]:
        asked: list[list[str]] = []
        real = workspace.util.run

        def _record(cmd, *a, **k):
            if "status" in cmd and cmd[-1] in paths:
                asked.append(list(cmd))
            return real(cmd, *a, **k)

        self.enterContext(mock.patch.object(workspace.util, "run", _record))
        return asked

    def test_a_checkout_with_nothing_at_the_path_is_not_asked(self):
        """Every launch wires every checkout: a `git status` per path per tree is the cost of a
        check that only a file there can answer yes to."""
        workspace.unwire_guest(self.clone)
        asked = self.git_status_of({SHARED, LOCAL, AGENT})
        self.added()
        self.assertEqual(asked, [])

    def test_a_launch_asks_about_your_file_once(self):
        workspace.unwire_guest(self.clone)
        (self.clone / ".claude").mkdir(exist_ok=True)
        (self.clone / SHARED).write_text(YOURS)
        self.by_hand()
        asked = self.git_status_of({SHARED})
        workspace.wire_harnesses(self.ws)
        self.assertEqual(len(asked), 1, asked)


class YourFileInAMainCheckoutCharterDoesNotWire(WorktreeLayer):
    """A worktree in a workspace whose main checkout lives outside the plane: charter wires the
    worktree and never the checkout, which reads the same exclude all the same."""

    def test_reinit_names_it_on_every_run_and_never_says_nothing_to_do(self):
        outside = _repo(self.tmp / "outside")
        (outside / ".claude").mkdir()
        (outside / SHARED).write_text(YOURS)
        wt = workspace.workspace_dir(self.ws) / "ext"
        _git(outside, "worktree", "add", "-q", "-b", "ext", str(wt))
        for _run in range(2):
            out, err = io.StringIO(), io.StringIO()
            with redirect_stdout(out), redirect_stderr(err):
                commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
            said = out.getvalue() + err.getvalue()
            self.assertIn(f"commit or move {os.path.realpath(outside / SHARED)}", said)
            self.assertNotIn("nothing to do", said)
        self.assertIn(f"?? {SHARED}\n", _status(outside))
