"""#917's siblings: every other place charter read an empty answer from git as a fact.

`charter save` is where the conflation was observed, and it is not where it lived. A `git
status` that failed writes nothing to stdout, so ``.stdout.strip()`` is empty — the same
value a clean tree produces — and the survey behind #917 found eleven such readings across
seven modules. They fail in **pairs**, which is how a defect of this shape survives being
noticed:

* `commands_persona._pending_memory` and `hooks._uncommitted_memory_nudge` are both halves
  of "is memory unsynced". One printed a green ``✓ nothing to sync``, the other stayed
  silent, for the same reason at the same moment.
* `commands_workspace._work_at_risk` and `_worktrees_at_risk` are both halves of the gate
  that decides whether `workspace remove` may hand a clone to `shutil.rmtree`.
* `doctor.check_plane_root` and `statusline._run_state` are deliberately built to agree, so
  they now agree on being wrong.

**The failure planted here is a repository git will not stand in**, not a lock. That is
deliberate and it is measured: under a held ``index.lock``, `git status` still answers
correctly (see `test_save_refuses_a_tree_it_could_not_read`) — the lock breaks `git add`,
which is why it broke `save` and nothing else. These sites break on the *other* ways git
declines to answer, and before this they were the same value as an empty answer.

What each site does about it differs, and the difference is the point rather than an
inconsistency:

======================================  ==========================================
a gate on a deletion or a merge          refuse — an unread tree is not a cleared one
a gate on a claim about durability       refuse, and say what charter could not read
a preflight verdict                      "not checked", never a tick
a column in a table                      a third word, ``unknown``
a plane with no repository at all        unchanged — a definite answer, not an unread one
======================================  ==========================================
"""

from __future__ import annotations

import io
import os
import subprocess
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import (commands, commands_persona, commands_worktree, config, doctor,
                     gitstate, hooks, workspace, worktree)
from charter import commands_workspace as cw
from tests._isolation import PersonaIso

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


def _real_plane(case) -> Path:
    """A plane that IS a control plane and IS a git repository, pointed at by `config`.

    `PersonaIso` derives `config` from a bare tmp directory, so `HAS_CONTROL_PLANE` is
    False there and every `doctor` row short-circuits to "no control plane found" — a green
    row that would pass a test asserting nothing. `config.use` re-derives, and its return
    value is restored on the way out.
    """
    plane = case.tmp / "plane"
    plane.mkdir(parents=True, exist_ok=True)
    (plane / "charter.toml").write_text("schema = 1\n")
    subprocess.run(["git", "init", "-q", "-b", "main", str(plane)], check=True,
                   capture_output=True, env={**os.environ, **_GIT_ENV})
    case.addCleanup(config.restore, config.use(plane))
    return plane


def _said(fn, *a, **kw):
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(*a, **kw)
    return rc, out.getvalue() + err.getvalue()


class Unreadable(PersonaIso):
    """A plane with one clone git refuses to answer about, and one it answers fine.

    The broken clone is a directory holding an **empty ``.git`` directory**. Two properties
    make it the right fixture and neither is incidental: `workspace.is_clone` counts it (a
    clone's ``.git`` is a directory — that IS git's own definition), and ``git -C <it>
    status --porcelain`` exits 128 writing nothing to stdout. Real git, real emptiness, no
    stub anywhere in the path under test.
    """

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")

    def git(self, cwd, *args: str):
        return subprocess.run(["git", "-C", str(cwd), *args], check=True,
                              capture_output=True, text=True,
                              env={**os.environ, **_GIT_ENV})

    def broken_clone(self, repo: str = "svc", ws: str = "alpha") -> Path:
        d = workspace.workspace_dir(ws) / repo
        (d / ".git").mkdir(parents=True, exist_ok=True)
        return d

    def healthy_clone(self, repo: str = "web", ws: str = "alpha") -> Path:
        d = workspace.workspace_dir(ws) / repo
        d.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(d)], check=True,
                       capture_output=True, env={**os.environ, **_GIT_ENV})
        (d / "README").write_text("base\n")
        self.git(d, "add", "-A")
        self.git(d, "-c", "commit.gpgsign=false", "commit", "-qm", "base")
        return d


class TestGitStatusThatFailedIsNotAnEmptyStatus(Unreadable):
    """The fixture itself, pinned against real git — the rest is worthless without it."""

    def test_the_broken_clone_exits_non_zero_with_empty_stdout(self):
        d = self.broken_clone()
        r = subprocess.run(["git", "-C", str(d), "status", "--porcelain"],
                           capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertEqual(r.stdout.strip(), "")

    def test_and_charter_can_now_tell_that_apart_from_clean(self):
        broken, healthy = self.broken_clone(), self.healthy_clone()
        self.assertFalse(gitstate.read(broken).known)
        self.assertTrue(gitstate.read(healthy).known)
        self.assertEqual(gitstate.read(healthy).rows, ())


class TestWorkspaceRemoveWillNotDeleteWhatItCouldNotRead(Unreadable):
    """The sharpest instance in the package: what `_work_at_risk` comes back empty of is
    what `cmd_workspace_remove` hands to `shutil.rmtree`."""

    def remove(self, force: bool = False):
        return _said(cw.cmd_workspace_remove, SimpleNamespace(name="alpha", force=force))

    def test_an_unreadable_clone_is_work_at_risk(self):
        self.broken_clone()
        self.assertTrue(any("could not be read" in r for r in cw._work_at_risk("alpha")))

    def test_removal_refuses(self):
        self.broken_clone()
        rc, said = self.remove()
        self.assertEqual(rc, 2, said)

    def test_and_the_workspace_is_still_there(self):
        self.broken_clone()
        self.remove()
        self.assertTrue(workspace.workspace_dir("alpha").exists())

    def test_a_readable_clean_workspace_still_removes(self):
        """The other half. A guard that refuses everything protects nothing, because the
        first thing it teaches is `--force`."""
        self.healthy_clone()
        rc, said = self.remove()
        self.assertEqual(rc, 0, said)
        self.assertFalse(workspace.workspace_dir("alpha").exists())

    def test_force_still_wins(self):
        self.broken_clone()
        rc, said = self.remove(force=True)
        self.assertEqual(rc, 0, said)
        self.assertFalse(workspace.workspace_dir("alpha").exists())


class TestASnapshotDoesNotClaimAStateNobodyMeasured(Unreadable):
    """`_restore_blockers` empty is what lets the manifest assert it captured reality — to
    another engineer, on another machine, who cannot know it was never checked."""

    def test_an_unreadable_clone_blocks(self):
        self.broken_clone()
        self.assertTrue(any("could not be read" in b
                            for b in cw._restore_blockers("alpha")))

    def test_snapshot_refuses(self):
        self.broken_clone()
        rc, said = _said(cw.cmd_workspace_snapshot,
                         SimpleNamespace(name="alpha", force=False, description=None))
        self.assertEqual(rc, 2, said)
        self.assertIn("could not be read", said)


class TestSyncDoesNotMergeIntoATreeItNeverRead(Unreadable):
    """The polarity that makes this one worse than the usual reading. Everywhere else an
    unread `git status` costs a warning nobody sees; in `_sync_one` the empty answer is
    what AUTHORISES the `git merge --ff-only` two lines below it."""

    def test_it_skips_and_says_why(self):
        d = self.broken_clone()
        rc, said = _said(commands._sync_one, d, "alpha")
        self.assertIn("skipping", said)
        self.assertIn("could not read", said)

    def test_it_never_reaches_fetch_or_merge(self):
        """Asserted on the argv rather than on the outcome: with the guard gone, `fetch`
        runs and fails, and the test would still see a repo nothing had merged into."""
        d = self.broken_clone()
        real = commands._git
        seen: list[list[str]] = []

        def spy(args, cwd=None):
            seen.append(list(args))
            return real(args, cwd=cwd)

        with mock.patch.object(commands, "_git", spy):
            _said(commands._sync_one, d, "alpha")
        self.assertEqual([a for a in seen if a and a[0] in ("fetch", "merge")], [])


class TestTheStatusTableSaysUnknownRatherThanClean(Unreadable):
    """A display site, and it still gets the third word. This row already has a written
    record of printing `clean` over a tree that was not (uninitialised submodules, #817)."""

    def test_an_unreadable_clone_is_not_clean(self):
        note = commands._clone_note(self.broken_clone())
        self.assertIn("unknown", note)
        self.assertNotIn("clean", note)

    def test_a_healthy_clone_still_reads_clean(self):
        self.assertIn("clean", commands._clone_note(self.healthy_clone()))


class TestAWorktreeCharterCouldNotReadIsNotRemovable(Unreadable):
    """`worktree.is_dirty` returned `bool`, so the third state was unrepresentable — which
    is why fixing the call sites alone could not have worked. Its own siblings already
    answered in three: `unique_commits` returns ``None``, and the refusal four lines below
    this one has said "could not determine … refusing to remove" since #104."""

    def wire(self, piece: str = "slice"):
        clone = self.healthy_clone(repo="svc")
        wt = worktree.path_for("alpha", "svc", piece)
        wt.parent.mkdir(parents=True, exist_ok=True)
        self.git(clone, "worktree", "add", "-q", "-b", piece, str(wt))
        return clone, wt

    def break_it(self, wt: Path):
        """Point the worktree's ``.git`` file at nothing. The directory still EXISTS, which
        is what keeps this off the `prunable` path that #597 already covers — the two
        failures are different and only one of them had ever been measured."""
        (wt / ".git").write_text("gitdir: /nonexistent/worktrees/gone\n")

    def test_dirt_says_it_could_not_look(self):
        _clone, wt = self.wire()
        self.break_it(wt)
        self.assertFalse(worktree.dirt(wt).known)

    def test_worktree_remove_refuses(self):
        _clone, wt = self.wire()
        self.break_it(wt)
        rc, said = _said(commands_worktree.cmd_worktree_remove,
                         SimpleNamespace(workspace="alpha", repo="svc", piece="slice",
                                         force=False))
        self.assertEqual(rc, 1, said)
        self.assertIn("could not determine", said)

    def test_the_workspace_gate_lists_it_too(self):
        """The other half of the pair. `_worktrees_at_risk` feeds the same rmtree."""
        _clone, wt = self.wire()
        self.break_it(wt)
        self.assertTrue(any("could not be checked for uncommitted changes" in r
                            for r in cw._worktrees_at_risk("alpha")))

    def test_the_listing_prints_unknown(self):
        _clone, wt = self.wire()
        self.break_it(wt)
        rc, said = _said(commands_worktree.cmd_worktree_list,
                         SimpleNamespace(workspace="alpha", repo=None))
        line = next(ln for ln in said.splitlines() if "slice" in ln)
        self.assertIn("unknown", line)
        self.assertNotIn("clean", line)


class MemoryPlane(PersonaIso):
    """A plane that IS a git repository, so the memory pair has something to fail at."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text('schema = 1\n')
        subprocess.run(["git", "init", "-q", "-b", "main", str(config.ROOT)], check=True,
                       capture_output=True, env={**os.environ, **_GIT_ENV})

    def unreadable(self):
        """`git status` fails while the plane still HAS a repository — so `no_repo` is
        False and the exemption below cannot be what makes a test pass."""
        return mock.patch.object(
            gitstate, "read",
            return_value=gitstate.TreeState(config.ROOT, (), "fatal: bad index file",
                                            None, False))


class TestBothHalvesOfIsMemoryUnsyncedStopAnsweringNo(MemoryPlane):
    """They failed together, so they are pinned together."""

    def test_memory_sync_refuses_rather_than_ticking(self):
        with self.unreadable():
            rc, said = _said(commands_persona.cmd_persona_memory_sync,
                             SimpleNamespace(no_push=True, sign=False))
        self.assertEqual(rc, 1, said)
        self.assertNotIn("nothing to sync", said)
        self.assertIn("bad index file", said)

    def test_the_session_start_nudge_says_it_could_not_look(self):
        with mock.patch.object(config, "MEMORY_SHARE", "commit"), self.unreadable():
            said = hooks._uncommitted_memory_nudge()
        self.assertIn("could not read", said)
        self.assertIn("bad index file", said)


class TestAPlaneWithNoRepositoryIsADefiniteAnswer(MemoryPlane):
    """`charter init` in a fresh directory does not run `git init` — the README's own
    60-second path. A plane with no repository has nowhere to sync TO, which is a fact
    rather than a failure to look, and telling every such plane at every session start that
    charter cannot read it would be a new false alarm on a supported resting state."""

    def setUp(self) -> None:
        super().setUp()
        import shutil
        shutil.rmtree(config.ROOT / ".git")

    def test_the_state_says_so(self):
        state = gitstate.read(config.ROOT, "personas")
        self.assertFalse(state.known)
        self.assertTrue(state.no_repo)

    def test_the_nudge_stays_silent(self):
        with mock.patch.object(config, "MEMORY_SHARE", "commit"):
            self.assertEqual(hooks._uncommitted_memory_nudge(), "")

    def test_memory_sync_does_not_refuse_over_it(self):
        rc, said = _said(commands_persona.cmd_persona_memory_sync,
                         SimpleNamespace(no_push=True, sign=False))
        self.assertNotIn("could not read", said)


class TestDoctorWillNotTickOverAStatusItCouldNotRun(PersonaIso):
    """`check_plane_root` was the one rc-blind git call in a function that checks every
    other one it makes — `--show-toplevel`, `symbolic-ref`, both `rev-list`s and
    `@{upstream}` all branch on `returncode`. The surrounding `try` already turned a
    TIMEOUT into an honest "not checked"; a non-zero exit deserves the same sentence."""

    def setUp(self) -> None:
        super().setUp()
        plane = _real_plane(self)
        (plane / "README").write_text("x\n")
        subprocess.run(["git", "-C", str(plane), "add", "-A"], check=True,
                       capture_output=True, env={**os.environ, **_GIT_ENV})
        subprocess.run(["git", "-C", str(plane), "-c", "commit.gpgsign=false",
                        "commit", "-qm", "base"], check=True, capture_output=True,
                       env={**os.environ, **_GIT_ENV})

    def test_a_status_that_exits_non_zero_is_not_checked(self):
        real = doctor._git_in

        def fake(root, *args):
            if args and args[0] == "status":
                return subprocess.CompletedProcess(args, 128, "", "fatal: bad index file")
            return real(root, *args)

        with mock.patch.object(doctor, "_git_in", fake):
            r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("not checked", r.detail)

    def test_a_status_that_answers_still_reads_clean(self):
        r = doctor.check_plane_root()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("clean", r.detail)


class TestDoctorNoticesAStaleLockBeforeASaveRunsIntoIt(PersonaIso):
    """The row #917 asked for. The operator's lock had been there for twenty-three hours,
    through however many sessions, and the first thing to look at it was the `charter save`
    it broke."""

    def setUp(self) -> None:
        super().setUp()
        _real_plane(self)

    def plant(self, *, size: int, age: float) -> Path:
        p = config.ROOT / ".git" / gitstate.LOCK_NAME
        p.write_bytes(b"x" * size)
        when = time.time() - age
        os.utime(p, (when, when))
        self.addCleanup(p.unlink, missing_ok=True)
        return p

    def test_the_row_exists_and_is_named(self):
        self.assertIn("index lock", doctor.check_names())

    def test_no_lock_is_green(self):
        r = doctor.check_index_lock()
        self.assertEqual(r.status, doctor.OK)

    def test_no_plane_is_green_and_says_so(self):
        """`check_control_plane_config` already says this loudly; a second row repeating it
        would be noise, and a row asking git about a plane that does not exist would be a
        subprocess spent on nothing."""
        with mock.patch.object(config, "HAS_CONTROL_PLANE", False):
            r = doctor.check_index_lock()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("no control plane", r.detail)

    def test_a_git_that_will_not_answer_is_not_checked(self):
        with mock.patch.object(doctor.gitstate, "for_repo",
                               side_effect=OSError("no git on PATH")):
            r = doctor.check_index_lock()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("not checked", r.detail)

    def test_a_stale_lock_warns_and_names_it(self):
        lock = self.plant(size=0, age=23 * 3600)
        r = doctor.check_index_lock()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(str(lock.resolve()), r.detail)
        self.assertIn("23h", r.detail)
        self.assertIn(f"rm -f {lock.resolve()}", r.hint)

    def test_the_remedy_is_the_operators_and_charter_says_so(self):
        lock = self.plant(size=0, age=23 * 3600)
        r = doctor.check_index_lock()
        self.assertIn("charter never removes a lock", r.hint)
        doctor.check_index_lock()
        self.assertTrue(lock.exists(), "doctor removed a lock it does not own")

    def test_a_lock_a_live_git_is_holding_is_not_a_warning(self):
        """A lock is not a fault — git takes one for every write to the index. Painting the
        preflight yellow for a healthy `git commit` mid-flight is a false alarm on correct
        behaviour, and a permanently-yellow row is one people stop reading."""
        self.plant(size=4096, age=2)
        r = doctor.check_index_lock()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("held now", r.detail)


class TestTheSweepWillNotMeasureATreeNobodyIsLookingAt(unittest.TestCase):
    """`tools.sweep.dirty_files` passed `check=False` to a helper that returns stdout only,
    so the exit status was not merely unchecked — it was unavailable. An empty listing
    built the sandbox from committed HEAD alone and the whole run reported findings against
    a tree the operator was not looking at.

    `NoSandbox` is that category and it is twenty lines up in the same file: a statement
    about the machine, made before a single mutation has been measured."""

    def test_a_status_that_failed_raises_nosandbox(self):
        import tempfile
        from tools import sweep
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(sweep.NoSandbox) as caught:
                sweep.dirty_files(Path(d), ("charter",))
        said = str(caught.exception)
        self.assertIn("could not be read", said)
        self.assertIn("not a verdict about any mutation", said)
        self.assertIn("fatal", said)      # git's own words, kept

    def test_a_git_that_failed_silently_still_raises(self):
        """An exit status with no words is still an exit status — the phrase `_must` uses
        for the same case, so the two readings of "the machine stopped us" read alike."""
        import tempfile
        from tools import sweep
        done = subprocess.CompletedProcess(("git",), 9, "", "")
        with tempfile.TemporaryDirectory() as d:
            with mock.patch.object(sweep.subprocess, "run", return_value=done):
                with self.assertRaises(sweep.NoSandbox) as caught:
                    sweep.dirty_files(Path(d), ("charter",))
        self.assertIn("without saying why", str(caught.exception))

    def test_a_clean_tree_still_returns_an_empty_dict(self):
        """The other side, and it is not decoration: a guard that raises on everything is
        indistinguishable from this one until something passes through it."""
        import tempfile
        from tools import sweep
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            subprocess.run(["git", "init", "-q", "-b", "main", str(root)], check=True,
                           capture_output=True, env={**os.environ, **_GIT_ENV})
            self.assertEqual(sweep.dirty_files(root, ("charter",)), {})


if __name__ == "__main__":
    unittest.main()
