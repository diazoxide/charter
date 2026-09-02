"""`charter save` typed in a workspace clone that is itself a plane committed the OUTER
plane's whole working tree.

`charter clone` puts clones at ``workspaces/<ws>/<repo>``, ``charter.toml`` is a tracked
file, and charter dogfoods on a clone of itself — so ``workspaces/dev/charter`` is a control
plane nested inside a control plane. `root._outermost` resolves outward to the enclosing
plane there, by **#140's deliberate choice**, and that choice is not in question here: the
outer plane is the one holding the vault, the personas and the memory, and #200 measured what
happens when identity lands in the inner one instead.

`charter save` is ``commit_push(config.ROOT, ["add", "-A"], …)``, so from inside the clone it
ran ``git -C <the outer plane> add -A``. Measured on a throwaway plane (a real ``git clone``
into ``workspaces/dev/charter``, and the ``.gitignore`` `charter init` writes):

1. the operator's untracked ``NOTES.txt`` in the outer plane, committed under the agent's
   message, exit 0;
2. the agent's own work in the clone — the only thing it asked to save — not committed at
   all, and the clone's HEAD not moved.

**This is a second route, not a widening of #808's.** `root.tree_of` is path arithmetic on
the ``.git`` *file* git writes into a linked worktree; a clone has a real ``.git``
**directory**, so `tree_of` correctly answers ``None`` here (asserted below, because the
whole shape of this fix rests on it). Widening it to "am I inside any other plane" would
collapse the plane/tree distinction its own docstring exists to draw. So the detector is
`root.nested_plane_in`, which asks the ``workspaces/`` question `enclosing_plane` already
answers, and the guard sits beside #808's in the same `stages_the_whole_tree` block.

**Refuse — not "commit the inner one instead".** The issue offered three answers. Committing
the inner plane would make `charter save` disagree with every other charter command about
which plane it is acting on: `persona remember`, `vault`, the dispatch tally and the
workspace manifest all follow `config.ROOT` outward, and a `save` that went the other way
would split the plane in two. Committing the outer one is the defect. So the refusal names
both trees and lets the caller say which they meant — ADR 0013's second rule, and the shape
#808 took.

**Keyed on the ADD, so plane state still reaches the plane.** `charter save` is the only
caller of `commit_push` that stages the whole tree; reactive memory, the dispatch tally, the
workspace manifest and the version pin all name plane-state files. Those must keep working
from inside a clone, because working inside a clone is the ordinary way work happens on this
plane and every agent doing it writes memory.

**`$CHARTER_ROOT=<clone>` is honoured, for free** — the same property #808 relied on.
`nested_plane_in` asks whether the caller stands in a plane nested inside *the plane being
committed*; under the override the plane being committed IS the one they are standing in, so
the answer is ``None`` and the save proceeds. The refusal prints that hatch, so it has to
work or the advice is a lie.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, root
from tests._isolation import PersonaIso

#: The arrangement the operator hand-maintains in the outer plane's `charter.toml`.
ARRANGEMENT = '[[frame.component]]\nuse = "workspaces"\n'

PLANE_TOML = ('schema = 1\n\n[[forge]]\nkind = "github"\n\n'
              '[frame]\nslots = ["top"]\n\n' + ARRANGEMENT)

#: What `charter init` writes (`commands.py`'s `_GITIGNORE`). Not decoration: without
#: ``/workspaces/*/*`` the outer plane's ``git add -A`` records the clone as a **gitlink**,
#: and a fixture missing this line reports a harm no real plane can reach.
PLANE_GITIGNORE = "/.charter/\n/workspaces/*/*\n!/workspaces/.gitkeep\n"


def _run(fn, *a, **kw):
    """``(rc, everything it said)``. `util` writes to stderr, some commands print to
    stdout; a refusal has to be findable in either."""
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(*a, **kw)
    return rc, out.getvalue() + err.getvalue()


def _save_args(message="agent: unrelated security fix"):
    return SimpleNamespace(message=message, sign=False, no_push=True)


class PlaneInsideAPlane(PersonaIso):
    """An outer plane with a REAL ``git clone`` of itself at ``workspaces/dev/charter``.

    A real clone, for the reason #808's fixture gives about worktrees: what separates this
    case from that one is that a clone has a ``.git`` **directory**, and a fixture that faked
    one would be asserting against this module's idea of git's layout rather than git's.
    """

    def setUp(self) -> None:
        super().setUp()
        # `find_root` consults $CHARTER_ROOT before walking, and the developer running the
        # suite may well have one exported — it would win outright and every case here
        # would resolve their plane instead of the fixture.
        self._env = os.environ.pop(root.ENV_VAR, None)
        if self._env is not None:
            self.addCleanup(os.environ.__setitem__, root.ENV_VAR, self._env)

        self.plane = self.tmp / "plane"
        self.plane.mkdir(parents=True, exist_ok=True)
        (self.plane / "charter.toml").write_text(PLANE_TOML)
        (self.plane / ".gitignore").write_text(PLANE_GITIGNORE)
        (self.plane / "personas").mkdir()
        (self.plane / "personas" / "steward.md").write_text("# steward\n")
        self.git("init", "-q", "-b", "main", ".")
        self.git("config", "user.email", "r@e.invalid")
        self.git("config", "user.name", "r")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", "-A")
        self.git("commit", "-qm", "plane")
        self.base = self.head()

        # Where `charter clone` puts it, and what charter's own dogfooding looks like.
        self.clone = self.plane / "workspaces" / "dev" / "charter"
        self.clone.parent.mkdir(parents=True)
        subprocess.run(["git", "clone", "-q", str(self.plane), str(self.clone)],
                       check=True, capture_output=True, text=True)
        self._git_in(self.clone, "config", "user.email", "a@e.invalid")
        self._git_in(self.clone, "config", "user.name", "a")
        self._git_in(self.clone, "config", "commit.gpgsign", "false")
        self.clone_base = self._git_in(self.clone, "rev-parse", "HEAD")

        # The operator is mid-edit IN THE OUTER PLANE: the arrangement half-deleted in a
        # tracked file, plus an untracked scratch file. The agent's own work is in the clone.
        (self.plane / "charter.toml").write_text(PLANE_TOML.replace(ARRANGEMENT, ""))
        (self.plane / "NOTES.txt").write_text("operator scratch\n")
        (self.clone / "agent-work.txt").write_text("the agent's own work\n")

        self.addCleanup(config.restore, config.use(self.plane))
        # `config.use` derives `NESTED_ORIGIN` from the ROOT, never the cwd (its own comment
        # says why), so it is `None` throughout this module. Stated, not assumed: a guard
        # written against `config.NESTED_ORIGIN` would pass every test here by never firing.
        self.assertIsNone(config.NESTED_ORIGIN)
        self.assertIn("edm-test-", str(config.STATE_DIR))

    def _git_in(self, where: Path, *argv: str) -> str:
        p = subprocess.run(["git", "-C", str(where), *argv],
                           check=True, capture_output=True, text=True)
        return p.stdout.strip()

    def git(self, *argv: str) -> str:
        return self._git_in(self.plane, *argv)

    def head(self) -> str:
        return self.git("rev-parse", "HEAD")

    def clone_head(self) -> str:
        return self._git_in(self.clone, "rev-parse", "HEAD")

    def staged(self) -> list[str]:
        return [ln for ln in self.git("diff", "--cached", "--name-only").splitlines() if ln]

    def standing_in(self, where: Path):
        """chdir for the rest of the test. Registered rather than a ``finally``, and before
        `PersonaIso`'s rmtree: a cwd left inside a deleted temp directory makes `Path.cwd()`
        raise for every test that runs after this one."""
        here = Path.cwd()
        self.addCleanup(os.chdir, here)
        os.chdir(where)

    def save_from(self, where: Path, **kw):
        self.standing_in(where)
        return _run(commands.cmd_save, _save_args(**kw))


class TestTheFixtureIsTheShapeTheIssueDescribes(PlaneInsideAPlane):
    """Preconditions. Every one of these is a claim the fix rests on, and each would make
    the cases below vacuous if it stopped holding."""

    def test_the_clone_is_itself_a_plane(self):
        self.assertTrue((self.clone / root.MARKER).is_file())

    def test_the_clone_has_a_real_git_directory(self):
        """The single fact that separates this from #808. A linked worktree's ``.git`` is a
        FILE; a clone's is a directory, which is why `tree_of` cannot see this case."""
        self.assertTrue((self.clone / ".git").is_dir())

    def test_the_worktree_detector_correctly_answers_none(self):
        """#808's route, measured rather than assumed. If this ever answered the clone, the
        fix below would be a duplicate and `tree_of` would have swallowed the plane/tree
        distinction it exists to draw."""
        self.assertIsNone(root.main_worktree_of(self.clone))
        self.assertIsNone(root.tree_of(self.plane, self.clone))

    def test_resolution_still_goes_outward(self):
        """#140's settled choice, which this fix does not touch."""
        self.assertEqual(root.find_root(self.clone), self.plane.resolve())

    def test_the_clone_is_ignored_by_the_outer_plane(self):
        """`charter init`'s ``/workspaces/*/*``. Without it the outer ``add -A`` records a
        gitlink and the harm under test is not the one real planes have."""
        proc = subprocess.run(["git", "-C", str(self.plane), "check-ignore",
                               "workspaces/dev/charter"], capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)


class TestSaveFromTheCloneIsRefused(PlaneInsideAPlane):
    """The defect. Each case is one of the two harms the issue measured."""

    def test_it_refuses(self):
        rc, said = self.save_from(self.clone)
        self.assertEqual(rc, 1, said)

    def test_it_refuses_from_a_directory_inside_the_clone(self):
        """The agent is rarely standing exactly on the clone's marker — it is in
        ``charter/`` or ``tests/`` inside it, which is where `charter save` gets typed."""
        deep = self.clone / "charter" / "frame"
        deep.mkdir(parents=True)
        rc, said = self.save_from(deep)
        self.assertEqual(rc, 1, said)

    def test_it_commits_nothing_in_the_outer_plane(self):
        """Harm 1. Without the guard the outer plane's HEAD moves, carrying `NOTES.txt`."""
        self.save_from(self.clone)
        self.assertEqual(self.head(), self.base,
                         "charter save moved the outer plane's HEAD from a nested clone")

    def test_it_stages_nothing_in_the_outer_plane(self):
        """Pins the guard's PLACEMENT, the half the HEAD check cannot see: a refusal that
        has already run ``git add -A`` has done the damage and merely declined to name it —
        the operator's next commit, by any hand, carries it."""
        self.save_from(self.clone)
        self.assertEqual(self.staged(), [],
                         "the outer plane's index was left holding the operator's work")

    def test_the_operators_mid_edit_file_is_left_uncommitted(self):
        """Harm 1 stated as the thing actually lost: an arrangement the operator was in the
        middle of editing, committed as deleted under someone else's message."""
        self.save_from(self.clone)
        self.assertIn("frame.component", self.git("show", "HEAD:charter.toml"))

    def test_the_remedy_points_at_the_tree_that_holds_the_unsaved_work(self):
        """Harm 2, pinned as the pairing it is. A refusal does not *save* the agent's work —
        that file is uncommitted either way, so asserting "agent-work.txt is still untracked"
        would be equally true of the defect and would measure nothing. What the guard owes is
        that the tree it names in the remedy is the tree that really holds the work."""
        _rc, said = self.save_from(self.clone)
        st = self._git_in(self.clone, "status", "--porcelain")
        self.assertIn("agent-work.txt", st)
        self.assertIn(f"git -C {self.clone.resolve()}", said)

    def test_it_leaves_the_clones_own_history_alone(self):
        """The guard refuses; it does not quietly commit the other side instead."""
        self.save_from(self.clone)
        self.assertEqual(self.clone_head(), self.clone_base)


class TestTheRefusalNamesBothTrees(PlaneInsideAPlane):
    """The issue's third option: the only answer that does not silently pick a side.
    Literals are hand-spelled — a test comparing a message against the constant that spells
    it dies to nothing."""

    def setUp(self) -> None:
        super().setUp()
        rc, self.said = self.save_from(self.clone)
        # A precondition, so that if the refusal stops happening these report that rather
        # than quietly matching the success line, which names the plane's path too.
        self.assertEqual(rc, 1, self.said)

    def test_it_names_the_plane_the_caller_is_standing_in(self):
        self.assertIn(str(self.clone.resolve()), self.said)

    def test_it_names_the_plane_it_would_have_committed(self):
        self.assertIn(str(self.plane), self.said)

    def test_it_says_why_the_two_are_different(self):
        """Not decoration. The caller is standing in a directory with a `charter.toml` in
        it; without the word, the refusal reads as a bug in charter rather than as the
        outward resolution doing exactly what it was built to do."""
        self.assertIn("workspaces/", self.said)

    def test_it_offers_git_for_the_callers_own_work(self):
        self.assertIn(f"git -C {self.clone.resolve()}", self.said)

    def test_it_offers_the_plane_for_the_planes_own_work(self):
        self.assertIn("charter save", self.said)

    def test_it_names_the_override(self):
        """A guard with no documented override is a guard people uninstall (#370). This one
        is honoured — `TestAnExplicitRootIsHonoured` below runs it.

        Hand-spelled, not `root.ENV_VAR`. Asserting a message against the constant that
        spells it dies to nothing: rename the variable and the message, the docs and the news
        entry all change together while this still passes. The words are the contract —
        somebody is going to copy this line out of a terminal."""
        self.assertIn("CHARTER_ROOT=", self.said)

    def test_it_does_not_call_the_clone_a_worktree(self):
        """The two refusals must not be interchangeable. Telling an agent standing in a
        clone that it is in a linked worktree sends it to `git worktree list`, where it
        finds nothing and concludes charter is confused."""
        self.assertNotIn("linked worktree", self.said)


class TestThePlaneItselfIsUnchanged(PlaneInsideAPlane):
    """The regression half. `nested_plane_in` answers ``None`` for a caller standing in the
    plane being committed, so the ordinary path — which is nearly every caller — is
    untouched."""

    def test_it_commits(self):
        rc, said = self.save_from(self.plane)
        self.assertEqual(rc, 0, said)
        self.assertNotEqual(self.head(), self.base)

    def test_it_committed_the_operators_work(self):
        self.save_from(self.plane)
        self.assertIn("NOTES.txt", self.git("show", "--name-only", "--format=", "HEAD"))

    def test_it_says_nothing_about_a_nested_plane(self):
        _rc, said = self.save_from(self.plane)
        self.assertNotIn("Refusing to stage", said)


class TestAnOrdinaryRepoCloneIsNotRefused(PlaneInsideAPlane):
    """The narrowness that keeps this from becoming "refuse from anywhere under
    ``workspaces/``".

    Most clones in a workspace are not planes. Standing in one, `charter save` commits the
    plane — which is what it says it does, and there is no second plane for it to have meant.
    Refusing there would break the documented command for the ordinary case, on a plane whose
    workspaces are full of ordinary repos.
    """

    def test_a_clone_without_a_marker_still_saves_the_plane(self):
        repo = self.plane / "workspaces" / "dev" / "widget"
        repo.mkdir(parents=True)
        (repo / "README.md").write_text("not a plane\n")
        rc, said = self.save_from(repo)
        self.assertEqual(rc, 0, said)
        self.assertIn("NOTES.txt", self.git("show", "--name-only", "--format=", "HEAD"))


class TestAnExplicitRootIsHonoured(PlaneInsideAPlane):
    """``$CHARTER_ROOT=<clone>`` — the hatch the refusal itself prints.

    An explicit override is a person saying "I mean this plane", and the plane they mean is
    the one they are standing in, so `save` commits it. The harm this module refuses —
    committing a tree the caller is not in — is not reachable this way, which is why the
    guard is keyed on the relationship between the root and the cwd rather than on "is this
    plane nested".
    """

    def test_it_commits_the_clones_own_tree(self):
        self.addCleanup(config.restore, config.use(self.clone))
        self.standing_in(self.clone)
        rc, said = _run(commands.cmd_save, _save_args())
        self.assertEqual(rc, 0, said)
        self.assertIn("agent-work.txt",
                      self._git_in(self.clone, "show", "--name-only", "--format=", "HEAD"))

    def test_it_leaves_the_outer_plane_alone(self):
        self.addCleanup(config.restore, config.use(self.clone))
        self.standing_in(self.clone)
        _run(commands.cmd_save, _save_args())
        self.assertEqual(self.head(), self.base)
        self.assertEqual(self.staged(), [])


class TestPlaneStateStillReachesThePlane(PlaneInsideAPlane):
    """The half that must NOT change, and the reason the guard is keyed on the add.

    Reactive memory, the dispatch tally, the workspace manifest and the version pin all
    reach `commit_push` with an add scoped to named plane-state files. Working inside a
    clone is the ordinary way work happens on this plane, and every agent doing it writes
    memory; refusing those would be a far larger harm than the one being fixed.
    """

    def test_a_scoped_add_still_commits_from_the_clone(self):
        from charter import planegit
        (self.plane / "memo.md").write_text("a memory\n")
        self.standing_in(self.clone)
        rc, said = _run(planegit.commit_push, self.plane, ["add", "--", "memo.md"],
                        "persona: remember", no_push=True)
        self.assertEqual(rc, 0, said)
        self.assertIn("memo.md", self.git("show", "--name-only", "--format=", "HEAD"))


class TestTheDetector(PlaneInsideAPlane):
    """`root.nested_plane_in` on its own, including the shapes no `cmd_save` case reaches."""

    def _plane_at(self, *parts: str) -> Path:
        d = self.tmp.joinpath(*parts)
        d.mkdir(parents=True, exist_ok=True)
        (d / root.MARKER).write_text("schema = 1\n")
        return d

    def test_it_answers_the_clone(self):
        self.assertEqual(root.nested_plane_in(self.plane, self.clone), self.clone.resolve())

    def test_it_answers_none_standing_in_the_plane_itself(self):
        self.assertIsNone(root.nested_plane_in(self.plane, self.plane))

    def test_it_answers_none_for_a_directory_that_is_in_no_plane(self):
        bare = self.tmp / "bare"
        bare.mkdir()
        self.assertIsNone(root.nested_plane_in(self.plane, bare))

    def test_it_answers_none_for_an_unrelated_plane(self):
        """The narrowness `enclosing_plane` already has: a plane somewhere else on disk is
        not nested inside this one, and committing this one from there is not this defect."""
        other = self._plane_at("elsewhere")
        self.assertIsNone(root.nested_plane_in(self.plane, other))

    def test_it_answers_none_when_the_nesting_is_not_through_workspaces(self):
        """A ``charter.toml`` below the plane but not under its ``workspaces/`` is not a
        clone charter made; `enclosing_plane` has never claimed it and neither does this."""
        stray = self.plane / "docs" / "example"
        stray.mkdir(parents=True)
        (stray / root.MARKER).write_text("schema = 1\n")
        self.assertIsNone(root.nested_plane_in(self.plane, stray))

    def test_it_reaches_through_a_chain(self):
        """A plane inside a plane inside a plane — the shape `_outermost` loops for. Asked
        of the MIDDLE plane, so the answer cannot come from "outermost wins": under
        ``$CHARTER_ROOT=<mid>`` the tree being committed is `mid`, and standing in `leaf`
        commits a tree the caller is not in exactly as before."""
        top = self._plane_at("top")
        mid = self._plane_at("top", "workspaces", "a", "mid")
        leaf = self._plane_at("top", "workspaces", "a", "mid", "workspaces", "b", "leaf")
        self.assertEqual(root.nested_plane_in(top, leaf), leaf.resolve())
        self.assertEqual(root.nested_plane_in(mid, leaf), leaf.resolve())
        self.assertIsNone(root.nested_plane_in(leaf, leaf))

    def test_it_answers_the_innermost_plane_not_an_intermediate_one(self):
        """The caller is standing in `leaf`; naming `mid` in the remedy would send them to
        run git in a tree that is not theirs either."""
        top = self._plane_at("top2")
        self._plane_at("top2", "workspaces", "a", "mid")
        leaf = self._plane_at("top2", "workspaces", "a", "mid", "workspaces", "b", "leaf")
        self.assertEqual(root.nested_plane_in(top, leaf), leaf.resolve())

    def test_it_never_raises_on_a_path_that_is_gone(self):
        """It sits on a command path, beside `tree_of`, which makes the same promise.

        A missing path does not actually THROW — `Path.resolve` tolerates it and `is_file`
        answers False — so this pins the ANSWER and nothing else.
        `TestTheDetectorNeverRaises` is where the catches are pinned."""
        self.assertIsNone(root.nested_plane_in(self.plane, self.tmp / "nope" / "gone"))
        self.assertIsNone(root.nested_plane_in(self.tmp / "nope" / "gone", self.clone))


class TestTheDetectorNeverRaises(PlaneInsideAPlane):
    """The "never raises" promise, made observable.

    Every one of these was a deletion-sweep SURVIVOR before it existed: the catches sat over
    calls that no fixture could make fail, so `narrow-except` changed nothing anywhere and
    the promise was asserted only by paths that never throw. A hostile filesystem is not
    exotic on the path this sits on — an unreadable ancestor directory, a cwd deleted out
    from under the process, a symlink loop — and this runs inside `charter save`, where an
    escaping `OSError` is a traceback instead of a refusal.

    Each case makes exactly ONE of the three calls throw, so the three catches are pinned
    separately rather than by one blanket failure.
    """

    def _raising(self, module, attr, exc=OSError("unreadable")):
        """The shape `tests/test_doctor_absent_is_not_health.py` uses: swap a module
        attribute for one that throws, and put it back afterwards."""
        def boom(*a, **kw):
            raise exc
        real = getattr(module, attr)
        setattr(module, attr, boom)
        self.addCleanup(setattr, module, attr, real)

    def test_a_cwd_that_no_longer_exists(self):
        """`Path.cwd()` raises `FileNotFoundError` when the process's working directory has
        been deleted — the OSError half of the first catch. Reached with ``start=None``,
        which is how `commit_push` calls it."""
        with mock.patch.object(Path, "cwd", side_effect=FileNotFoundError("gone")):
            self.assertIsNone(root.nested_plane_in(self.plane))

    def test_a_symlink_loop_under_the_starting_directory(self):
        """`Path.resolve()` raises `RuntimeError`, not `OSError`, on a symlink loop — the
        other half of the first catch, and the reason it names two exception types. Narrow
        it to `OSError` alone and this is a traceback out of `charter save`."""
        with mock.patch.object(Path, "resolve", side_effect=RuntimeError("symlink loop")):
            self.assertIsNone(root.nested_plane_in(self.plane, self.clone))

    def test_an_ancestor_directory_that_cannot_be_read(self):
        """`Path.is_file()` propagates `PermissionError` (it swallows only
        ENOENT/ENOTDIR/EBADF/ELOOP, never EACCES) — the second catch, over the marker walk.
        One unreadable directory between the caller and the plane is enough."""
        with mock.patch.object(Path, "is_file", side_effect=PermissionError("denied")):
            self.assertIsNone(root.nested_plane_in(self.plane, self.clone))

    def test_the_walk_outward_hitting_an_unreadable_directory(self):
        """The third catch, over `enclosing_plane` — which does its own `is_file` stat on
        each ancestor and is documented as able to throw. Reached only from a caller that
        IS in a nested plane, so the loop is entered before it fails."""
        self._raising(root, "enclosing_plane")
        self.assertIsNone(root.nested_plane_in(self.plane, self.clone))

    def test_the_refusal_path_survives_it_too(self):
        """The contract that matters to the caller: `charter save` degrades to committing
        the plane, not to a traceback. `nested_plane_in` answering `None` means "no nested
        plane I can see", and an unreadable tree is exactly that."""
        self._raising(root, "enclosing_plane")
        rc, said = self.save_from(self.clone)
        self.assertEqual(rc, 0, said)
        self.assertNotIn("Traceback", said)


if __name__ == "__main__":
    unittest.main()
