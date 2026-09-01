"""`charter save` typed in a linked worktree committed the PLANE's whole working tree.

`root._plane_of` sends a linked worktree's plane back to the tree it was cut from, which is
right for identity — a worktree is a view of a plane's repo, not a second plane. `charter
save` is ``commit_push(config.ROOT, ["add", "-A"], …)``, so from a worktree it ran ``git -C
<the plane's clone> add -A`` and committed **that** tree. Three harms in one call, none
announced:

1. the operator's mid-edit files in the plane, committed under the agent's message;
2. scratch the agent had never seen, committed with it;
3. the agent's own work — the only thing it asked to save — not committed at all.

Measured on a throwaway plane, in both layouts a worktree of a plane is found in: beside the
plane, and at ``<plane>/.claude/worktrees/<agent>``, which is the layout ``.gitignore``'s last
line says the harness uses. `git worktree list` on the operator's own clone showed 22 live
worktrees while this was being written, and the plane's ``charter.toml`` was dirty.

**The refusal is keyed on the ADD, not on the command.** The property that makes a call
dangerous is that it stages files the caller never named and, from another working tree,
cannot see — so the guard lives in `planegit.commit_push` where the root and the add meet,
and covers any future caller that copies `charter save`'s shape. Every other caller of
`commit_push` scopes its add
to named plane-state files (reactive memory, the dispatch tally, the workspace manifest, the
version pin). Those are correct from a worktree and stay allowed: plane state belongs to the
plane and has no worktree copy that would be right. Refusing them would stop an agent working
in a worktree from recording memory at all — a larger harm than the one being fixed — and
would take the rebase-on-rejection path (`planegit.push_head`) down with it.

**`$CHARTER_ROOT` is honoured, for free.** The guard asks `root.tree_of(root)`: is the caller
standing in a linked worktree *of the tree being committed*? With ``$CHARTER_ROOT=<worktree>``
— a hatch `personas/release/memory/` actively instructs agents to set — the tree being
committed IS the caller's own tree, no ancestor of the cwd is a worktree of it, and the answer
is ``None``. An explicit override is a person saying "I mean this tree", and the tree they
mean is the one they are in, so the harm this refuses is not reachable through it.

**And the success line names the tree.** ``✓ Committed <sha>: …`` named a sha and a count and
no path, which is why the defect survived months of daily use in a repo full of worktrees.
"""

from __future__ import annotations

import io
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config, planegit, root
from tests._isolation import PersonaIso

#: The arrangement the operator hand-maintains in the plane's `charter.toml`. Half-deleted in
#: the fixture below, exactly as a mid-edit file is, so a `git add -A` in the plane commits a
#: deletion nobody asked for — #726's original symptom.
ARRANGEMENT = "[[frame.component]]\nuse = \"workspaces\"\n"

PLANE_TOML = ("schema = 1\n\n[[forge]]\nkind = \"github\"\n\n"
              "[frame]\nslots = [\"top\"]\n\n" + ARRANGEMENT)


def _run(fn, *a, **kw):
    """Call *fn*, returning ``(rc, everything it said)``. `util` writes to stderr, some
    commands print to stdout; a refusal has to be findable in either."""
    out, err = io.StringIO(), io.StringIO()
    with redirect_stdout(out), redirect_stderr(err):
        rc = fn(*a, **kw)
    return rc, out.getvalue() + err.getvalue()


def _save_args(message="agent: unrelated security fix"):
    return SimpleNamespace(message=message, sign=False, no_push=True)


class PlaneWithWorktrees(PersonaIso):
    """A real plane repository with two real linked worktrees over it.

    Real git, for the reason `tests/test_the_charter_a_sub_agent_reads_is_the_one_that_was_
    written.py` gives: the redirect under test is decided by a ``.git`` FILE that only git
    writes, and a fixture that wrote one by hand would be asserting against this module's
    idea of git's layout rather than against git's.
    """

    def setUp(self) -> None:
        super().setUp()
        self.plane = self.tmp / "plane"
        self.plane.mkdir(parents=True, exist_ok=True)
        (self.plane / "charter.toml").write_text(PLANE_TOML)
        (self.plane / "personas").mkdir()
        (self.plane / "personas" / "steward.md").write_text("# steward\n")
        self.git("init", "-q", "-b", "main", ".")
        self.git("config", "user.email", "r@e.invalid")
        self.git("config", "user.name", "r")
        self.git("config", "commit.gpgsign", "false")
        self.git("add", "-A")
        self.git("commit", "-qm", "plane")
        self.base = self.head()

        # Beside the plane, and the harness's own layout. Both are worktrees of the same
        # repo; nothing below may depend on which.
        self.beside = self.tmp / "wt"
        self.git("worktree", "add", "-q", "-b", "beside", str(self.beside))
        self.nested = self.plane / ".claude" / "worktrees" / "agent"
        self.git("worktree", "add", "-q", "-b", "nested", str(self.nested))

        # The operator is mid-edit IN THE PLANE: the arrangement half-deleted in a tracked
        # file, plus an untracked scratch file. Neither was saved, and neither is the
        # agent's.
        (self.plane / "charter.toml").write_text(PLANE_TOML.replace(ARRANGEMENT, ""))
        (self.plane / "NOTES.txt").write_text("operator scratch\n")
        for wt in (self.beside, self.nested):
            (wt / "agent-work.txt").write_text("the agent's own work\n")

        self.addCleanup(config.restore, config.use(self.plane))

    def git(self, *argv: str) -> str:
        p = subprocess.run(["git", "-C", str(self.plane), *argv],
                           check=True, capture_output=True, text=True)
        return p.stdout.strip()

    def head(self) -> str:
        return self.git("rev-parse", "HEAD")

    def staged(self) -> list[str]:
        return [ln for ln in self.git("diff", "--cached", "--name-only").splitlines() if ln]

    def standing_in(self, where: Path):
        """chdir to *where* for the rest of the test. Registered rather than a ``finally``,
        and before `PersonaIso`'s rmtree: a cwd left inside a deleted temp directory makes
        `Path.cwd()` raise for every test that runs after this one."""
        here = Path.cwd()
        self.addCleanup(os.chdir, here)
        os.chdir(where)

    def save_from(self, where: Path, **kw):
        self.standing_in(where)
        return _run(commands.cmd_save, _save_args(**kw))


class TestSaveFromAWorktreeIsRefused(PlaneWithWorktrees):
    """The defect, from both layouts. Each assertion is one of the three harms."""

    def test_beside_the_plane(self):
        rc, said = self.save_from(self.beside)
        self.assertEqual(rc, 1, said)

    def test_the_harness_layout(self):
        """`.gitignore`'s last line says the harness puts worktrees under the plane itself,
        so a guard that only recognised a sibling would miss every real case."""
        rc, said = self.save_from(self.nested)
        self.assertEqual(rc, 1, said)

    def test_it_commits_nothing_in_the_plane(self):
        for wt in (self.beside, self.nested):
            with self.subTest(worktree=wt.name):
                self.save_from(wt)
                self.assertEqual(self.head(), self.base,
                                 "charter save moved the plane's HEAD from a worktree")

    def test_it_stages_nothing_in_the_plane(self):
        """Pins the guard's PLACEMENT, which is the half `test_it_commits_nothing_in_the
        _plane` cannot see: a refusal that has already run ``git add -A`` has done the damage
        and merely declined to name it — the operator's next commit, by any hand, carries
        it, and `git status` in the plane now reads as though they staged it themselves."""
        self.save_from(self.beside)
        self.assertEqual(self.staged(), [],
                         "the plane's index was left holding the operator's work")

    def test_the_operators_mid_edit_file_is_left_uncommitted(self):
        """Harm 1, stated as the thing that was actually lost: an arrangement the operator
        was in the middle of editing, committed as deleted under someone else's message."""
        self.save_from(self.beside)
        committed = self.git("show", "HEAD:charter.toml")
        self.assertIn("frame.component", committed)

    def test_the_remedy_points_at_the_tree_that_holds_the_unsaved_work(self):
        """Harm 3, pinned as the pairing it actually is.

        A refusal does not *save* the agent's work — that file is uncommitted either way, so
        asserting "agent-work.txt is still untracked here" would be equally true of the
        defect and would measure nothing. What the guard owes is that the tree it names in
        the remedy is the tree that really holds the work."""
        _rc, said = self.save_from(self.beside)
        st = subprocess.run(["git", "-C", str(self.beside), "status", "--porcelain"],
                            check=True, capture_output=True, text=True).stdout
        self.assertIn("agent-work.txt", st)
        self.assertIn(f"git -C {self.beside.resolve()}", said)


class TestTheRefusalNamesTheRemedy(PlaneWithWorktrees):
    """#764's shape: a command that cannot do what you meant says so, and says what will."""

    def setUp(self) -> None:
        super().setUp()
        rc, self.said = self.save_from(self.beside)
        # Stated as a precondition, so that if the refusal stops happening these assertions
        # report that rather than quietly matching the success line instead — the plane's
        # path now appears in THAT too.
        self.assertEqual(rc, 1, self.said)

    def test_it_names_the_tree_the_caller_is_in(self):
        self.assertIn(str(self.beside.resolve()), self.said)

    def test_it_names_the_plane_it_would_have_committed(self):
        """``config.ROOT`` as this invocation resolved it, verbatim — the same string every
        other message in `planegit` prints, so the reader can match them up."""
        self.assertIn(str(self.plane), self.said)

    def test_it_offers_git_for_the_callers_own_work(self):
        """The caller almost certainly meant their own changes, which live here and which
        charter has no business committing for them."""
        self.assertIn(f"git -C {self.beside.resolve()}", self.said)

    def test_it_offers_the_plane_for_the_planes_own_work(self):
        self.assertIn("charter save", self.said)

    def test_it_names_the_override(self):
        """`$CHARTER_ROOT=<worktree>` is a documented hatch, honoured below. A guard with no
        documented override is a guard people uninstall (#370)."""
        self.assertIn(root.ENV_VAR, self.said)


class TestThePlaneItselfIsUnchanged(PlaneWithWorktrees):
    """The regression half. `tree_of` answers ``None`` for a caller standing in the plane,
    so the ordinary path — which is every non-worktree caller — must be untouched."""

    def test_it_commits(self):
        rc, said = self.save_from(self.plane)
        self.assertEqual(rc, 0, said)
        self.assertNotEqual(self.head(), self.base)

    def test_it_committed_the_operators_work(self):
        self.save_from(self.plane)
        self.assertIn("NOTES.txt", self.git("show", "--name-only", "--format=", "HEAD"))

    def test_it_says_nothing_about_a_worktree(self):
        _rc, said = self.save_from(self.plane)
        self.assertNotIn("linked worktree", said)


class TestTheSuccessLineNamesTheTree(PlaneWithWorktrees):
    """The line that would have made this visible months ago. It named a sha and a count and
    no path, so a commit in the wrong tree read exactly like a commit in the right one."""

    def test_the_committed_line_names_the_tree(self):
        _rc, said = self.save_from(self.plane)
        self.assertIn(str(self.plane), said)

    def test_it_still_names_the_sha_and_the_count(self):
        _rc, said = self.save_from(self.plane)
        self.assertIn(self.head()[:7], said)
        self.assertIn("file(s)", said)


class TestAnExplicitRootIsHonoured(PlaneWithWorktrees):
    """``$CHARTER_ROOT=<worktree>`` — the hatch `personas/release/memory/editing-personas-
    release-persona-md-in-a-workspa.md` instructs agents to set.

    An explicit override is a person saying "I mean this tree", and the tree they mean is the
    one they are standing in, so `save` commits it. The harm this module refuses — committing
    a tree the caller is not in — is not reachable this way, which is the whole reason the
    guard is keyed on the relationship between the root and the cwd rather than on "is this a
    worktree".
    """

    def test_it_commits_the_worktrees_own_tree(self):
        self.addCleanup(config.restore, config.use(self.beside))
        self.standing_in(self.beside)
        rc, said = _run(commands.cmd_save, _save_args())
        self.assertEqual(rc, 0, said)
        wt_head = subprocess.run(["git", "-C", str(self.beside), "show", "--name-only",
                                  "--format=", "HEAD"], check=True, capture_output=True,
                                 text=True).stdout
        self.assertIn("agent-work.txt", wt_head)

    def test_it_leaves_the_plane_alone(self):
        self.addCleanup(config.restore, config.use(self.beside))
        self.standing_in(self.beside)
        _run(commands.cmd_save, _save_args())
        self.assertEqual(self.head(), self.base)
        self.assertEqual(self.staged(), [])


class TestPlaneStateStillReachesThePlane(PlaneWithWorktrees):
    """The half that must NOT change, and the reason the guard is keyed on the add.

    Reactive memory, the dispatch tally, the workspace manifest and the version pin all reach
    `commit_push` with an add scoped to named plane-state files. Those files belong to the
    plane and have no worktree copy that would be right, so they must keep working from a
    worktree — every agent working in one writes memory. This is also where `push_head`'s
    ``rebase FETCH_HEAD`` stays reachable, deliberately: it replays commits charter itself
    made of plane state onto origin's copy of the plane's own branch, and `_land_via_branch`
    — the other rejection path — never moves the plane's HEAD at all.
    """

    def test_a_scoped_add_still_commits_from_a_worktree(self):
        (self.plane / "memo.md").write_text("a memory\n")
        self.standing_in(self.beside)
        rc, said = _run(planegit.commit_push, self.plane, ["add", "--", "memo.md"],
                        "memory: a memory", no_push=True)
        self.assertEqual(rc, 0, said)
        self.assertIn("memo.md", self.git("show", "--name-only", "--format=", "HEAD"))

    def test_a_scoped_add_does_not_sweep_up_the_operators_work(self):
        """The property that makes it safe, asserted rather than assumed: a scoped add
        commits what it named and nothing else."""
        (self.plane / "memo.md").write_text("a memory\n")
        self.standing_in(self.beside)
        rc, said = _run(planegit.commit_push, self.plane, ["add", "--", "memo.md"], "m",
                        no_push=True)
        # Both halves, or this passes for the wrong reason: a refusal commits nothing, so
        # "NOTES.txt is not in HEAD" is true of a call that did nothing at all.
        self.assertEqual(rc, 0, said)
        files = self.git("show", "--name-only", "--format=", "HEAD").split()
        self.assertEqual(files, ["memo.md"])


class TestTheGuardIsOnTheAddNotTheCommand(PlaneWithWorktrees):
    """`cmd_save` is the only caller that stages the whole tree today. The guard is in
    `commit_push` so that the next one to copy that shape is refused too, rather than
    re-deriving this defect in a command nobody thought to look at."""

    def test_commit_push_refuses_an_unscoped_add_from_a_worktree(self):
        self.standing_in(self.beside)
        rc, said = _run(planegit.commit_push, self.plane, ["add", "-A"], "m", no_push=True)
        self.assertEqual(rc, 1, said)
        self.assertEqual(self.head(), self.base)


class TestWhichAddsStageTheWholeTree(unittest.TestCase):
    """The predicate, on its own. It decides whether a call can commit something the caller
    cannot see, so it is worth pinning apart from any caller that happens to exist.

    Every case here was measured against real git first — one edited tracked file and one
    untracked file in a throwaway repo — because the first draft of this predicate was
    written from what the flags *look* like they mean and got two of them backwards.
    """

    def test_add_dash_a_with_no_pathspec(self):
        self.assertTrue(planegit.stages_the_whole_tree(["add", "-A"]))

    def test_add_dash_u_with_no_pathspec(self):
        """``-u`` stages only tracked modifications — which is exactly the operator's
        mid-edit `charter.toml`, so it is no safer than ``-A`` for this purpose."""
        self.assertTrue(planegit.stages_the_whole_tree(["add", "-u"]))

    def test_a_dot_is_the_whole_tree_too(self):
        """``git -C <root> add .`` stages all of *root*, pathspec or not — the same call
        wearing a different spelling."""
        self.assertTrue(planegit.stages_the_whole_tree(["add", "."]))
        self.assertTrue(planegit.stages_the_whole_tree(["add", "--", ":/"]))

    def test_named_paths_are_scoped(self):
        self.assertFalse(planegit.stages_the_whole_tree(["add", "--", "memo.md"]))

    def test_a_bare_separator_bounds_nothing(self):
        """Measured: `git add -A --` stages the whole tree, exactly as `git add -A` does.
        The first draft of this predicate read the separator as a bound and would have
        waved that call straight through the guard it is the whole point of."""
        self.assertTrue(planegit.stages_the_whole_tree(["add", "-A", "--"]))

    def test_no_pathspec_and_no_widening_flag_stages_nothing(self):
        """git's own "Nothing specified, nothing added" — measured. Every scoped caller
        builds its list by splatting (``["add", "--", *paths]``), so an empty list produces
        this exact shape, and reading it as "the whole tree" would turn a harmless no-op
        into a refusal, from a worktree only, on the one path nobody would think to test."""
        self.assertFalse(planegit.stages_the_whole_tree(["add", "--"]))
        self.assertFalse(planegit.stages_the_whole_tree(["add"]))

    def test_dash_a_with_named_paths_is_still_scoped(self):
        """`commands_workspace.cmd_workspace_add` passes ``add -A -- <paths>``: the ``-A``
        widens what happens TO those paths, it does not widen the paths."""
        self.assertFalse(planegit.stages_the_whole_tree(
            ["add", "-A", "--", "workspaces/x", ".gitignore"]))

    def test_one_whole_tree_spelling_widens_the_whole_list(self):
        """Measured: `git add -- . u.txt` stages the whole tree. The named path beside it
        changes nothing, so the question is whether ANY pathspec is a whole-tree spelling,
        not whether they all are. The deletion sweep found this — the shipped `all` survived
        every case above, because none of them mixed the two kinds."""
        self.assertTrue(planegit.stages_the_whole_tree(["add", "--", ".", "memo.md"]))
        self.assertTrue(planegit.stages_the_whole_tree(["add", "--", "memo.md", ":/"]))

    def test_a_pathspec_named_like_a_flag_is_a_path_past_the_separator(self):
        """The reason the split is at ``--`` rather than on "does it start with a dash", and
        the only shape in which that split is observable at all: a file really can be called
        ``-A``, and past the separator git stages the file. Read as a flag instead, this
        would be refused as a whole-tree add. `TestThePredicateAgreesWithGit` measures the
        git half; the sweep reported the branch as a survivor until both existed."""
        self.assertFalse(planegit.stages_the_whole_tree(["add", "--", "-A"]))
        self.assertFalse(planegit.stages_the_whole_tree(["add", "--", "-weird"]))


class TestThePredicateAgreesWithGit(unittest.TestCase):
    """The predicate is a claim **about git**, so it is checked against git.

    This class exists because the first draft was written from what the flags look like they
    mean, and got two of them backwards: it read `add -A --` as bounded by the separator (git
    stages the whole tree) and bare `add` as unbounded (git stages nothing). The unit tests
    above, written from the same belief, agreed with it — which is the whole failure mode.

    The ground truth is deliberately *not* "did git stage two files": it is **did git stage a
    file the argv never named**, which is the property the refusal is protecting. A caller
    that names its paths is safe from another working tree however many files it touches; a
    caller that names none and stages any is not.
    """

    #: Each argv, run for real. One edited TRACKED file and one UNTRACKED file, so `-A` and
    #: `-u` are distinguishable — they are not the same answer and the predicate says so.
    CASES = (["add"], ["add", "--"], ["add", "-A"], ["add", "-A", "--"], ["add", "-u"],
             ["add", "-u", "--"], ["add", "."], ["add", "--", "u.txt"],
             ["add", "-A", "--", "t.txt"], ["add", "--", ":/"],
             # A whole-tree spelling BESIDE a named path. The sweep found the predicate
             # reading this as scoped; git stages everything.
             ["add", "--", ".", "u.txt"], ["add", "--", ":/", "u.txt"])

    def setUp(self) -> None:
        self.repo = Path(tempfile.mkdtemp(prefix="edm-addprobe-"))
        self.addCleanup(shutil.rmtree, self.repo, ignore_errors=True)
        self.git("init", "-q", "-b", "main", ".")
        self.git("config", "user.email", "r@e.invalid")
        self.git("config", "user.name", "r")
        (self.repo / "t.txt").write_text("tracked\n")
        self.git("add", "t.txt")
        self.git("-c", "commit.gpgsign=false", "commit", "-qm", "base")
        (self.repo / "t.txt").write_text("tracked\nedited\n")
        (self.repo / "u.txt").write_text("untracked\n")

    def git(self, *argv: str) -> str:
        p = subprocess.run(["git", "-C", str(self.repo), *argv],
                           capture_output=True, text=True)
        return p.stdout

    def test_a_file_named_like_a_flag_is_staged_as_a_file(self):
        """`git add -- -A`, with a file called `-A` in the tree, stages that file and
        nothing else — measured. So the predicate must read it as a named path, not as the
        whole-tree flag it is spelled like."""
        (self.repo / "-A").write_text("weird\n")
        self.git("reset", "-q")
        self.git("add", "--", "-A")
        staged = set(self.git("diff", "--cached", "--name-only").split())
        self.assertEqual(staged, {"-A"}, "git did not stage the FILE named -A")
        self.assertFalse(planegit.stages_the_whole_tree(["add", "--", "-A"]))

    def test_it_says_what_git_does(self):
        for argv in self.CASES:
            with self.subTest(argv=" ".join(argv)):
                self.git("reset", "-q")
                self.git(*argv)
                staged = {ln for ln in self.git("diff", "--cached", "--name-only").split()}
                named = set(argv) & {"t.txt", "u.txt"}
                git_says = bool(staged - named)
                self.assertEqual(planegit.stages_the_whole_tree(list(argv)), git_says,
                                 f"git staged {sorted(staged) or 'nothing'} for "
                                 f"`git add {' '.join(argv[1:])}`, of which "
                                 f"{sorted(staged - named) or 'nothing'} was never named")


if __name__ == "__main__":
    unittest.main()
