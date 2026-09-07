"""`charter save` said the tree was clean when it had not been able to read the tree.

#917, on the operator's own plane and not hypothesised::

    $ git status --porcelain | wc -l
          13
    $ charter save
    • Nothing to save — the control-plane working tree is clean.

The cause was a stale ``.git/index.lock`` — zero bytes, twenty-three hours old, no process
holding it, left by a git that had crashed the day before. With the file removed, the
identical command on the identical tree committed twenty files.

**The mechanism, measured against git 2.50.1** and reproduced by `LockedPlane` below:

===========================  ==============================================
``git status --porcelain``   rc 0, every modified file listed
``git add -A``               rc 128, ``Unable to create '…/index.lock'``
``git diff --cached --quiet``rc 0 — nothing was staged, so there is no diff
===========================  ==============================================

`planegit.commit_push` discarded the add's exit status and read the diff's ``0`` as *the
tree is clean*. The lock is invisible to the question charter asks last and fatal to the
one it asks first, which is why the defect renders as a confident sentence rather than as
an error.

**Why this instance is worse than its siblings.** Every other version of this shape in
charter degrades to "nothing to show" — a missing gauge, an empty row, a cache read as
cold. This one degrades to a claim about durability that is false, and the operator's next
act is to stop worrying about the work. It becomes data loss the moment anything
afterwards assumes the save happened: a `git checkout`, a worktree removal, a machine
going away.

So `commit_push` now answers three questions where it used to answer two, and this module
pins all three: the tree is clean, the tree has changes, or **charter could not tell** — in
which case it refuses, names the lock and says how old it is.

**And it never removes the lock.** `TestCharterReportsALockAndLeavesIt` is that rule. A
lock held by a live git is real and one command cannot tell the two apart from the inside;
the operator cleared #917's by hand after checking `ps` and the file's mtime, which is the
right division of labour.
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

from charter import commands, config, gitstate, planegit
from tests._isolation import PersonaIso

#: How many files the operator's tree held. Thirteen because that is the number in the
#: issue: the reproduction is worth nothing if it is not the observed shape.
DIRTY_FILES = 13

#: The age of the lock that was actually found. Twenty-three hours, so `23h` is what the
#: refusal has to be able to say.
LOCK_AGE = 23 * 3600

PLANE_TOML = 'schema = 1\n\n[[forge]]\nkind = "github"\n'


def _run(fn, *a, **kw):
    """Call *fn*, returning ``(rc, everything it said)``. `util` writes to stderr and some
    commands print to stdout; a refusal has to be findable in either."""
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(*a, **kw)
    return rc, out.getvalue() + err.getvalue()


def _save_args(message=None):
    return SimpleNamespace(message=message, sign=False, no_push=True)


class LockedPlane(PersonaIso):
    """A real plane repository, really dirty, with a real lock file planted in it.

    Real git throughout. The defect is a disagreement between what two git subcommands do
    with the same lock, so a fixture that faked either of them would be asserting against
    this module's idea of git rather than against git's.
    """

    def setUp(self) -> None:
        super().setUp()
        self.plane = self.tmp / "plane"
        self.plane.mkdir(parents=True, exist_ok=True)
        (self.plane / "charter.toml").write_text(PLANE_TOML)
        (self.plane / "personas").mkdir()
        for i in range(DIRTY_FILES):
            (self.plane / "personas" / f"p{i}.md").write_text(f"# persona {i}\n")
        self.git("init", "-q", "-b", "main", ".")
        # Repo-scoped, never global: `tests._gitguard` already redirects the config files
        # every child of this suite reads, and a fixture writing outside the repo under
        # test would put that back.
        self.git("config", "user.email", "r@e.invalid")
        self.git("config", "user.name", "r")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", "-A")
        self.git("commit", "-qm", "plane")
        self.base = self.head()

        # The operator's thirteen modified files.
        for i in range(DIRTY_FILES):
            (self.plane / "personas" / f"p{i}.md").write_text(f"# persona {i}\nedited\n")

        self.addCleanup(config.restore, config.use(self.plane))
        self.standing_in(self.plane)

    # -- the repository ---------------------------------------------------- #
    def git(self, *argv: str) -> str:
        p = subprocess.run(["git", "-C", str(self.plane), *argv],
                           check=True, capture_output=True, text=True)
        return p.stdout.strip()

    def head(self) -> str:
        return self.git("rev-parse", "HEAD")

    def porcelain(self) -> list[str]:
        return [ln for ln in self.git("status", "--porcelain").splitlines() if ln.strip()]

    def staged(self) -> list[str]:
        return [ln for ln in self.git("diff", "--cached", "--name-only").splitlines() if ln]

    def standing_in(self, where: Path):
        """chdir for the rest of the test. Registered rather than a ``finally``, and before
        `PersonaIso`'s rmtree: a cwd left inside a deleted temp directory makes
        `Path.cwd()` raise for every test that runs after this one."""
        here = Path.cwd()
        self.addCleanup(os.chdir, here)
        os.chdir(where)

    # -- the lock ---------------------------------------------------------- #
    @property
    def lock_path(self) -> Path:
        return self.plane / ".git" / gitstate.LOCK_NAME

    def plant_lock(self, *, size: int = 0, age: float = LOCK_AGE) -> Path:
        """Leave an ``index.lock`` behind, exactly as a crashed git does.

        **Always removed again on the way out**, whatever the test does with it. A fixture
        that left one behind would hand the next module a repository no `git add` can
        touch — this defect, planted in the suite that exists to catch it.
        """
        p = self.lock_path
        p.write_bytes(b"x" * size)
        when = time.time() - age
        os.utime(p, (when, when))
        self.addCleanup(p.unlink, missing_ok=True)
        return p

    def save(self, **kw):
        return _run(commands.cmd_save, _save_args(**kw))


class TestTheTreeWasReadableAndTheAnswerWasStillEmpty(LockedPlane):
    """The asymmetry the defect is made of, pinned against real git.

    Not a test of charter at all, and that is the point: if git ever stops answering
    `status` under a held lock, or starts refusing `diff --cached`, the reproduction below
    is no longer reproducing #917 and this fails first and says so.
    """

    def test_status_still_lists_every_file(self):
        self.plant_lock()
        self.assertEqual(len(self.porcelain()), DIRTY_FILES)

    def test_add_cannot_stage_anything(self):
        self.plant_lock()
        r = subprocess.run(["git", "-C", str(self.plane), "add", "-A"],
                           capture_output=True, text=True)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("index.lock", r.stderr)

    def test_and_the_index_then_looks_exactly_like_a_clean_tree(self):
        """``diff --cached --quiet`` → 0. The lie, one layer below charter."""
        self.plant_lock()
        r = subprocess.run(["git", "-C", str(self.plane), "diff", "--cached", "--quiet"],
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)


class TestSaveRefusesATreeItCouldNotRead(LockedPlane):
    """Outcome 3. The sentence in the issue must not be printable here."""

    def test_it_does_not_say_the_tree_is_clean(self):
        self.plant_lock()
        rc, said = self.save()
        self.assertNotIn("Nothing to save", said)
        self.assertNotIn("clean", said)

    def test_it_refuses(self):
        """rc 1, not 0. `charter save` exiting 0 is what a caller reads as "it is saved",
        and every harm in #917 is downstream of something believing that."""
        self.plant_lock()
        rc, said = self.save()
        self.assertEqual(rc, 1, said)

    def test_it_names_the_lock(self):
        lock = self.plant_lock()
        rc, said = self.save()
        self.assertIn(str(lock.resolve()), said)

    def test_it_says_how_old_the_lock_is(self):
        """The age is the fact that decides what the operator does: seconds means wait,
        hours means the writer is dead. A refusal without it sends both to the same place."""
        self.plant_lock()
        rc, said = self.save()
        self.assertIn("23h", said)

    def test_a_zero_byte_lock_hours_old_is_called_a_crash(self):
        self.plant_lock(size=0, age=LOCK_AGE)
        rc, said = self.save()
        self.assertIn("crashed", said)

    def test_a_fresh_lock_is_not_called_a_crash(self):
        """Contention is a different sentence, because it has a different answer. A live
        `git commit` holding the index is not something to `rm`."""
        self.plant_lock(size=0, age=2)
        rc, said = self.save()
        self.assertEqual(rc, 1, said)
        self.assertNotIn("crashed", said)

    def test_it_commits_nothing(self):
        self.plant_lock()
        self.save()
        self.assertEqual(self.head(), self.base)

    def test_it_stages_nothing(self):
        self.plant_lock()
        self.save()
        self.assertEqual(self.staged(), [])

    def test_the_operators_work_is_still_there_to_save(self):
        """The harm stated as what was actually at stake: thirteen files that charter had
        just reported safe."""
        self.plant_lock()
        self.save()
        self.assertEqual(len(self.porcelain()), DIRTY_FILES)


class TestCharterReportsALockAndLeavesIt(LockedPlane):
    """charter never runs the `rm`. This is a rule, not an implementation detail."""

    def test_the_lock_survives_the_refusal(self):
        lock = self.plant_lock()
        self.save()
        self.assertTrue(lock.exists(), "charter removed a lock it does not own")

    def test_the_removal_is_offered_to_the_operator(self):
        lock = self.plant_lock()
        rc, said = self.save()
        self.assertIn(f"rm -f {lock.resolve()}", said)

    def test_and_the_check_that_makes_it_safe_comes_first(self):
        """`ps` before `rm`. A remedy that printed the removal alone would teach the habit
        of skipping the one step that makes it safe."""
        rc, said = self.save() if self.plant_lock() else (None, "")
        self.assertLess(said.index("ps -eo"), said.index("rm -f"), said)


class TestTheOtherTwoOutcomesAreUnchanged(LockedPlane):
    """A refusal that also broke saving would be a worse defect than the one it fixed."""

    def test_a_clean_tree_still_says_nothing_to_save(self):
        self.git("add", "-A")
        self.git("commit", "-qm", "everything")
        rc, said = self.save()
        self.assertEqual(rc, 0, said)
        self.assertIn("Nothing to save", said)

    def test_a_dirty_tree_still_saves(self):
        """The other half of the issue's reproduction: with no lock in the way, the
        identical command on the identical tree commits."""
        rc, said = self.save()
        self.assertEqual(rc, 0, said)
        self.assertNotEqual(self.head(), self.base)
        self.assertIn(f"{DIRTY_FILES} file(s)", said)

    def test_the_same_tree_saves_once_the_operator_clears_the_lock(self):
        lock = self.plant_lock()
        self.assertEqual(self.save()[0], 1)
        lock.unlink()
        rc, said = self.save()
        self.assertEqual(rc, 0, said)
        self.assertNotEqual(self.head(), self.base)


class TestAnUnreadableIndexIsRefusedWithoutALock(LockedPlane):
    """The category is *charter could not tell*, not *there is a lock*.

    A lock is the way #917 happened; it is not the only way git answers "I could not
    compute that". ``diff --cached --quiet`` returns 0 for no difference, 1 for a
    difference and something else entirely when it failed, and the third of those used to
    fall into the same branch as the second and reach `git commit` with an empty message
    about zero files.
    """

    def test_a_diff_that_could_not_run_is_not_a_clean_tree(self):
        real = planegit._git

        def fake(args, cwd=None):
            if args[:2] == ["diff", "--cached"] and "--quiet" in args:
                return subprocess.CompletedProcess(args, 128, "", "fatal: bad index file")
            return real(args, cwd=cwd)

        with mock.patch.object(planegit, "_git", fake):
            rc, said = self.save()
        self.assertEqual(rc, 1, said)
        self.assertNotIn("Nothing to save", said)
        self.assertIn("bad index file", said)


class TestALockIsDescribedByThreeFacts(unittest.TestCase):
    """`gitstate` on its own: the record, not the command that prints it."""

    def test_a_missing_lock_is_none(self):
        self.assertIsNone(gitstate.find(Path("/nonexistent-git-dir-917")))

    def test_zero_bytes_and_old_is_a_crash(self):
        lock = gitstate.IndexLock(Path("/x/index.lock"), 0, gitstate.STALE_AFTER + 1)
        self.assertTrue(lock.crashed)
        self.assertIn("crashed", lock.describe())

    def test_zero_bytes_and_young_is_not(self):
        lock = gitstate.IndexLock(Path("/x/index.lock"), 0, 1)
        self.assertFalse(lock.crashed)

    def test_a_lock_with_an_index_in_it_is_not_a_crash_however_old(self):
        """git writes the new index INTO the lock before it opens an editor, so the
        long-lived legitimate holder is not zero bytes. Calling that one abandoned is how a
        report gets someone to delete an index that was about to be renamed into place."""
        lock = gitstate.IndexLock(Path("/x/index.lock"), 4096, 30 * 86400)
        self.assertFalse(lock.crashed)

    def test_the_age_is_coarse_and_true(self):
        self.assertEqual(gitstate.age_phrase(0), "0s")
        self.assertEqual(gitstate.age_phrase(59), "59s")
        self.assertEqual(gitstate.age_phrase(60), "1m")
        self.assertEqual(gitstate.age_phrase(3600), "1h")
        self.assertEqual(gitstate.age_phrase(23 * 3600), "23h")
        self.assertEqual(gitstate.age_phrase(86400 * 3), "3d")

    def test_a_negative_age_reads_as_now(self):
        """A clock that went backwards, or a file stamped in the future by a bad archive.
        `-4s old` would read as a defect in charter rather than in the mtime."""
        self.assertEqual(gitstate.age_phrase(-5), "0s")


class TestALockedWorktreeIsItsOwnIndex(LockedPlane):
    """A linked worktree keeps its own index under ``.git/worktrees/<name>``, so the lock
    that stops a save there is not the plane's. Guessing ``root/.git`` would report the
    wrong tree healthy, and charter runs from worktrees all day."""

    def test_the_git_dir_is_the_worktrees_own(self):
        wt = self.tmp / "wt"
        self.git("worktree", "add", "-q", "-b", "side", str(wt))
        found = gitstate.git_dir_of(wt)
        self.assertIsNotNone(found)
        self.assertEqual(found, (self.plane / ".git" / "worktrees" / "wt").resolve())

    def test_a_lock_there_is_the_one_found(self):
        wt = self.tmp / "wt"
        self.git("worktree", "add", "-q", "-b", "side", str(wt))
        lock = (self.plane / ".git" / "worktrees" / "wt" / gitstate.LOCK_NAME)
        lock.write_bytes(b"")
        self.addCleanup(lock.unlink, missing_ok=True)
        found = gitstate.for_repo(wt)
        self.assertIsNotNone(found)
        self.assertEqual(found.path, lock.resolve())

    def test_a_directory_that_is_not_a_repository_has_no_git_dir(self):
        plain = self.tmp / "plain"
        plain.mkdir()
        self.assertIsNone(gitstate.git_dir_of(plain))
        self.assertIsNone(gitstate.for_repo(plain))


if __name__ == "__main__":
    unittest.main()
