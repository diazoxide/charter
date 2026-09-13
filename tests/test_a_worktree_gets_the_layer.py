"""A piece's worktree gets charter's layer, the way a clone does — #951.

`charter wt add <repo> <piece>` cuts a worktree at `workspaces/<ws>/.worktrees/<repo>/<piece>`
(or under a relocated root) and tells the worker to `cd` into it. That directory is a git root
of its own, so Claude Code reads the shared `.claude/settings.json` from it and nowhere else, and
its walk for `.claude/agents/` and `.claude/skills/` stops at it. Charter wrote its layer into
the clone beside it and never into the worktree: a session there had no plugin enabled, no
`$CHARTER_HARNESS`, no persona agents, none of the plane's `ask`/`deny` rules — while `reinit`
said there was nothing to do and `doctor`'s `workspace layer` row was green.

Every worktree here is cut by the real `cmd_worktree_add`, in a `PersonaIso` tmp plane REACHED
THROUGH A SYMLINK: git records a worktree by its resolved path, and on Linux a temp directory is
already resolved, so without the link the arithmetic that joins git's spelling to charter's
would be normalised for free on the runner and tested only on macOS. `git` identity and signing
come from `tests/_gitguard`.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import tempfile
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, commands_workspace, commands_worktree, config, doctor, workspace

from tests import _isolation
from tests.test_a_clone_gets_the_layer_and_hides_it import _git, _repo
from tests.test_a_restrictive_rule_reaches_a_workspace import _plane_local, _status
from tests.test_a_workspace_carries_charters_layer import _plane_settings

#: Spelled by hand: imported from `claude_code`, they would agree with whatever value it takes.
SHARED = ".claude/settings.json"
LOCAL = ".claude/settings.local.json"
AGENT = ".claude/agents/steward.md"
MARKER = ".charter-generated"


class WorktreeLayer(_isolation.PersonaIso):
    """A plane with committed and local restrictions and one persona agent, reached through a
    symlink, and a workspace `api` holding one real, wired clone `svc`."""

    def setUp(self) -> None:
        super().setUp()
        self.assertIn("edm-test-", str(config.STATE_DIR))
        real = self.tmp / "real-plane"
        real.mkdir()
        (real / "charter.toml").write_text("schema = 1\n")
        link = self.tmp / "link-plane"
        link.symlink_to("real-plane")
        self.addCleanup(config.restore, config.use(link))
        self.assertNotEqual(str(config.WORKSPACES_DIR), os.path.realpath(config.WORKSPACES_DIR),
                            "fixture normalised for free — the join to git's spelling is untested")
        _plane_settings(config.ROOT, permissions={"ask": ["Bash(terraform apply *)"],
                                                  "deny": ["Bash(rm -rf /)"]})
        _plane_local(config.ROOT, ask=["Bash(charter change land *)"])
        agents = config.ROOT / ".claude" / "agents"
        agents.mkdir(parents=True)
        (agents / "steward.md").write_text("route the work\n")
        self.ws = "api"
        workspace.ensure(self.ws)
        self.clone = _repo(workspace.workspace_dir(self.ws) / "svc")
        workspace.wire_harnesses(self.ws)

    def add(self, piece: str = "p1") -> tuple[int, str, Path]:
        """`charter wt add svc <piece> -w api`: its exit code, what it printed, where it cut."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_worktree.cmd_worktree_add(SimpleNamespace(
                workspace=self.ws, repo="svc", piece=piece, branch=None))
        path = commands_worktree.worktree.path_for(self.ws, "svc", piece)
        return rc, out.getvalue() + err.getvalue(), path

    def added(self, piece: str = "p1") -> Path:
        rc, said, path = self.add(piece)
        self.assertEqual(rc, 0, said)
        return path

    def by_hand(self, piece: str = "p1") -> Path:
        """A worktree at the layout's path made with plain git — what an older charter's
        `wt add` left, and what a worker who never ran charter makes."""
        path = commands_worktree.worktree.path_for(self.ws, "svc", piece)
        path.parent.mkdir(parents=True, exist_ok=True)
        _git(self.clone, "worktree", "add", "-q", "-b", piece, str(path))
        return path


class WtAddGivesTheWorktreeTheLayer(WorktreeLayer):
    def test_the_worktree_gets_the_planes_settings_and_its_restrictions(self):
        wt = self.added()
        doc = json.loads((wt / SHARED).read_text())
        self.assertEqual(doc["enabledPlugins"], {"charter@charter": True})
        self.assertEqual(doc["env"]["CHARTER_HARNESS"], "claude-code")
        self.assertEqual(doc["permissions"], {"ask": ["Bash(terraform apply *)"],
                                              "deny": ["Bash(rm -rf /)"]})

    def test_the_worktree_gets_the_planes_agents(self):
        wt = self.added()
        self.assertEqual((wt / AGENT).read_text(), "route the work\n")

    def test_the_worktree_gets_the_planes_local_rules_as_a_clone_does(self):
        wt = self.added()
        self.assertEqual(json.loads((wt / LOCAL).read_text())["permissions"],
                         {"ask": ["Bash(charter change land *)"]})

    def test_nothing_charter_wrote_shows_in_either_checkouts_status(self):
        wt = self.added()
        self.assertTrue((wt / MARKER).is_file(), "fixture: nothing was written to hide")
        self.assertEqual(_status(wt), "")
        self.assertEqual(_status(self.clone), "")

    def test_clone_names_a_piece_it_wires_by_its_path(self):
        """`charter clone` wires every guest, pieces included, and a piece's own name is not
        enough to find it by: every repository can have a `p1`."""
        self.by_hand()
        out = io.StringIO()
        with redirect_stderr(out):
            commands._wire_clones(self.ws)
        self.assertIn(".worktrees/svc/p1: charter's layer written", out.getvalue())

    def test_wt_add_says_what_it_wrote_and_where_it_is_hidden(self):
        _rc, said, _wt = self.add()
        self.assertIn("svc · p1: charter's layer written", said)
        self.assertIn("hidden in that repo's .git/info/exclude", said)


class ReinitRepairsAWorktreeWithoutTheLayer(WorktreeLayer):
    """A worktree an older charter's `wt add` cut, or one made with plain git, has no layer.
    `reinit` said "nothing to do" over it, because its scan listed only a workspace's direct
    children and a piece sits two levels down."""

    def reinit(self) -> str:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_workspace.cmd_workspace_reinit(SimpleNamespace(name=self.ws, all=False))
        self.assertEqual(rc, 0, out.getvalue() + err.getvalue())
        return out.getvalue() + err.getvalue()

    def test_reinit_writes_the_layer_into_it_and_names_the_worktree(self):
        wt = self.by_hand()
        said = self.reinit()
        self.assertIn(f"wrote .worktrees/svc/p1/{SHARED}", said)
        self.assertEqual(json.loads((wt / SHARED).read_text())["permissions"]["deny"],
                         ["Bash(rm -rf /)"])
        self.assertEqual((wt / AGENT).read_text(), "route the work\n")
        self.assertEqual(_status(wt), "")
        self.assertEqual(_status(self.clone), "")

    def test_a_launch_repairs_it_too(self):
        """`commands_frame._launch_root` runs `ensure`, which wires the workspace."""
        wt = self.by_hand()
        workspace.ensure(self.ws)
        self.assertTrue((wt / SHARED).is_file())

    def test_a_second_reinit_has_nothing_to_do(self):
        self.by_hand()
        self.reinit()
        self.assertNotIn(".worktrees/svc/p1", self.reinit())


class DoctorSeesTheWorktree(WorktreeLayer):
    def test_the_layer_rows_name_a_worktree_without_the_layer(self):
        self.by_hand()
        rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[f".worktrees/svc/p1/{SHARED}"], "missing")
        self.assertEqual(rows[f".worktrees/svc/p1/{AGENT}"], "missing")

    def test_the_workspace_layer_row_warns_and_names_the_repair(self):
        self.by_hand()
        r = doctor.check_workspace_harness()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(f"api/.worktrees/svc/p1/{SHARED} (missing)", r.detail)
        self.assertIn("charter workspace reinit --all", r.hint)

    def test_the_row_is_green_once_the_worktree_is_wired(self):
        self.added()
        self.assertEqual(doctor.check_workspace_harness().status, doctor.OK)

    def test_a_chat_rooted_in_the_worktree_is_told_which_rules_are_not_in_force(self):
        wt = self.by_hand()
        said = workspace.rules_not_in_force(wt)
        self.assertIn("NOT in force in this checkout", said)
        self.assertIn("Bash(terraform apply *)", said)
        self.assertIn("`charter workspace reinit api` writes them", said)

    def test_a_chat_deeper_inside_the_worktree_is_told_too(self):
        wt = self.by_hand()
        (wt / "src").mkdir()
        self.assertIn("Bash(rm -rf /)", workspace.rules_not_in_force(wt / "src"))

    def test_a_chat_in_a_wired_worktree_is_told_nothing(self):
        wt = self.added()
        self.assertEqual(workspace.rules_not_in_force(wt), "")

    def test_a_directory_at_a_pieces_path_that_is_no_checkout_is_told_nothing(self):
        """What `git worktree remove` of a piece with an ignored build directory can leave."""
        left = workspace.workspace_dir(self.ws) / ".worktrees" / "svc" / "gone"
        left.mkdir(parents=True)
        self.assertEqual(workspace.rules_not_in_force(left), "")

    def test_reinit_is_named_as_what_reaches_a_chat_rooted_in_a_piece(self):
        wt = self.by_hand()
        self.assertEqual(doctor._reinit_target(wt.resolve()), self.ws)


class DoctorsHintReadsAPiecesRowAsThatPiece(WorktreeLayer):
    """The hint groups rows by checkout. Split on its first `/`, every piece in a workspace is
    `.worktrees`, and a relocated piece's absolute label is the empty string."""

    def rows(self, *rows) -> None:
        self.enterContext(mock.patch.object(workspace, "harness_layer",
                                            lambda ws: list(rows) if ws == self.ws else []))

    def test_an_unrecorded_piece_does_not_keep_reinit_from_leading_for_another(self):
        self.by_hand("p1")
        self.by_hand("p2")
        self.rows((f".worktrees/svc/p1/{MARKER}", "unrecorded"),
                  (f".worktrees/svc/p2/{SHARED}", "missing"))
        self.enterContext(mock.patch.object(workspace, "unrecorded_fix",
                                            lambda tree, where: f"restore write access to {where}"))
        hint = doctor.check_workspace_harness().hint
        self.assertIn("api/.worktrees/svc/p1: restore write access to that checkout", hint)
        self.assertIn("charter workspace reinit --all clears the rest", hint)

    def test_what_an_unaccounted_piece_could_not_account_for_is_printed(self):
        wt = self.by_hand()
        self.rows((f".worktrees/svc/p1/.git/info/exclude", "unaccounted"))
        self.enterContext(mock.patch.object(
            workspace, "unaccounted",
            lambda tree: ["the piece's own reason"] if tree == wt else []))
        self.assertIn("the piece's own reason", doctor.check_workspace_harness().hint)


def _charters_files(tree: Path) -> list[str]:
    """What of charter's is left in *tree*: its marker, and anything under `.claude/`."""
    left = [MARKER] if os.path.lexists(tree / MARKER) else []
    return left + sorted(str(p.relative_to(tree)) for p in (tree / ".claude").rglob("*"))


class RelocatedRoot(WorktreeLayer):
    """`$CHARTER_WORKTREES` / `[plane] worktrees`: pieces at `<root>/<ws>/<repo>/<piece>`, outside
    `workspaces/` and outside the plane, reached through a symlink for the class docstring's
    reason. `config.worktrees_root_for` hands back a resolved root; spelled through a link here, the
    fixture is the harder of the two."""

    def setUp(self) -> None:
        super().setUp()
        real = Path(tempfile.mkdtemp(prefix="edm-test-worktrees-"))
        self.addCleanup(shutil.rmtree, real, ignore_errors=True)
        link = self.tmp / "worktrees-link"
        link.symlink_to(real)
        config.WORKTREES_ROOT = link
        self.root = link


class APieceUnderARelocatedRootGetsTheLayer(RelocatedRoot):
    def test_wt_add_wires_it(self):
        wt = self.added()
        self.assertEqual(wt, self.root / self.ws / "svc" / "p1", "fixture: cut somewhere else")
        self.assertTrue((wt / SHARED).is_file())
        self.assertEqual(_status(wt), "")

    def test_reinit_repairs_one_made_by_hand_and_doctor_names_it_first(self):
        wt = self.by_hand()
        self.assertEqual(dict(workspace.harness_layer(self.ws))[f"{wt}/{SHARED}"], "missing")
        detail = doctor.check_workspace_harness().detail
        self.assertIn(f"{wt}/{SHARED} (missing)", detail)
        self.assertNotIn(f"{self.ws}//", detail)
        workspace.wire_harnesses(self.ws)
        self.assertTrue((wt / SHARED).is_file())
        self.assertEqual(doctor.check_workspace_harness().status, doctor.OK)

    def test_guard_ask_names_it_by_its_path(self):
        wt = self.added()
        (wt / SHARED).write_text('{"env": {"THEIRS": "1"}}\n')
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            commands.cmd_guard_ask(SimpleNamespace(pattern="kubectl delete *", local=False))
        self.assertIn(f"NOT in force in {wt}/{SHARED}", out.getvalue())
        self.assertNotIn(f"{self.ws}//", out.getvalue())

    def test_a_chat_in_it_is_told_which_rules_are_not_in_force(self):
        wt = self.by_hand()
        self.assertIn("`charter workspace reinit api` writes them",
                      workspace.rules_not_in_force(os.path.realpath(wt)))

    def test_removing_the_workspace_leaves_none_of_charters_files_in_it(self):
        """`workspace remove` is a `shutil.rmtree` of `workspaces/<ws>`, which never reaches a
        piece under a relocated root: that directory, and whatever charter wrote into it,
        outlive the workspace unless the removal takes them out first."""
        wt = self.added()
        (wt / "work.txt").write_text("the worker's\n")
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            rc = commands_workspace.cmd_workspace_remove(SimpleNamespace(name=self.ws, force=True))
        self.assertEqual(rc, 0, out.getvalue())
        self.assertTrue((wt / "work.txt").is_file(), "fixture: the piece itself went too")
        self.assertEqual(_charters_files(wt), [])


class WhichWorktreesArePieces(WorktreeLayer):
    """Git is the registry, and the layout decides whose piece a listed worktree is."""

    def test_a_piece_removed_by_hand_is_not_one(self):
        wt = self.by_hand()
        shutil.rmtree(wt)
        self.assertNotIn(wt, workspace.guest_trees(self.ws))

    def test_a_worktree_directly_under_the_root_is_not_a_piece(self):
        odd = workspace.workspace_dir(self.ws) / ".worktrees" / "odd"
        odd.parent.mkdir(parents=True)
        _git(self.clone, "worktree", "add", "-q", "-b", "odd", str(odd))
        self.assertEqual(workspace.guest_trees(self.ws), [self.clone])

    def test_a_directory_whose_name_only_begins_like_the_root_holds_no_pieces(self):
        """`.worktrees-old/…` is spelled with `.worktrees` in front and is not under it."""
        wt = self.by_hand()
        self.assertIsNone(workspace.checkout_row(self.ws, f".worktrees-svc/p1/{SHARED}"))
        self.assertEqual(workspace.checkout_row(self.ws, f".worktrees/svc/p1/{SHARED}"),
                         (wt, SHARED))

    def test_a_chat_in_a_clone_is_still_read_as_the_clones(self):
        """Not every chat is in a piece: `worktree.locate` answers nothing for a clone."""
        (self.clone / SHARED).unlink()
        self.assertIn("Bash(rm -rf /)", workspace.rules_not_in_force(self.clone))

    def test_yesterdays_pieces_stay_pieces_once_the_root_is_relocated(self):
        """`worktree.locate`'s reason: declaring `[plane] worktrees` moves where the next piece
        is cut, not the pieces already cut."""
        wt = self.by_hand()
        config.WORKTREES_ROOT = self.tmp / "relocated"
        self.assertIn(wt, workspace.guest_trees(self.ws))
        workspace.wire_harnesses(self.ws)
        self.assertTrue((wt / SHARED).is_file())

    def test_a_pieces_row_through_a_link_inside_it_is_still_that_pieces(self):
        """A `.claude` the piece's branch commits as a link out: its row is `foreign`, and the hint
        tells the owner how to commit their own file — which needs the row read back to the piece
        by its spelling, since the path resolves nowhere near it."""
        wt = self.by_hand()
        elsewhere = Path(tempfile.mkdtemp(prefix="edm-victim-"))
        self.addCleanup(shutil.rmtree, elsewhere, ignore_errors=True)
        (elsewhere / "settings.json").write_text("{}\n")
        (wt / ".claude").symlink_to(elsewhere)
        self.assertEqual(workspace.checkout_row(self.ws, f".worktrees/svc/p1/{SHARED}"),
                         (wt, SHARED))


class RemovingTheWorkspaceUnwiresItsPieces(WorktreeLayer):
    def test_unwiring_takes_the_pieces_files_and_leaves_both_statuses_clean(self):
        wt = self.added()
        removed = workspace.unwire_guests(self.ws)
        self.assertIn(f".worktrees/svc/p1/{SHARED}", removed)
        self.assertEqual(_charters_files(wt), [])
        self.assertEqual(_status(wt), "")
        self.assertNotIn(workspace._EXCLUDE_BEGIN, workspace.git_exclude_file(self.clone).read_text())

    def test_another_workspaces_piece_of_this_clone_is_left_wired(self):
        """A worktree made by hand under `other`'s root from `api`'s clone is `other`'s piece by
        where it lives. Removing `api` must not strip a checkout it does not hold."""
        workspace.ensure("other")
        theirs = workspace.workspace_dir("other") / ".worktrees" / "svc" / "theirs"
        theirs.parent.mkdir(parents=True)
        _git(self.clone, "worktree", "add", "-q", "-b", "theirs", str(theirs))
        workspace.wire_guest(theirs)
        self.assertNotIn(theirs, workspace.guest_trees(self.ws))
        workspace.unwire_guests(self.ws)
        self.assertTrue((theirs / SHARED).is_file())


class TheClonesBlockSurvivesItsWorktrees(WorktreeLayer):
    """A piece reads its clone's `info/exclude`. Adding one, removing one, or unwiring one must
    leave the clone's own files hidden in the clone."""

    def test_adding_and_removing_pieces_keeps_the_clone_hidden(self):
        self.added("p1")
        self.added("p2")
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(out):
            rc = commands_worktree.cmd_worktree_remove(SimpleNamespace(
                workspace=self.ws, repo="svc", piece="p1", force=False, delete_branch=False))
        self.assertEqual(rc, 0, out.getvalue())
        self.assertEqual(_status(self.clone), "")
        workspace.wire_harnesses(self.ws)
        self.assertEqual(_status(self.clone), "")

    def test_unwiring_a_piece_keeps_the_clones_lines(self):
        wt = self.added()
        workspace.unwire_guest(wt)
        block = workspace.git_exclude_file(self.clone).read_text()
        for line in (f"/{SHARED}", f"/{LOCAL}", f"/{AGENT}", f"/{MARKER}"):
            self.assertIn(line, block)
        self.assertEqual(_status(self.clone), "")


class GitIsAskedOncePerRepository(WorktreeLayer):
    def git_asked(self) -> list[list[str]]:
        calls: list[list[str]] = []
        real = workspace.util.run

        def _record(cmd, *args, **kwargs):
            calls.append(list(cmd))
            return real(cmd, *args, **kwargs)

        self.enterContext(mock.patch.object(workspace.util, "run", _record))
        return calls

    def test_a_launch_with_pieces_lists_the_worktrees_once(self):
        self.added("p1")
        self.added("p2")
        calls = self.git_asked()
        workspace.wire_harnesses(self.ws)
        self.assertEqual(len([c for c in calls if "worktree" in c]), 1)

    def test_a_piece_is_listed_once_when_two_checkouts_share_its_repository(self):
        """A worktree at the workspace's top level is a guest of its own and lists the same
        pieces its clone does."""
        wt = self.added()
        _git(self.clone, "worktree", "add", "-q", "-b", "flat",
             str(workspace.workspace_dir(self.ws) / "svc-flat"))
        self.assertEqual(workspace.guest_trees(self.ws).count(wt), 1)

    def test_a_worktrees_directory_that_cannot_be_checked_is_still_asked_of_git(self):
        wt = self.by_hand()
        admin = str(self.clone / ".git" / "worktrees")
        real = os.scandir

        def refusing(path=".", *a, **k):
            if os.fspath(path) == admin:
                raise PermissionError(13, "Permission denied", admin)
            return real(path, *a, **k)

        with mock.patch.object(workspace.os, "scandir", refusing):
            self.assertIn(wt, workspace.guest_trees(self.ws))

    def test_git_that_cannot_be_run_costs_the_pieces_and_nothing_else(self):
        self.by_hand()

        def _missing(cmd, *a, **k):
            raise FileNotFoundError("git")

        with mock.patch.object(workspace.util, "run", _missing):
            rows = dict(workspace.harness_layer(self.ws))
        self.assertEqual(rows[f"svc/{SHARED}"], "ok")
        self.assertFalse([r for r in rows if r.startswith(".worktrees/")])


class APieceWhoseRootLeavesItsBase(WorktreeLayer):
    """#1062's containment, asked of a piece. A `.worktrees` (or a relocated `<root>/<ws>`) that is
    a link out of its base sends `git worktree add` wherever it points, and git lists the piece
    THERE. Charter names that checkout and writes nothing into it, and takes nothing out of it."""

    def setUp(self) -> None:
        super().setUp()
        self.outside = Path(tempfile.mkdtemp(prefix="edm-victim-")).resolve()
        self.addCleanup(shutil.rmtree, self.outside, ignore_errors=True)

    def assert_named_and_untouched(self, label: str, wt: Path, said: str) -> None:
        self.assertTrue(os.path.realpath(wt).startswith(str(self.outside)),
                        "fixture: the piece did not land outside")
        self.assertIn("charter's layer was not written", said)
        self.assertEqual(_charters_files(wt), [])
        self.assertEqual(dict(workspace.harness_layer(self.ws))[f"{label}/{MARKER}"], "foreign")
        self.assertEqual(dict(workspace.wire_harnesses(self.ws))[f"{label}/{MARKER}"], "blocked")
        self.assertEqual(_charters_files(wt), [])
        # Something that looks like charter's own, planted there: removal must not follow it out.
        (wt / ".claude").mkdir()
        (wt / SHARED).write_text("{}\n")
        (wt / MARKER).write_text(json.dumps({SHARED: workspace.content_digest("{}\n")}))
        workspace.unwire_guests(self.ws)
        self.assertEqual(_charters_files(wt), [MARKER, ".claude/settings.json"])

    def test_a_linked_worktrees_directory(self):
        (workspace.workspace_dir(self.ws) / ".worktrees").symlink_to(self.outside)
        rc, said, wt = self.add()
        self.assertEqual(rc, 0, said)
        self.assertIn(wt, workspace.guest_trees(self.ws), "fixture: git's listing was not joined")
        self.assert_named_and_untouched(".worktrees/svc/p1", wt, said)

    def test_a_linked_workspace_directory_under_a_relocated_root(self):
        root = Path(tempfile.mkdtemp(prefix="edm-test-worktrees-"))
        self.addCleanup(shutil.rmtree, root, ignore_errors=True)
        config.WORKTREES_ROOT = root
        (root / self.ws).symlink_to(self.outside)
        rc, said, wt = self.add()
        self.assertEqual(rc, 0, said)
        self.assertIn(wt, workspace.guest_trees(self.ws), "fixture: git's listing was not joined")
        self.assert_named_and_untouched(str(wt), wt, said)

    def test_a_root_that_contains_the_plane_is_not_the_base_a_workspace_child_answers_to(self):
        """`$CHARTER_WORKTREES` takes anything. Set to a directory above the plane, it must not
        widen what a `workspaces/<ws>/<name>` link may reach."""
        config.WORKTREES_ROOT = Path(os.path.commonpath([os.path.realpath(self.tmp),
                                                         str(self.outside)]))
        repo = _repo(self.outside / "theirs")
        child = workspace.workspace_dir(self.ws) / "theirs"
        child.symlink_to(repo)
        self.assertEqual(dict(workspace.wire_harnesses(self.ws))[f"theirs/{MARKER}"], "blocked")
        self.assertFalse((repo / SHARED).exists())

    def test_with_no_relocated_root_a_checkout_outside_workspaces_is_not_wired(self):
        repo = _repo(self.outside / "theirs")
        self.assertEqual(workspace.wire_guest(repo), [(MARKER, "blocked")])
        self.assertFalse((repo / SHARED).exists())
