"""`git checkout <path>` restores a file; the plane-root guard read it as a branch switch
and refused it (#461).

`git checkout` is two commands wearing one name. ``git checkout <rev>`` moves HEAD;
``git checkout <pathspec>`` restores files and moves nothing — the same operation
``git restore <pathspec>`` performs, which this guard has always allowed. So charter was not
holding a policy against restoring a file in the plane root; it was refusing **one of the
two ways git spells an operation it permits**, which is a false positive.

Worse than the inconvenience: the denial was confident, detailed, and *wrong about what the
command does*. An operator following its advice would go and create a workspace clone in
order to restore one file, and the message's escape hatch — "run it yourself, in your own
terminal" — is the right answer for a guard that is correctly refusing an agent and the
wrong outcome for one that has simply misread the command.

**Every row below is a real git invocation whose effect was measured, not assumed.** Each
was run against git 2.50 in a scratch repo with HEAD recorded before and after; the
docstrings quote what git actually answered. That is the shape #401 established for
`reset`, and it is the only way to tell a guard's decision from git's behaviour — the two
had drifted apart here precisely because nothing compared them.

The ambiguous case is asserted as **still denied**, so the fix cannot be over-applied into
"any operand that happens to name a file is safe".

**Round two.** The first cut of this read the operand and not the options, and `git checkout
--orphan README` — a branch creation whose operand is a tracked file — walked through the
restore carve-out. `origin/main` refused it: the fix was strictly weaker than the code it
replaced, on the very command it is named after. Every row here asserted a verdict somebody
had thought of, so a spelling nobody thought of had no row at all, and that is the defect
`TestGitItselfIsTheOracle` closes: it runs each spelling against real git in a throwaway
copy of this fixture and requires that anything which MOVED HEAD is denied. The expected
verdict is not written down; git supplies it.

**Round three, and the same shape one level up.** Asking git for the verdict fixed who
decides; it did not fix who chooses the questions, and the corpus was still 41 rows a person
had typed. Both holes round three found are COMBINATIONS of axes that were each already
covered: `--detach` had rows and `main` had rows, but `git checkout --detach main` — which
walked through the "returning to the default branch is always allowed" carve-out and past
every line of the `--detach` handling — had none; `git switch` had rows and `--` had rows,
but `git switch -- feature` had none, and `switch` has no path half for a separator to
introduce. So the corpus is now the PRODUCT of its axes (`_generated_corpus`), and a second
one crosses commands with every ROUTE to the plane root — where a relative `git -C` was being
resolved against the hook process's cwd rather than the shell's.

**Round four: the baseline moved into the subject.** Round three added a third cross-check
that loaded `charter/hooks.py` from `origin/main` at runtime and required that nothing the
shipped guard refused was allowed here. It held exactly once. The moment the PR carrying it
merged, `origin/main` *was* the code under test, baseline and subject became byte-identical,
its "at least four relaxations" assertion could never be satisfied again, and `main` went red
for every branch cut from it (#482).

That is not a bug in the assertion; it is what a differential test against a MOVING
reference always becomes. The property was worth keeping — *charter's plane-root guard must
never become weaker than it was* — so it is expressed the way `test_vault_path_re_only_widens`
expresses its own: as a VENDORED corpus (`PINNED` below). Every row is a real git invocation
paired with the verdict the guard must give, checked in beside the code. Nothing resolves a
ref, nothing reads git history, nothing leaves the process; the file IS the baseline, so it
cannot drift into the thing it measures, and a relaxation flips a row red with a name that
says which spelling regressed.
"""

from __future__ import annotations

import itertools
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook


#: ---------------------------------------------------------------------------------------
#: The corpus, as a PRODUCT of axes rather than a list somebody typed.
#:
#: Every bypass this guard has had was a spelling nobody thought to type: `--orphan README`,
#: `-bREADME`, `-fq`, `git checkout --detach main`, `git switch -- feature`. A written list
#: is only ever as long as the last audit, and round two's oracle inherited that shape — it
#: asked real git for the verdict, which was the right idea, but it asked about 41 rows a
#: person had chosen. Two of the three holes round three found are combinations of axes that
#: were each already present: `--detach` had rows, `main` had rows, and `--detach main` had
#: none.
#:
#: So the axes are named and crossed. Adding one option or one operand adds every
#: combination of it, and no combination can be added with the wrong expected answer,
#: because the expected answer is not written down at all — git supplies it.
_SUBS = ("checkout", "switch")

#: One witness per option CLASS the rule knows — none, restore-only, detach, create, and an
#: option charter cannot place — in each spelling git's parser accepts: long, short, and a
#: cluster in BOTH orders, since a cluster is read left to right and `-qd` meets a different
#: letter first than `-dq`.
#:
#: `-B` and `-C` are here since #483, and they are the axis that issue is really about: a
#: short option whose LETTER is also the name of one of git's own globals. Crossing them
#: with the operands is what produces `git switch -C feature` without anyone deciding to
#: type it.
_OPTS = ("", "-q", "--ours", "--detach", "-d", "-qd", "-dq", "-b", "-B", "-C", "--orphan")

#: One witness per operand CLASS: the plane's DEFAULT BRANCH — the remedy's name, and the
#: one the carve-out used to hand a free pass to whatever stood beside it — another branch,
#: a name that is both a branch and a tracked file, and a tracked file.
_OPERANDS = ("main", "feature", "ambig", "README")

#: Where `--` goes. It is an axis and not a footnote because it does not mean the same thing
#: on both subcommands: `checkout` has a path half for a separator to introduce and `switch`
#: has none, so `git switch -- feature` is a branch move wearing a restore's punctuation.
_SHAPES = ("git {sub} {opt} {op}", "git {sub} {opt} {op} --", "git {sub} {opt} -- {op}")


def _generated_corpus() -> tuple[str, ...]:
    """Every ``{sub} × {opt} × {operand} × {shape}``, deduplicated, order preserved."""
    out: dict[str, None] = {}
    for sub, opt, op, shape in itertools.product(_SUBS, _OPTS, _OPERANDS, _SHAPES):
        out[" ".join(shape.format(sub=sub, opt=opt, op=op).split())] = None
    return tuple(out)


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class CheckoutCase(PersonaIso):
    """A plane root that is a REAL git repo, because the guard now asks git what an operand
    is. A fixture that only pretended to be a repo would make every operand unresolvable,
    every answer a denial, and the whole file unfalsifiable.

    Four names, chosen so each bucket of the rule has a witness:
      * ``README``  — a tracked path, and no ref of that name.
      * ``feature`` — a branch, and no path of that name.
      * ``ambig``   — BOTH: a tracked file and a branch. The one git resolves in favour of
                      the branch, and the one that must stay denied.
      * ``main``    — the default branch, the documented remedy.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        self.build_repo(self.root)

    def build_repo(self, where: Path) -> None:
        """The fixture, as a function, so the oracle in `TestGitItselfIsTheOracle` can run
        real git commands against the SAME repo the guard is judging rather than a
        hand-rolled second one that could drift from it."""
        where.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(where))
        (where / "README").write_text("plane\n")
        (where / "ambig").write_text("both\n")
        (where / "notes").mkdir(exist_ok=True)
        (where / "notes" / "a.md").write_text("a\n")
        self._in(where, "add", "-A")
        self._in(where, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
                 "-c", "user.name=t", "commit", "-qm", "init")
        self._in(where, "branch", "feature")
        self._in(where, "branch", "ambig")

    @staticmethod
    def _git(*args):
        return subprocess.run(["git", *args], check=True, capture_output=True, text=True)

    @staticmethod
    def _in(where, *args):
        return subprocess.run(["git", "-C", str(where), *args], check=True,
                              capture_output=True, text=True)

    def run_cmd(self, cmd: str, cwd: Path | None = None):
        return run_hook(hooks.pretooluse, {
            "tool_input": {"command": cmd},
            "cwd": str(cwd if cwd is not None else self.root),
            "session_id": "s"})

    # --- git as the oracle ------------------------------------------------------------
    # Shared rather than owned by one class: the point of asking real git what a command
    # does is lost if only one test file's worth of spellings gets to ask.

    def _head(self, where: Path) -> str:
        r = subprocess.run(["git", "-C", str(where), "symbolic-ref", "-q", "--short", "HEAD"],
                           capture_output=True, text=True)
        if r.returncode == 0:
            return "branch:" + r.stdout.strip()
        return "detached:" + subprocess.run(
            ["git", "-C", str(where), "rev-parse", "HEAD"],
            capture_output=True, text=True).stdout.strip()

    def moves_head(self, cmd: str) -> bool:
        """Run *cmd* in a throwaway copy of this fixture and report whether HEAD moved.

        The copy lives OUTSIDE the plane root, which is the tmp directory itself: copying
        the root into a subdirectory of the root is a recursive copy that never ends. It hung
        the suite once, which is why this note is here.
        """
        if getattr(self, "_oracle_base", None) is None:
            self._oracle_base = Path(tempfile.mkdtemp(prefix="edm-oracle-"))
            self.addCleanup(shutil.rmtree, self._oracle_base, ignore_errors=True)
            self._oracle_n = 0
            # A copy of the live fixture, so any per-class setUp (aliases, extra branches)
            # is part of what git is asked about.
            tpl = self._oracle_base / "template"
            shutil.copytree(self.root, tpl)
            # `git init`'s sample hooks are most of the bytes in a fixture this small and
            # nothing here runs one. Dropping them is what keeps a generated corpus of a
            # couple of hundred spellings affordable — the copy below happens once per row.
            shutil.rmtree(tpl / ".git" / "hooks", ignore_errors=True)
            # Read once, not once per row: every scratch below is a fresh copy of this
            # template, so they all start from the same HEAD.
            self._oracle_before = self._head(tpl)
        self._oracle_n += 1
        scratch = self._oracle_base / f"run-{self._oracle_n}"
        shutil.copytree(self._oracle_base / "template", scratch)
        subprocess.run(shlex.split(cmd), cwd=scratch, capture_output=True, text=True,
                       timeout=60)
        return self._head(scratch) != self._oracle_before


class TestRestoringAFileIsNotABranchSwitch(CheckoutCase):
    """git: `git checkout README` → "Updated 1 path from the index", HEAD unchanged."""

    def test_the_reported_case(self):
        """The literal command from the issue: a tracked file in the plane root."""
        self.assertIsNone(_decision(self.run_cmd("git checkout README")))

    def test_the_modern_spelling_of_it_is_still_allowed(self):
        """`git restore README` was always allowed. Two spellings of one operation getting
        two different verdicts is what made this a false positive rather than a policy."""
        self.assertIsNone(_decision(self.run_cmd("git restore README")))

    def test_a_directory_pathspec_is_a_restore(self):
        """git: `git checkout notes` → "Updated 1 path from the index"."""
        self.assertIsNone(_decision(self.run_cmd("git checkout notes")))

    def test_a_dot_pathspec_is_a_restore(self):
        """git: `git checkout .` → "Updated 2 paths from the index". A `.` is not a branch
        by any reading, and the guard refused it."""
        self.assertIsNone(_decision(self.run_cmd("git checkout .")))

    def test_a_restore_from_another_tree_ish_is_allowed(self):
        """git: `git checkout feature README` → "Updated 1 path from <sha>", HEAD
        unchanged. Two operands: the first is the tree read from, the rest are paths."""
        self.assertIsNone(_decision(self.run_cmd("git checkout feature README")))

    def test_a_restore_flag_does_not_make_it_a_switch(self):
        """git: `git checkout --ours README` → "Updated 1 path from the index".

        `--ours` is on `_RESTORE_OPTS` and has to be: since #461's round two the carve-out
        opens only for a command whose options are ALL placed as restore-only, because an
        option decides what its operand means (`--orphan README` creates a branch called
        README). The list is the cost of that direction — a restore-only flag charter has
        not heard of is refused here until someone adds it, and the two spellings that need
        no flags at all stay allowed."""
        for cmd in ("git checkout --ours README", "git checkout -f README",
                    "git checkout -q README", "git checkout --conflict=merge README"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(_decision(self.run_cmd(cmd)))

    def test_it_still_works_through_a_dash_C(self):
        """The guard's reach is unchanged by this fix: a restore reached from a clone via
        `git -C <plane>` is a restore too."""
        clone = config.WORKSPACES_DIR / "ws" / "svc"
        clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(clone))
        self.assertIsNone(_decision(
            self.run_cmd(f"git -C {self.root} checkout README", cwd=clone)))


class TestARefMoveIsStillRefused(CheckoutCase):
    """The half that must not move, asserted with the REASON and not only the status —
    a denial that arrives from a different guard would otherwise read as this one working.
    """

    def _denied_as_the_branch_guard(self, cmd: str):
        r = self.run_cmd(cmd)
        self.assertEqual(_decision(r), "deny", cmd)
        self.assertIn("PLANE ROOT", _reason(r), cmd)
        return _reason(r)

    def test_a_branch_is_still_a_branch(self):
        """git: `git checkout feature` → "Switched to branch 'feature'"."""
        self._denied_as_the_branch_guard("git checkout feature")

    def test_switch_is_untouched(self):
        """`git switch` takes branches and nothing else — it exists because this overload
        is confusing — so the operand rule is asked of `checkout` only."""
        self._denied_as_the_branch_guard("git switch feature")

    def test_a_separator_does_not_give_switch_a_path_half_it_does_not_have(self):
        """Found by the generated corpus, not by anyone reading the code.

        `--` introduces PATHS on a `checkout`, and the rule reads "something after the
        separator, therefore a restore". `git switch` has no path half for a separator to
        introduce — that is most of why it exists — so on `switch` the same three characters
        are punctuation in front of a branch name. Measured against git 2.50:
        `git switch -- feature` and `git switch -q -- ambig` both answer "Switched to
        branch", and both were allowed in the plane root.
        """
        for cmd in ("git switch -- feature", "git switch -q -- ambig",
                    "git switch -- ambig", "git switch --detach -- main"):
            with self.subTest(cmd=cmd):
                self._denied_as_the_branch_guard(cmd)

    def test_but_the_remedy_survives_the_separator(self):
        """`git switch -- main` puts the root back on its default branch, separator and all
        — verified, "Switched to branch 'main'" — and the line above must not have turned
        every `switch --` into a refusal."""
        self._in(self.root, "checkout", "-q", "feature")
        self.assertIsNone(_decision(self.run_cmd("git switch -- main")))

    def test_a_commit_ish_that_is_not_a_branch_is_still_a_ref_move(self):
        """git: `git checkout HEAD~0` detaches. Resolving as a COMMIT is the test, not
        resolving as a branch — a tag or a raw sha moves HEAD just as far."""
        self._denied_as_the_branch_guard("git checkout HEAD~0")

    def test_the_previous_branch_shorthand_is_still_a_ref_move(self):
        """`-` is `@{-1}`, and it is what made the six-switch session in #157 cheap to
        repeat. It is never handed to git as an operand here, where it would read as an
        option."""
        self._denied_as_the_branch_guard("git checkout -")

    def test_creating_a_branch_is_still_refused(self):
        for flag in ("-b", "-B"):
            self._denied_as_the_branch_guard(f"git checkout {flag} chore/x")

    def test_creating_a_branch_named_after_a_tracked_file_is_still_refused(self):
        """A ref move is a ref move whatever the operand looks like, so the operand rule
        must not be reached at all — otherwise each of these resolves `README` as a path and
        walks straight past the guard.

        **`--orphan` is the row that was missing, and it was a live bypass**: measured
        against git 2.50, `git checkout --orphan README` answers "Switched to a new branch
        'README'" and HEAD moves off the plane root's branch. `origin/main` denied it and
        the first version of this fix allowed it — the branch was weaker than the code it
        was replacing, on the very command the fix is named after.
        """
        for flag in ("-b", "-B", "--orphan"):
            with self.subTest(flag=flag):
                self._denied_as_the_branch_guard(f"git checkout {flag} README")

    def test_an_unresolvable_operand_is_still_refused(self):
        """The fail-closed direction, and the one that matters for the next bypass: only a
        POSITIVE "git tracks this path and cannot read it as a commit" opens the gate. A
        name git has never heard of — a typo, or a branch that exists only on a remote,
        where git's DWIM creates it locally and switches (verified: "Switched to a new
        branch 'lonely'") — is not that."""
        self._denied_as_the_branch_guard("git checkout nosuch-branch")

    def test_a_shell_form_charter_cannot_resolve_is_still_refused(self):
        """`git checkout "$BR"` and `git checkout $(pick)` reach the guard as literal
        tokens. There is no spelling of a branch move that becomes an allow by being
        unreadable."""
        for cmd in ('git checkout "$BR"', "git checkout $(pick)"):
            self._denied_as_the_branch_guard(cmd)

    def test_a_command_substitution_does_not_become_a_multi_operand_restore(self):
        """**This is where the next bypass was.** charter's tokeniser flattens
        `git checkout $(echo feature)` into `['git','checkout','$','(','echo','feature',')']`
        — five operands. A rule that read "more than one operand" as "a restore" would have
        allowed it, and in a real shell it switches the plane root's branch. So the trailing
        operands are resolved against git rather than assumed, and `(` is not a tracked
        path."""
        self._denied_as_the_branch_guard("git checkout $(echo feature)")

    def test_a_redirection_glued_to_the_operands_does_not_excuse_a_switch(self):
        """Same class, ordinary spelling: whatever the tokeniser leaves alongside the
        branch name has to answer git's question too."""
        self._denied_as_the_branch_guard("git checkout feature 2>/dev/null")

    def test_two_operands_that_are_not_a_restore_are_still_refused(self):
        """git: `git checkout feature main` → "error: pathspec 'main' did not match any
        file(s) known to git". Refusing something git refuses costs nothing; allowing it
        because it had two operands would be the assumption above."""
        self._denied_as_the_branch_guard("git checkout feature main")

    def test_a_restore_of_more_paths_than_the_guard_will_resolve_is_refused(self):
        """The bounded-work end of the same rule. Past `_MAX_CHECKOUT_OPERANDS` the guard
        stops asking and keeps refusing — and the escape is the spelling that needs no
        questions at all, asserted here so the cap cannot become a dead end."""
        many = " ".join(["README"] * (hooks._MAX_CHECKOUT_OPERANDS + 2))
        self._denied_as_the_branch_guard(f"git checkout {many}")
        self.assertIsNone(_decision(self.run_cmd(f"git checkout -- {many}")))

    def test_a_git_that_cannot_be_asked_is_still_refused(self):
        """A guard that opened because it failed to ask is the fail-open shape #438 is
        about. `README` is the one operand in this fixture that the rule otherwise allows,
        so this can only pass by failing closed."""
        from charter import util

        with mock.patch("charter.doctor._git_in",
                        side_effect=util.ProcTimeout("git", 5)):
            r = self.run_cmd("git checkout README")
        self.assertEqual(_decision(r), "deny")
        self.assertIn("could not ask git", _reason(r))


class TestTheAmbiguousCaseStaysDenied(CheckoutCase):
    """`ambig` is both a tracked file and a branch.

    git: `git checkout ambig` → "Switched to branch 'ambig'". The ref wins, so refusing is
    correct here — and this is the case that stops the fix being over-applied into "any
    operand that names a file is safe".
    """

    def test_it_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git checkout ambig")), "deny")

    def test_the_message_says_ambiguous_rather_than_asserting_a_branch(self):
        """The half of #461 that is about the prose. A guard that is right to refuse and
        wrong about why still sends the operator to the wrong remedy."""
        reason = _reason(self.run_cmd("git checkout ambig"))
        self.assertIn("AMBIGUOUS", reason)
        self.assertIn("git restore ambig", reason)

    def test_the_unambiguous_spellings_of_the_same_restore_run(self):
        """And the message's advice has to be true: both forms it names must be allowed."""
        self.assertIsNone(_decision(self.run_cmd("git checkout -- ambig")))
        self.assertIsNone(_decision(self.run_cmd("git restore ambig")))


class TestDetachWasTheOtherHalfOfTheMisreading(CheckoutCase):
    """The same guard was too NARROW where it was too broad, and for the same reason —
    matching command shape. It required an operand, so the one HEAD move that needs no
    operand at all was invisible to it.

    git: `git checkout --detach` → "HEAD is now at <sha>", off `main`. Same for
    `git switch --detach`.
    """

    def test_checkout_detach_with_no_operand_is_refused(self):
        r = self.run_cmd("git checkout --detach")
        self.assertEqual(_decision(r), "deny")
        self.assertIn("detach HEAD", _reason(r))

    def test_the_short_form_is_the_same_detach_and_says_so(self):
        """git: `git checkout -d` → "HEAD is now at <sha>", and `git switch -d` likewise —
        both with no operand at all. The allowlist would refuse them either way, as an
        option it cannot place; the reason it is *named* as a detach is that the denial has
        to describe the command it is refusing."""
        for cmd in ("git checkout -d", "git switch -d"):
            with self.subTest(cmd=cmd):
                r = self.run_cmd(cmd)
                self.assertEqual(_decision(r), "deny")
                self.assertIn("detach HEAD", _reason(r))

    def test_switch_detach_with_no_operand_is_refused(self):
        self.assertEqual(_decision(self.run_cmd("git switch --detach")), "deny")

    def test_detach_at_a_named_ref_is_refused(self):
        self.assertEqual(_decision(self.run_cmd("git checkout --detach feature")), "deny")

    def test_detach_does_not_let_a_path_operand_excuse_it(self):
        """`--detach` is never a restore, so the operand rule must not be reached — a
        `--detach` with a tracked path is nonsense git rejects, and reading it as a restore
        would be a two-word bypass on the case above."""
        self.assertEqual(_decision(self.run_cmd("git checkout --detach README")), "deny")

    def test_the_default_branch_beside_a_detach_does_not_excuse_it_either(self):
        """Round three's hole, by name.

        The remedy carve-out — "returning the root to its default branch is always allowed"
        — gated on `not creating` and the operand's spelling, so `main` standing beside a
        `--detach` reached `continue` and skipped every line of the `--detach` handling
        above. Measured against git 2.50: each of these answers "HEAD is now at <sha>" and
        HEAD goes from `branch:main` to detached. The remedy is a command that leaves HEAD
        ATTACHED to the default branch, not one with the default branch's name in it.
        """
        for cmd in ("git checkout --detach main", "git switch -d main",
                    "git checkout -qd main", "git checkout -dq main",
                    "git checkout --detach main --", "git switch --detach -- main",
                    "git checkout --detach=main main"):
            with self.subTest(cmd=cmd):
                r = self.run_cmd(cmd)
                self.assertEqual(_decision(r), "deny", cmd)
                self.assertIn("PLANE ROOT", _reason(r), cmd)

    def test_a_detach_with_an_operand_is_described_as_a_detach(self):
        """Not "switch to 'main'". A refusal that is right and a sentence that is wrong about
        what the command does is the other half of #461 — and here the wrong sentence would
        name the exact command the denial's own last line promises is allowed."""
        r = self.run_cmd("git checkout --detach main")
        self.assertIn("detach HEAD at 'main'", _reason(r))
        self.assertNotIn("switch to", _reason(r))

    def test_the_denial_names_the_spelling_it_promises_rather_than_a_category(self):
        """The closing sentence used to read "Returning the root to its default branch is
        always allowed", printed underneath a refusal of `git checkout --detach main` —
        which is a command that returns the root to its default branch by name and is
        refused. It now names the spelling that is actually allowed, and the test asserts
        that spelling really is."""
        self.assertIn("`git checkout main`", _reason(self.run_cmd("git checkout --detach main")))
        self.assertIsNone(_decision(self.run_cmd("git checkout main")))


class TestTheRemedyAndTheRestOfTheGuardAreUnchanged(CheckoutCase):
    def test_returning_to_the_default_branch_is_still_allowed(self):
        """`doctor` prints exactly this. A guard that blocks the fix it recommends is a
        trap, and that carve-out predates this change."""
        self._in(self.root, "checkout", "-q", "feature")
        self.assertIsNone(_decision(self.run_cmd("git checkout main")))

    def test_a_bare_checkout_is_still_ignored(self):
        self.assertIsNone(_decision(self.run_cmd("git checkout")))

    def test_committing_in_the_root_is_still_allowed(self):
        self.assertIsNone(_decision(self.run_cmd("git commit -m x")))

    def test_a_commit_message_mentioning_checkout_is_still_not_a_switch(self):
        self.assertIsNone(_decision(
            self.run_cmd('git commit -m "docs: explain git checkout main"')))

    def test_switching_inside_a_workspace_clone_is_still_allowed(self):
        clone = config.WORKSPACES_DIR / "ws" / "svc"
        clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(clone))
        self.assertIsNone(_decision(self.run_cmd("git checkout -b feature/x", cwd=clone)))

    def test_no_control_plane_means_no_opinion(self):
        config.HAS_CONTROL_PLANE = False
        self.assertIsNone(_decision(self.run_cmd("git checkout feature")))


class TestGitItselfIsTheOracle(CheckoutCase):
    """The corpus test, with **real git as the oracle**: for every spelling below, run it in
    a throwaway copy of the plane root, look at HEAD before and after, and require that
    anything which MOVED HEAD is denied.

    This exists because round one's guard was strictly weaker than the code it replaced on
    one input (`git checkout --orphan README`), and no test in this file could have noticed:
    each row asserted a verdict somebody wrote down, so a spelling nobody thought of had no
    row. Here the expected verdict is not written down at all — git supplies it. A new
    spelling is one line, and it cannot be added with the wrong answer.

    One direction only, on purpose. "HEAD moved ⇒ denied" is the security property;
    "HEAD stayed ⇒ allowed" is not, and must not be asserted, because over-refusing in the
    plane root is a cost and not a hole (`git checkout --guess README` is refused here and
    `git restore README` does the job). The restores charter positively promises are
    asserted as allowed in `TestRestoringAFileIsNotABranchSwitch`, and repeated below
    against the oracle so the pair cannot drift.
    """

    #: Argv-able only: the shell forms (`$(…)`, `2>/dev/null`) belong to the tokeniser
    #: tests above, where the point is what charter sees rather than what git does.
    #:
    #: **Named rows, and the smaller half of the corpus.** Each one is a spelling that was
    #: once a live bypass or is a promise charter makes in prose, kept by name so a
    #: regression reads as itself in the failure output. The bulk of the coverage is
    #: `_generated_corpus()` below, because a hand-written list is only ever as long as the
    #: last audit — every bypass this guard has had was a spelling nobody typed here.
    NAMED = (
        # Ref moves, in every spelling of the value that git accepts.
        "git checkout feature", "git checkout ambig", "git checkout HEAD~0",
        "git checkout -b chore/x", "git checkout -b README", "git checkout -bREADME",
        "git checkout -B README", "git checkout -BREADME",
        "git checkout --orphan README", "git checkout --orphan=README",
        "git checkout --orphan README --", "git checkout -qbREADME",
        "git checkout --detach", "git checkout --detach feature", "git checkout -d feature",
        "git checkout -d", "git switch -d",
        "git checkout -f feature", "git checkout -q feature", "git checkout -fq feature",
        "git checkout --guess feature", "git checkout -m feature",
        "git switch feature", "git switch -c neu", "git switch -cneu",
        # Round five's bypass, and the reason it was one: `-C` is git's change-directory
        # global AND `switch`'s `--force-create`, and `_git_target` stripped the first
        # reading from anywhere in the argv, so the SEPARATED form retargeted the guard at a
        # directory called `neu` and the attached `-Cneu` was refused the whole time (#483).
        "git switch -C neu", "git switch -Cneu", "git switch -C feature",
        "git switch --orphan neu", "git switch --orphan=neu",
        "git switch --detach feature", "git switch -d feature",
        # Restores and no-ops: nothing here may move HEAD, and the guard's verdict on them
        # is deliberately not asserted from this list.
        "git checkout", "git checkout README", "git checkout .", "git checkout notes",
        "git checkout feature README", "git checkout -- README",
        "git checkout --ours README", "git checkout -f README", "git checkout -q README",
        "git checkout --conflict=merge README", "git restore README",
    )

    CORPUS = NAMED + _generated_corpus()

    #: What charter PROMISES is allowed. Every one is also run past git below, so the
    #: promise and the behaviour cannot drift apart silently.
    PROMISED_RESTORES = ("git checkout README", "git checkout .", "git checkout notes",
                         "git checkout feature README", "git checkout -- README",
                         "git restore README", "git checkout --ours README")

    def test_every_spelling_that_moves_head_is_denied(self):
        movers = []
        for cmd in self.CORPUS:
            with self.subTest(cmd=cmd):
                if not self.moves_head(cmd):
                    continue
                movers.append(cmd)
                r = self.run_cmd(cmd)
                self.assertEqual(_decision(r), "deny", f"git moved HEAD for: {cmd}")
                self.assertIn("PLANE ROOT", _reason(r), cmd)
        # A corpus in which git moved nothing would make the loop above vacuous — the
        # "test that cannot fail" shape. These are the counts and the names measured
        # against git 2.50, so a fixture that stops reproducing the conditions (no branch
        # to switch to, say) fails here rather than passing silently.
        self.assertGreaterEqual(len(movers), 90, len(movers))
        # Named witnesses, one per bypass this guard has actually had, so a fixture that
        # stops producing that shape fails loudly instead of quietly covering less. The
        # last two are round three's: a detach whose operand is the default branch walked
        # through the remedy carve-out, and `--` on a `switch` — which has no path half for
        # a separator to introduce — was read as "paths follow, so this is a restore".
        for must in ("git checkout --orphan README", "git checkout -bREADME",
                     "git switch -cneu", "git checkout -d feature",
                     "git checkout --detach main", "git switch -d main",
                     "git switch -- feature", "git switch -q -- ambig",
                     "git switch -C neu"):
            self.assertIn(must, movers)

    def test_no_promised_restore_moves_head_and_every_one_is_allowed(self):
        """The other half, and the reason the fix is not "deny everything": these are the
        sentences charter's denial message and news entry make true."""
        for cmd in self.PROMISED_RESTORES:
            with self.subTest(cmd=cmd):
                self.assertFalse(self.moves_head(cmd), cmd)
                self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)


class TestEveryRouteToThePlaneRootIsTheSameRoute(CheckoutCase):
    """A command that moves the plane root's HEAD is denied **however the root is reached**.

    The guard's subject is a repository, not a directory, and there is more than one way for
    one shell command to name it: the cwd, `git -C <path>`, a `cd` earlier in the same
    command, and every combination. The oracle above holds the command still and varies its
    spelling; this holds the command still and varies the ROUTE — and it is where the second
    of round three's holes lived.

    `_git_target` read `git -C <path>` as ``Path(args[i + 1])``, so a RELATIVE `-C` was
    resolved against the directory the hook process happened to be started in rather than
    against the shell's. Measured end to end against git 2.50 from a workspace clone:
    `git -C ../../.. checkout feature` answers *"Switched to branch 'feature'"* and the
    plane root's `symbolic-ref` follows it — while the guard was looking three levels above
    the hook's cwd, finding something that was not the plane root, and standing aside.
    `git -C . checkout feature` typed in the root itself, and `cd .. && git -C <root>
    checkout feature`, landed the same way.

    So the routes are an axis too, crossed with commands whose effect **git decides** — the
    same oracle as above, run on the bare command once, because a route cannot change what a
    command does to HEAD.

    **Round five added the routes that name a repository without naming a directory to stand
    in** (#477). `--git-dir`, `--work-tree` and their `GIT_DIR` / `GIT_WORK_TREE` env
    spellings were all already known to this guard — they sat in `_GIT_VALUE_OPTS`, where
    they were skipped as option VALUES so they could not be misread as a subcommand, and
    then never looked at again. Verified end to end against git 2.50 from a workspace clone:
    `git --git-dir <plane>/.git checkout feature` answers *"Switched to branch 'feature'"*
    and the plane root's `symbolic-ref` follows, and the guard printed nothing. That is the
    shape this class exists for — a route the guard has heard of and does not follow — and
    it survived three rounds of route work because the routes were a list rather than an
    axis crossed with the commands.
    """

    #: `(subcommand, rest)`, kept split so the alias route can put the subcommand inside
    #: `-c alias.…=` where no `checkout` token survives in the argv at all.
    COMMANDS = (
        ("checkout", "feature"), ("switch", "feature"), ("checkout", "-b chore/x"),
        ("checkout", "--detach main"), ("switch", "-d main"), ("checkout", "--orphan README"),
        ("switch", "-- feature"), ("checkout", "ambig"),
        # #483's command, carried through every route on purpose: the `-C` that means
        # `--force-create` here has to survive being crossed with the routes that spell
        # themselves `-C`, and `git -C <root> switch -C neu` is where the two readings meet
        # in one argv.
        ("switch", "-C neu"),
        # Not movers — carried through the same loop so the routes are exercised on the
        # allowed half too, and a route that started refusing every restore fails here.
        ("checkout", "README"), ("restore", "README"), ("checkout", "main"),
    )

    def setUp(self) -> None:
        super().setUp()
        self.clone = config.WORKSPACES_DIR / "ws" / "svc"
        self.clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(self.clone))
        # How a session in the clone spells the plane root without naming it absolutely.
        self.up = os.path.relpath(self.root, self.clone)
        self.assertTrue(self.up.startswith(".."), self.up)

    def routes(self, sub: str, rest: str):
        """``(label, cwd, command)`` for every way one shell command reaches the root."""
        tail = f"{sub} {rest}".strip()
        yield "cwd is the root", self.root, f"git {tail}"
        yield "-C, absolute", self.clone, f"git -C {self.root} {tail}"
        yield "-C, relative", self.clone, f"git -C {self.up} {tail}"
        yield "-C .", self.root, f"git -C . {tail}"
        yield "-C, relative, twice", self.clone, f"git -C {self.up} -C . {tail}"
        yield "cd into the root", self.root.parent, f"cd {self.root.name} && git {tail}"
        yield ("cd out, -C back in", self.root,
               f"cd .. && git -C {self.root.name} {tail}")
        yield ("an alias, from the clone", self.clone,
               f"git -C {self.up} -c alias.zz={sub} zz {rest}".strip())
        # #477: the two globals that name a repository WITHOUT naming a directory to stand
        # in. Both were already in `_GIT_VALUE_OPTS`, where they were skipped as option
        # values so they could not be misread as a subcommand — and then never looked at
        # again, so `git --git-dir <plane>/.git checkout feature` from a clone moved the
        # root's HEAD and the guard said nothing. Verified end to end against git 2.50.
        yield "--git-dir, separated", self.clone, f"git --git-dir {self.root}/.git {tail}"
        yield "--git-dir=, attached", self.clone, f"git --git-dir={self.root}/.git {tail}"
        yield ("--work-tree and --git-dir", self.clone,
               f"git --work-tree={self.root} --git-dir={self.root}/.git {tail}")
        yield ("--work-tree alone", self.clone,
               f"git --work-tree {self.root} --git-dir {self.root}/.git {tail}")
        # A relative `--git-dir` is interpreted against the directory the `-C`s ended at,
        # which is git's own documented rule for it — so the two compose.
        yield ("--git-dir relative to a -C", self.clone,
               f"git -C {self.up} --git-dir=.git {tail}")
        # The same two options spelled as environment. `_split_env` was already holding
        # them; nothing read them.
        yield ("GIT_DIR in the environment", self.clone,
               f"GIT_DIR={self.root}/.git git {tail}")
        yield ("GIT_WORK_TREE in the environment", self.clone,
               f"GIT_WORK_TREE={self.root} GIT_DIR={self.root}/.git git {tail}")

    def test_a_head_move_is_denied_by_every_route(self):
        movers = []
        for sub, rest in self.COMMANDS:
            bare = f"git {sub} {rest}".strip()
            if not self.moves_head(bare):
                continue
            movers.append(bare)
            for label, cwd, cmd in self.routes(sub, rest):
                with self.subTest(route=label, cmd=cmd):
                    r = self.run_cmd(cmd, cwd=cwd)
                    self.assertEqual(_decision(r), "deny", f"{label}: {cmd}")
                    self.assertIn("PLANE ROOT", _reason(r), cmd)
        # Non-vacuity: if git stopped moving HEAD for these — a fixture with no second
        # branch, say — the loop above would assert nothing and still pass. And the ROUTES
        # are counted too: a `routes()` that quietly stopped yielding the `--git-dir` half
        # would leave every remaining assertion green while covering less than it did.
        self.assertGreaterEqual(len(movers), 8, movers)
        labels = {label for label, _cwd, _cmd in self.routes("checkout", "feature")}
        self.assertGreaterEqual(len(labels), 15, sorted(labels))
        for must in ("--git-dir, separated", "--git-dir=, attached",
                     "--work-tree and --git-dir", "--git-dir relative to a -C",
                     "GIT_DIR in the environment", "GIT_WORK_TREE in the environment"):
            self.assertIn(must, labels)

    def test_a_restore_is_allowed_by_every_route(self):
        """The routes do not become a way to refuse what charter permits. Widening the
        guard's reach and narrowing what it refuses are the two halves of this change, and a
        route that only ever denied would look like the first while undoing the second."""
        for sub, rest in (("checkout", "README"), ("restore", "README")):
            bare = f"git {sub} {rest}"
            self.assertFalse(self.moves_head(bare), bare)
            for label, cwd, cmd in self.routes(sub, rest):
                with self.subTest(route=label, cmd=cmd):
                    self.assertIsNone(_decision(self.run_cmd(cmd, cwd=cwd)), f"{label}: {cmd}")

    def test_a_route_that_does_not_reach_the_root_is_still_not_the_root(self):
        """The reach is not "anything with a `-C` in it". A relative `-C` that lands in the
        clone is a clone command, and branch work in a clone is the thing charter tells
        people to do."""
        for cmd in ("git -C . checkout -b feature/x",
                    f"git -C {self.clone} checkout -b feature/x",
                    "cd .. && git -C svc checkout -b feature/x"):
            with self.subTest(cmd=cmd):
                self.assertIsNone(_decision(self.run_cmd(cmd, cwd=self.clone)), cmd)


#: ---------------------------------------------------------------------------------------
#: THE PINNED CORPUS — the baseline, vendored.
#:
#: `(verdict, where, command, note)`. The verdict is what the plane-root branch guard must
#: answer for that exact command typed in that exact directory; `where` is one of `root`
#: (the plane root), `clone` (a workspace clone outside it) or `above` (the root's parent);
#: `{root}`, `{clone}`, `{up}` and `{name}` are filled in per run because absolute paths
#: cannot be checked in.
#:
#: **This table is the baseline, and that is the whole point.** Its predecessor asked
#: `origin/main` what the guard used to answer, which worked until the branch carrying it
#: merged and `origin/main` became the code under test (#482). A reference that the subject
#: can grow into is not a reference. So the answers live here, in the repository, next to
#: the guard: nothing below resolves a ref, shells out for history, or reaches the network,
#: and the file is as true in a tarball and a depth-1 CI checkout as it is in a full clone.
#:
#: Two directions, both load-bearing:
#:
#: * A `DENY` row that starts passing means the guard STOPPED refusing a command that moves
#:   the plane root's HEAD — the regression this file exists to catch, named by spelling
#:   rather than reported as a count.
#: * An `ALLOW` row that starts failing means the guard started refusing something charter
#:   promises works (`git checkout <path>` is the whole of #461) — or that one of the
#:   KNOWN LIMITS below has been closed, which is good news that must be taken deliberately
#:   rather than absorbed silently.
#:
#: Every `DENY` row's effect on HEAD was measured against real git 2.50 rather than reasoned
#: about, and the argv-able ones are re-measured on every run by `TestGitItselfIsTheOracle`,
#: which asks git for the verdict this table writes down. The two are meant to overlap: the
#: oracle proves the answers here are git's, and this table keeps answering when the command
#: is a shell form git cannot be handed, or a route that only means something relative to a
#: plane root the oracle's scratch copy does not have.
PINNED: tuple[tuple[str, str, str, str], ...] = (

    # --- checkout, the ref-move half. git moves HEAD for every row here. -----------------
    ("DENY", "root", "git checkout feature", "git: Switched to branch 'feature'"),
    ("DENY", "root", "git checkout 'feature'", "quoted: one word once the tokeniser is done"),
    ("DENY", "root", 'git checkout "feature"', "the other quote, same word"),
    ("DENY", "root", r"git checkout fea\ture", "escaped: still the branch `feature`"),
    ("DENY", "root", "git checkout 'fea'ture", "quoting that closes mid-word"),
    ("DENY", "root", "git checkout -", "`-` is @{-1}, the previous branch: a ref, never a path"),
    ("DENY", "root", "git checkout HEAD~0", "a commit-ish that is not a branch detaches HEAD"),
    ("DENY", "root", "git checkout nosuchthing", "neither ref nor path: git DWIMs a remote branch"),
    ("DENY", "root", "git checkout feature --", "a TRAILING bare `--` still switches"),
    ("DENY", "root", "git checkout $BRANCH", "charter cannot resolve it, and unreadable is not an allow"),
    ("DENY", "root", 'git checkout "$(echo feature)"', "the same, through a substitution"),
    ("DENY", "root", "git checkout -fq feature", "restore-only options do not make a ref operand a path"),

    # --- checkout, creating a branch. `--orphan` was round one's bypass. -----------------
    ("DENY", "root", "git checkout -b chore/x", "git: Switched to a new branch 'chore/x'"),
    ("DENY", "root", "git checkout -B chore/x", "the force-create spelling"),
    ("DENY", "root", "git checkout -b 'neu'", "a quoted name"),
    ("DENY", "root", r"git checkout \-b neu", "an escaped dash is still `-b` after tokenising"),
    ("DENY", "root", "git checkout -bREADME", "ATTACHED value: the branch name is inside the option"),
    ("DENY", "root", "git checkout -BREADME", "the same, force-create"),
    ("DENY", "root", "git checkout -qbREADME", "a cluster: `-q -b README`, read left to right"),
    ("DENY", "root", "git checkout -b neu --", "git refuses this today; charter does not depend on that"),
    ("DENY", "root", "git checkout -B README --", "a create whose operand is a tracked file"),
    ("DENY", "root", "git checkout --orphan README", "round one's bypass: a create that reads as a restore"),
    ("DENY", "root", "git checkout --orphan=README", "the attached long form of it"),
    ("DENY", "root", "git checkout --orphan README --", "and with the separator as decoration"),
    ("DENY", "root", "git checkout --orphan notes", "a directory pathspec as the new branch's name"),

    # --- checkout, detaching. No operand needed, which is why it was invisible. ----------
    ("DENY", "root", "git checkout --detach", "git: HEAD is now at <sha> — with no operand at all"),
    ("DENY", "root", "git checkout --detach feature", ""),
    ("DENY", "root", "git checkout --detach main", "round three's bypass: the remedy's NAME beside a detach"),
    ("DENY", "root", "git checkout --detach main --", "and with a trailing separator"),
    ("DENY", "root", "git checkout -d", "git's own short form of --detach"),
    ("DENY", "root", "git checkout -d feature", ""),
    ("DENY", "root", "git checkout -qd main", "a cluster, restore letter first"),
    ("DENY", "root", "git checkout -dq main", "the same cluster the other way round"),
    ("DENY", "root", "git checkout --detach README",
     "git errors here (README is no commit); charter refuses AHEAD of git, deliberately"),

    # --- switch. It has no path half at all, so every row is a ref move. -----------------
    ("DENY", "root", "git switch feature", ""),
    ("DENY", "root", "git switch -", "the previous branch, again"),
    ("DENY", "root", "git switch -c neu", ""),
    ("DENY", "root", "git switch -cneu", "attached"),
    ("DENY", "root", "git switch -Cneu", "the ATTACHED spelling, refused all along"),
    # Was the LIMIT #483 row until this commit, and the flip is the point of pinning it:
    # `-C` is git's change-directory global AND `switch`'s `--force-create`, and
    # `_git_target` stripped the first reading from ANYWHERE in the argv — so this
    # retargeted the guard at a directory called `neu`, which is not the plane root, and
    # both plane-root guards stood aside without printing anything. git: "Switched to a new
    # branch 'neu'". `_git_globals` now splits the argv at the subcommand first, which is
    # the only position git itself reads a global `-C` in.
    ("DENY", "root", "git switch -C neu", "#483: the SEPARATED short form on `switch`"),
    ("DENY", "root", "git switch -C feature", "the same, force-creating over a branch that exists"),
    ("DENY", "root", "git switch -q -C neu", "and with a restore-shaped option beside it"),
    ("DENY", "root", "git -C . switch -C neu",
     "both readings of `-C` in one argv: the global retargets, the option creates"),
    ("DENY", "clone", "git -C {up} switch -C neu", "the same from a clone, relative"),
    ("DENY", "root", "git switch --create neu", ""),
    ("DENY", "root", "git switch --force-create neu", ""),
    ("DENY", "root", "git switch --orphan neu", ""),
    ("DENY", "root", "git switch -d", ""),
    ("DENY", "root", "git switch -d main", "a detach whose operand is the default branch"),
    ("DENY", "root", "git switch --detach feature", ""),
    ("DENY", "root", "git switch -- feature", "round three's other bypass: `switch` has no paths for a `--`"),
    ("DENY", "root", "git switch -q -- ambig", "the same, with an option and the ambiguous name"),
    ("DENY", "root", "git switch --detach -- main", "and with the remedy's name after the separator"),

    # --- the ambiguous case, which must STAY denied. -------------------------------------
    ("DENY", "root", "git checkout ambig",
     "both a tracked path and a ref; git breaks the tie for the REF and switches"),

    # --- an option charter cannot place keeps the guard shut. ----------------------------
    ("DENY", "root", "git checkout --guess README", "an allowlist, so an unplaced option refuses"),
    ("DENY", "root", "git checkout --track README", ""),
    ("DENY", "root", "git checkout -t README", ""),
    ("DENY", "root", "git checkout -l README", ""),
    ("DENY", "root", "git checkout --nonesuch-that-git-does-not-have README",
     "stands for whatever option git adds after this commit"),
    ("DENY", "root", "git checkout --nonesuch-that-git-does-not-have", "and with no operand to read"),

    # --- aliases: another spelling of the same command. ----------------------------------
    ("DENY", "root", "git co feature", "`co = checkout` in this fixture's config"),
    ("DENY", "root", "git -c alias.zz=checkout zz feature", "defined and used in one command line"),
    ("DENY", "root", "git -c alias.z='checkout feature' z", "#467's spelling: the operand is inside the alias"),
    ("DENY", "root", "git -c commit.gpgsign=false checkout feature",
     "a global `-c` VALUE is not a subcommand — this repo's own commit convention"),

    # --- routes. The subject is a repository, not a directory. ---------------------------
    ("DENY", "root", "git -C . checkout feature", "relative `-C`, standing in the root"),
    ("DENY", "root", "git -C {root} checkout feature", "absolute `-C`, standing in the root"),
    ("DENY", "root", "git -C {root} switch -d main", "and a detach by that route"),
    ("DENY", "clone", "git -C {root} checkout feature", "absolute `-C`, from a workspace clone"),
    ("DENY", "clone", "git -C {up} checkout feature",
     "RELATIVE `-C` from outside: resolved against the shell, which is round three's fix"),
    ("DENY", "clone", "git -C {up} -C . checkout feature", "two `-C`, each relative to the one before"),
    ("DENY", "clone", "git -C {up} switch -c neu", "a create by that route"),
    ("DENY", "clone", "git -C {up} -c alias.zz=checkout zz feature", "an alias by that route"),
    # Was the three LIMIT #477 rows until this commit. `--git-dir` and `--work-tree` name a
    # repository without naming a directory to stand in, and both were in `_GIT_VALUE_OPTS`
    # — skipped as option VALUES so they could not be misread as a subcommand, and then
    # never looked at again. Verified against git 2.50: from a clone, each of these answers
    # "Switched to branch 'feature'" and the plane root's `symbolic-ref` follows.
    ("DENY", "clone", "git --git-dir={root}/.git checkout feature", "#477: --git-dir, attached"),
    ("DENY", "clone", "git --git-dir {root}/.git checkout feature", "#477: the separated form"),
    ("DENY", "clone", "git --work-tree={root} --git-dir={root}/.git checkout feature",
     "#477: --work-tree names the tree directly"),
    ("DENY", "clone", "git --git-dir={root}/.git switch -c neu", "#477, creating a branch"),
    ("DENY", "clone", "git -C {up} --git-dir=.git checkout feature",
     "#477: a relative --git-dir is read against the directory the -C landed in"),
    ("DENY", "clone", "GIT_DIR={root}/.git git checkout feature", "#477: the env spelling"),
    ("DENY", "clone", "GIT_WORK_TREE={root} GIT_DIR={root}/.git git switch feature",
     "#477: both env spellings together"),
    ("DENY", "clone", "git --git-dir={root}/.git switch -C neu",
     "#477 and #483 in one argv: the route the guard could not follow, carrying the option "
     "it could not read"),
    ("DENY", "clone", "git --git-dir={root}/.git checkout --orphan README",
     "#477 carrying round one's bypass"),
    ("DENY", "clone", "git --git-dir={root}/.git --work-tree={root} checkout --detach main",
     "#477 with the options the other way round, carrying round three's"),
    ("DENY", "above", "git --git-dir={name}/.git checkout feature",
     "#477 with a RELATIVE git dir, resolved against the shell like a `-C`"),
    ("DENY", "clone", "GIT_DIR={root}/.git git -c alias.zz=switch zz -d main",
     "#477's route carrying #467's alias"),
    ("DENY", "clone", "git --work-tree {root} checkout feature",
     "git itself refuses --work-tree without a git dir; charter refuses AHEAD of git, "
     "deliberately, rather than depending on git continuing to"),
    ("DENY", "above", "cd {name} && git checkout feature", "a `cd` earlier in the SAME command"),
    ("DENY", "above", "cd {name} && git switch feature", ""),
    ("DENY", "root", "cd .. && git -C {name} checkout feature", "out and back in again"),

    # --- ALLOW: the file-restore carve-out, which is the whole of #461. ------------------
    ("ALLOW", "root", "git checkout README", "the reported case: git updates 1 path, HEAD unchanged"),
    ("ALLOW", "root", "git checkout -- README", "the explicit spelling"),
    ("ALLOW", "root", "git checkout -- README ambig", "after a `--` everything is a path"),
    ("ALLOW", "root", "git checkout -- ambig", "the unambiguous spelling of the ambiguous name"),
    ("ALLOW", "root", "git checkout -- .", ""),
    ("ALLOW", "root", "git checkout .", "a pathspec that is no single file"),
    ("ALLOW", "root", "git checkout notes", "a directory pathspec"),
    ("ALLOW", "root", "git checkout notes/a.md", ""),
    ("ALLOW", "root", "git checkout -- 'notes/a.md'", "quoted"),
    ("ALLOW", "root", "git checkout feature README", "`<tree-ish> <paths…>`: HEAD stays put"),
    ("ALLOW", "root", "git checkout HEAD -- README", ""),
    ("ALLOW", "root", "git checkout main -- README", ""),
    ("ALLOW", "root", "git checkout --ours README", "a restore-only option"),
    ("ALLOW", "root", "git checkout -f README", ""),
    ("ALLOW", "root", "git checkout -q README", ""),
    ("ALLOW", "root", "git checkout -fq README", "a cluster of restore letters"),
    ("ALLOW", "root", "git checkout -qf README", "and the other order"),
    ("ALLOW", "root", "git checkout --conflict=merge README", "an attached long value"),
    ("ALLOW", "root", "git checkout -p", "an interactive restore has no operand at all"),
    ("ALLOW", "root", "git checkout", "bare: moves nothing"),
    ("ALLOW", "root", "git restore README", "the modern spelling, always allowed"),
    ("ALLOW", "root", "git restore ambig", "`restore` has no ref half, so there is no ambiguity to break"),
    ("ALLOW", "root", "git restore --staged README", ""),
    ("ALLOW", "root", "git restore --source=feature README", ""),
    ("ALLOW", "clone", "git -C {up} checkout README", "the carve-out survives the route"),
    ("ALLOW", "clone", "git -C {up} checkout -- README", ""),
    ("ALLOW", "clone", "git -C {root} restore README", ""),

    # --- ALLOW: the documented remedy stays runnable. ------------------------------------
    ("ALLOW", "root", "git checkout main", "`doctor` prints this; a guard that blocks its own fix is bypassed"),
    ("ALLOW", "root", "git checkout -f main", "options and all"),
    ("ALLOW", "root", "git switch main", ""),

    # --- ALLOW: a clone is not the root, and branch work belongs there. ------------------
    ("ALLOW", "clone", "git -C . checkout -b feature/x", "the reach is not 'anything with a -C in it'"),
    ("ALLOW", "clone", "git -C {clone} checkout -b feature/x", ""),
    ("ALLOW", "clone", "cd .. && git -C svc checkout -b feature/x", ""),
    ("ALLOW", "clone", "git switch -C neu", "#483's option, in the repo where branch work belongs"),
    ("ALLOW", "clone", "git --git-dir=.git switch -c feature/x",
     "#477's option pointed at the clone's OWN git dir: the reach is the repository it names"),
    ("ALLOW", "clone", "git --work-tree={clone} --git-dir={clone}/.git checkout -b feature/x", ""),
    ("ALLOW", "clone", "GIT_DIR={clone}/.git git switch -c feature/x", "the env spelling of the same"),
    ("ALLOW", "clone", "git --git-dir={root}/.git checkout README",
     "#477 widened the REACH, not what is refused: a restore is still a restore there"),

    # --- ALLOW, and every row a KNOWN LIMIT: a command that DOES move the plane root's
    #     HEAD and is not refused. Pinned as facts rather than left as silent gaps, each
    #     naming the issue that tracks it. Closing one turns its row RED, which is the
    #     point: a limit should not disappear without somebody noticing and saying so.
    ("ALLOW", "clone", "export GIT_DIR={root}/.git && git checkout feature",
     "LIMIT #496: the shared walk carries a `cd` across segments and not an env assignment, "
     "so the ATTACHED spelling `GIT_DIR=… git …` is refused (#477) and this one is not. "
     "git: Switched to branch 'feature', and the root's symbolic-ref follows"),
    # #430 CLOSED. These six rows were recorded as limits — `prog` came from token 0, so a
    # wrapper word or a shell keyword standing in front of `git` hid it from every guard in
    # the module. `_split_env_chdir` strips the wrapper run before naming the program and
    # `_segment_tokens` opens a segment at a grouping token in command position, so all six
    # now reach the same `git checkout` the bare spelling does. Flipped here rather than
    # deleted: a row that changes verdict is the record of what changed, and one that
    # disappears is a row nobody can see used to say something else.
    ("DENY", "root", "env git checkout feature",
     "#430: a wrapper prefix no longer hides the program"),
    ("DENY", "root", "/usr/bin/env git checkout feature", "#430: the same by absolute path"),
    ("DENY", "root", "command git checkout feature", "#430"),
    ("DENY", "root", "nohup git checkout feature", "#430"),
    ("DENY", "root", "if true; then git checkout feature; fi", "#430: a shell keyword in token 0"),
    ("DENY", "root", "( git checkout feature )", "#430: a grouping token opens a segment"),
    ("ALLOW", "root", "sh -c 'git checkout feature'",
     "LIMIT #430 (named there as out of scope): an interpreter's argument is the command as "
     "TEXT, and charter does not re-parse it — the same deliberate limit pinned in "
     "tests/test_leak_guard_readers_that_write.py"),
    ("ALLOW", "root", 'bash -c "git checkout feature"', "LIMIT #430: the same, another shell"),
)

#: Rows whose note names a tracking issue. Split out so the two halves can be counted
#: separately: a corpus that quietly turned into limits would still pass a bare row count.
_LIMIT = "LIMIT #"


class TestThePinnedCorpusIsTheBaseline(CheckoutCase):
    """**Every row of `PINNED` gets the verdict written beside it**, and the file is the
    baseline.

    This replaces a differential test that read the shipped guard out of `origin/main` at
    runtime. It was right about the property and wrong about where to keep the reference:
    the branch carrying it merged, `origin/main` became the subject, and the comparison
    turned into `x == x` — an assertion that could not be satisfied again on `main` or on
    anything cut from it (#482). Fixing the ref it read would only move the expiry date; a
    baseline that lives anywhere the subject can reach expires by construction.

    So the baseline is checked in, the way `test_vault_path_re_only_widens` keeps its own.
    That trades one thing for another and the trade is worth naming: reading `origin/main`
    covered whatever spellings the generated corpus produced, automatically, while this
    covers the spellings written down here. What it buys is that it keeps answering — in a
    tarball, in a shallow clone, on a detached CI tree, and next year — and that a
    regression arrives as a NAMED row rather than as `0 not greater than or equal to 4`.

    The unbounded half of the question is not this test's job and never was:
    `TestGitItselfIsTheOracle` crosses the axes and lets real git supply the verdict, and
    `TestEveryRouteToThePlaneRootIsTheSameRoute` crosses commands with routes. Those two
    carry the property. This one pins the answers so they cannot quietly change.
    """

    def setUp(self) -> None:
        super().setUp()
        # A clone outside the root, and the relative path back to the root from it: how a
        # session standing in a workspace spells the plane root without naming it absolutely.
        self.clone = config.WORKSPACES_DIR / "ws" / "svc"
        self.clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(self.clone))
        self.up = os.path.relpath(self.root, self.clone)
        self.assertTrue(self.up.startswith(".."), self.up)
        # `co = checkout` is on a large share of developer machines, and the corpus has a
        # row for it. Configured here rather than assumed, so the row tests the guard's
        # alias resolution rather than the machine the suite happens to run on.
        self._in(self.root, "config", "alias.co", "checkout")

    def render(self, where: str, cmd: str) -> tuple[Path, str]:
        """`(cwd, command)` for one row: the absolute paths a checked-in table cannot hold."""
        cwd = {"root": self.root, "clone": self.clone, "above": self.root.parent}[where]
        return cwd, cmd.format(root=self.root, clone=self.clone, up=self.up,
                               name=self.root.name)

    # --- the corpus ---------------------------------------------------------------------

    def test_every_pinned_row_gets_its_pinned_verdict(self):
        """The whole table, one subTest per row, so a failure names the spelling."""
        wrong = []
        for verdict, where, cmd, note in PINNED:
            cwd, rendered = self.render(where, cmd)
            with self.subTest(verdict=verdict, where=where, cmd=rendered):
                got = _decision(self.run_cmd(rendered, cwd=cwd))
                got = "DENY" if got == "deny" else "ALLOW"
                if got != verdict:
                    wrong.append(f"{cmd!r} in {where}: pinned {verdict}, got {got}"
                                 + (f" — {note}" if note else ""))
                self.assertEqual(got, verdict, f"{rendered}\n{note}")
        self.assertEqual(wrong, [], wrong)

    def test_a_denial_says_which_guard_refused(self):
        """A `DENY` row that is refused by some OTHER guard would satisfy the row above
        while the plane-root guard slept through it. Every denial here has to be this one,
        and to carry the sentence an operator acts on."""
        for verdict, where, cmd, _note in PINNED:
            if verdict != "DENY":
                continue
            cwd, rendered = self.render(where, cmd)
            with self.subTest(cmd=rendered):
                reason = _reason(self.run_cmd(rendered, cwd=cwd))
                self.assertIn("PLANE ROOT", reason, rendered)
                self.assertIn("charter workspace create", reason, rendered)

    # --- the corpus's own shape ---------------------------------------------------------

    def test_the_corpus_is_not_vacuous(self):
        """Counts, so a table gutted down to the rows that happen to pass fails here rather
        than shrinking quietly. The numbers are floors measured at the time of writing;
        adding rows never breaks them."""
        denies = [r for r in PINNED if r[0] == "DENY"]
        allows = [r for r in PINNED if r[0] == "ALLOW"]
        limits = [r for r in PINNED if _LIMIT in r[3]]
        self.assertGreaterEqual(len(denies), 85, len(denies))
        self.assertGreaterEqual(len(allows) - len(limits), 35, len(allows) - len(limits))
        # There is deliberately NO floor on `len(limits)`. A count cannot tell "closed" from
        # "deleted" in either direction: #430's six rows are still here, flipped to DENY and
        # annotated, and #483/#477's four are gone because they were closed. What holds the
        # limits honest is `LIMIT_ISSUES` below — the exact SET of issues cited, which fails
        # when one is closed without saying so AND when one is added without saying so.
        self.assertEqual(sorted({r[0] for r in PINNED}), ["ALLOW", "DENY"])

    def test_a_closed_limit_stays_in_the_table_as_a_denial(self):
        """The other half of the count above. Lowering a floor because rows left the table
        and lowering it because rows changed verdict look identical from the number, so the
        rows that changed are asserted by name: still present, still `root`, now DENY."""
        rows = {(w, c): v for v, w, c, _n in PINNED}
        for cmd in ("env git checkout feature", "/usr/bin/env git checkout feature",
                    "command git checkout feature", "nohup git checkout feature",
                    "if true; then git checkout feature; fi", "( git checkout feature )"):
            with self.subTest(cmd=cmd):
                self.assertEqual(rows.get(("root", cmd)), "DENY",
                                 "a limit closed by #430 left the table instead of "
                                 "changing verdict in it")

    def test_no_row_is_written_twice(self):
        """Two rows for one `(where, command)` are either a duplicate or a contradiction,
        and a contradiction would make one of them unfalsifiable."""
        seen: dict[tuple[str, str], str] = {}
        for verdict, where, cmd, _note in PINNED:
            key = (where, cmd)
            self.assertNotIn(key, seen, f"{cmd!r} in {where} appears twice")
            seen[key] = verdict

    #: The issues the remaining `LIMIT` rows track — the WHOLE set, not a floor.
    #:
    #: A count was the wrong shape and this commit is why. Round five closed #483 and #477,
    #: which took the corpus from twelve limits to eight; a `>= 12` assertion turns closing
    #: a hole into a red test with no name on it, and the only way to read that failure is
    #: to lower the number, which is indistinguishable from a limit quietly reappearing. An
    #: exact set fails in BOTH directions and says which issue moved: closing one leaves an
    #: issue listed here with no row, and adding one leaves a row citing an issue not here.
    LIMIT_ISSUES = {"#430", "#496"}

    def test_every_known_limit_names_the_issue_that_tracks_it(self):
        """A limit with no issue behind it is a gap somebody decided to live with and then
        forgot. The `#nnn` is what makes the row a decision rather than an omission — and
        what a reader follows when the row goes red because the limit was closed."""
        limits = [r for r in PINNED if _LIMIT in r[3]]
        self.assertTrue(limits)
        for verdict, _where, cmd, note in limits:
            with self.subTest(cmd=cmd):
                self.assertEqual(verdict, "ALLOW", "a LIMIT row records what is NOT refused")
                self.assertRegex(note, r"LIMIT #\d+", cmd)
        cited = {m for _v, _w, _c, note in limits
                 for m in re.findall(r"LIMIT (#\d+)", note)}
        self.assertEqual(cited, self.LIMIT_ISSUES,
                         "a limit was closed or added without saying so here")

    def test_the_axes_the_recent_work_established_all_have_rows(self):
        """The spellings this guard was taught over four rounds, asserted to be PRESENT.

        A regression can be introduced by deleting a row as easily as by relaxing the code,
        and the row above only checks the rows that are there. So the axes are named: drop
        every `--orphan` row, or every route row, and this fails with the axis's name.
        """
        commands = " || ".join(f"{w} {c}" for _v, w, c, _n in PINNED)
        for axis in ("checkout -b", "checkout -B", "--detach", "--orphan", "git switch ",
                     "switch -c", "switch -C neu", "switch -d", "checkout -d",
                     "-qbREADME", "-qd", "-dq", "checkout -- ", "git restore ",
                     "git -C .", "git -C {root}", "git -C {up}", "cd {name} &&",
                     "alias", "ambig", "notes/a.md", "$(echo",
                     # Round five's axes: #483's separated short `-C`, and #477's three
                     # spellings of "the repository is over there".
                     "--git-dir=", "--git-dir ", "--work-tree", "GIT_DIR=", "GIT_WORK_TREE="):
            with self.subTest(axis=axis):
                self.assertIn(axis, commands, f"no row covers {axis!r} any more")


class TestAnOptionCharterCannotPlaceKeepsTheGuardShut(CheckoutCase):
    """The property behind the `--orphan` bypass, tested as the property.

    `--orphan` got through because the guard held a list of the four flags it knew moved
    HEAD and treated everything else as harmless — so every option git has that was not on
    that list, and every option git adds next, was a candidate bypass. The rule is now the
    other way round: the restore carve-out opens only when every option present is one
    charter can place as restore-only.

    `--nonesuch-that-git-does-not-have` is in here as the *future* spelling: it stands for
    whatever git adds after this commit, and it is refused today.
    """

    def _denied(self, cmd: str) -> str:
        r = self.run_cmd(cmd)
        self.assertEqual(_decision(r), "deny", cmd)
        self.assertIn("PLANE ROOT", _reason(r), cmd)
        return _reason(r)

    def test_an_unrecognised_option_does_not_open_the_restore_carve_out(self):
        for cmd in ("git checkout --guess README", "git checkout --track README",
                    "git checkout --nonesuch-that-git-does-not-have README",
                    "git checkout -t README", "git checkout -l README"):
            with self.subTest(cmd=cmd):
                self._denied(cmd)

    def test_the_denial_names_the_option_rather_than_asserting_a_switch(self):
        """#461's other half was a denial that was right to refuse and wrong about why.
        charter does not know that `--guess README` switches — it knows it cannot read the
        command as a restore, and that is what it says, with the two spellings that work."""
        reason = self._denied("git checkout --guess README")
        self.assertIn("--guess", reason)
        self.assertIn("git restore README", reason)
        self.assertNotIn("would switch to", reason)

    def test_an_unrecognised_option_with_no_operand_is_not_a_bare_checkout(self):
        """`git checkout -bREADME` has no operand at all — the whole command is one token
        after `checkout` — and "no operand means nothing moves" allowed it. git: "Switched
        to a new branch 'README'"."""
        for cmd in ("git checkout -bREADME", "git checkout -BREADME",
                    "git switch -cneu", "git checkout -qbREADME"):
            with self.subTest(cmd=cmd):
                self._denied(cmd)

    def test_an_unplaced_option_alone_does_not_get_the_bare_checkout_pass(self):
        """`git checkout` with nothing after it moves nothing, and that pass used to be
        granted on "no operand" alone — which is how `-bREADME` got it, the branch name
        being inside the option. It now also requires every option to be placed, so the next
        option git adds that needs no operand does not inherit the pass either; `--detach`
        was one such option, and it walked past this guard for a year.

        The contrast matters as much as the denial: `-p` is a restore option, so an
        interactive restore in the plane root is still allowed."""
        self._denied("git checkout --nonesuch-that-git-does-not-have")
        self.assertIsNone(_decision(self.run_cmd("git checkout -p")))

    def test_the_denial_names_the_branch_hidden_inside_the_option(self):
        """The name is in the option token, so a message built from the operands would name
        nothing. A guard that cannot say what it is refusing is teaching people to ignore
        it."""
        self.assertIn("create 'README'", self._denied("git checkout -bREADME"))
        self.assertIn("create 'README'", self._denied("git checkout --orphan=README"))

    def test_the_value_forms_of_a_creator_are_the_same_creator(self):
        for cmd in ("git checkout --orphan=README", "git switch --orphan=neu",
                    "git switch --create=neu", "git checkout --no-orphan README"):
            with self.subTest(cmd=cmd):
                self._denied(cmd)

    def test_a_separator_does_not_launder_a_branch_creation(self):
        """`git checkout -b neu -- README` is refused by git today, and the carve-out used
        to allow it on the strength of "something follows `--`". A guard that is safe only
        while git keeps rejecting something is a bypass waiting for a release note."""
        for cmd in ("git checkout -b neu -- README", "git checkout --orphan neu -- README",
                    "git checkout --orphan README --"):
            with self.subTest(cmd=cmd):
                self._denied(cmd)

    def test_the_option_classifier_places_git_2_50s_own_option_list(self):
        """The helper against git's own `-h` output, one layer down from the guard, so the
        allowlist's contents are asserted somewhere other than by their effect."""
        for tok, expected in (("-b", "create"), ("-B", "create"), ("-c", "create"),
                              ("-C", "create"), ("--orphan", "create"),
                              ("--orphan=x", "create"), ("-bx", "create"),
                              ("--create", "create"), ("--force-create", "create"),
                              ("--detach", "detach"), ("-d", "detach"),
                              ("--no-detach", "detach"),
                              ("--ours", "restore"), ("--theirs", "restore"),
                              ("-2", "restore"), ("-3", "restore"), ("-f", "restore"),
                              ("-fq", "restore"), ("--conflict=merge", "restore"),
                              ("--no-quiet", "restore"), ("--pathspec-from-file=x",
                                                          "restore"),
                              ("--track", "unknown"), ("-t", "unknown"),
                              ("--guess", "unknown"), ("--no-guess", "unknown"),
                              ("-l", "unknown"), ("--brand-new-git-flag", "unknown")):
            with self.subTest(tok=tok):
                self.assertEqual(hooks._checkout_opt_kind(tok), expected)


class TestAnAliasIsAnotherSpellingOfTheSameCommand(CheckoutCase):
    """`git checkout` is not the only way to spell `git checkout`.

    This is the answer to "what is the next spelling of this that still gets through?" for
    the round-two fix, and it was not hypothetical: with `co = checkout` — an alias on a
    large share of developer machines — `git co feature` in the plane root answered
    "Switched to branch 'feature'" and the guard never saw a `checkout`. `origin/main` was
    equally blind, so this is not a regression; it is the same defect one layer out, the
    guard matching a SPELLING rather than asking what the command does.

    Every alias below was run against real git 2.50 with HEAD recorded before and after.
    """

    def setUp(self) -> None:
        super().setUp()
        self._in(self.root, "config", "alias.co", "checkout")
        self._in(self.root, "config", "alias.ck", "co")           # an alias to an alias
        self._in(self.root, "config", "alias.sw", "switch -c")    # carrying its own option
        self._in(self.root, "config", "alias.bang", "!git checkout")
        self._in(self.root, "config", "alias.shell", "!echo checkout")

    def _denied(self, cmd: str) -> str:
        """Denied — and, for the spellings git can be asked about here, only counted as a
        pass once real git has been shown to move HEAD for it."""
        r = self.run_cmd(cmd)
        self.assertEqual(_decision(r), "deny", cmd)
        self.assertIn("PLANE ROOT", _reason(r), cmd)
        return _reason(r)

    def _really_moves_and_is_denied(self, cmd: str) -> None:
        self.assertTrue(self.moves_head(cmd), f"git did not move HEAD for: {cmd}")
        self._denied(cmd)

    def test_a_plain_alias_is_followed(self):
        """git: `git co feature` → "Switched to branch 'feature'"."""
        self._really_moves_and_is_denied("git co feature")

    def test_an_alias_chain_is_followed(self):
        """git follows `ck` → `co` → `checkout` and switches. One hop would not have."""
        self._really_moves_and_is_denied("git ck feature")

    def test_an_alias_that_carries_its_own_options_is_judged_with_them(self):
        """git: `git sw neu` → "Switched to a new branch 'neu'". The alias supplies `-c` and
        the caller supplies the name, so neither half read alone is a branch creation."""
        self._really_moves_and_is_denied("git sw neu")

    def test_an_alias_defined_on_the_command_line_is_followed(self):
        """git: `git -c alias.zz=checkout zz feature` → "Switched to branch 'feature'". No
        config on disk has ever heard of `zz`, which is what makes this the cheapest
        spelling of all — it needs no setup."""
        self._really_moves_and_is_denied("git -c alias.zz=checkout zz feature")

    def test_a_bang_alias_that_is_a_git_command_is_followed(self):
        """git: `git bang feature` → "Switched to branch 'feature'". `!git checkout` is a
        git command wearing a shell alias's clothes."""
        self._really_moves_and_is_denied("git bang feature")

    def test_an_alias_does_not_turn_a_restore_into_a_switch(self):
        """The direction that keeps this from being a cage: git: `git co README` → "Updated
        1 path from the index", HEAD unchanged. Following the alias means the operand rule
        runs, not that the command is refused."""
        self.assertFalse(self.moves_head("git co README"))
        self.assertIsNone(_decision(self.run_cmd("git co README")))

    def test_a_shell_alias_charter_cannot_read_is_out_of_scope_not_refused(self):
        """The limit, asserted so it is a decision rather than an accident: `!echo checkout`
        is a shell command charter does not run and cannot read. Refusing every `!` alias in
        the plane root would refuse `s = !git status` and its cousins, which is a cage; the
        guard stands aside, and `docs/hooks.md` says so rather than claiming otherwise."""
        self.assertIsNone(_decision(self.run_cmd("git shell feature")))

    def test_an_ordinary_command_costs_no_alias_lookup(self):
        """The cost half. `git status` in the plane root must not spend a subprocess asking
        whether `status` is an alias — this guard runs on every Bash call."""
        with mock.patch("charter.doctor._git_in") as asked:
            self.assertIsNone(_decision(self.run_cmd("git status")))
        self.assertEqual(asked.call_args_list, [])

    def test_a_git_that_will_not_answer_does_not_break_the_turn(self):
        """`_plane_default_branch` raising out of a `PreToolUse` handler was a broken turn
        rather than a verdict, and the alias lookup is one more place that could do it."""
        from charter import util

        with mock.patch("charter.doctor._git_in", side_effect=util.ProcTimeout("git", 5)):
            self.assertIsNone(_decision(self.run_cmd("git co feature")))

    def test_the_resolver_reports_what_git_would_run(self):
        """One layer down, so the table is asserted somewhere other than by its effect."""
        cases = (
            ("co", ["feature"], [], ("checkout", ["feature"])),
            ("ck", ["feature"], [], ("checkout", ["feature"])),
            ("sw", ["neu"], [], ("switch", ["-c", "neu"])),
            ("bang", ["feature"], [], ("checkout", ["feature"])),
            ("shell", ["feature"], [], ("shell", ["feature"])),
            ("status", [], [], ("status", [])),
            ("zz", ["feature"], ["-c", "alias.zz=checkout"], ("checkout", ["feature"])),
        )
        for sub, post, pre, expected in cases:
            with self.subTest(sub=sub):
                self.assertEqual(hooks._resolve_git_alias(self.root, sub, post, pre),
                                 expected)

    def test_an_alias_loop_terminates(self):
        """`a = b`, `b = a` is a config a person can write, and a guard that follows it for
        ever is a hung `PreToolUse` hook — every tool call in the session."""
        self._in(self.root, "config", "alias.aa", "bb")
        self._in(self.root, "config", "alias.bb", "aa")
        self.assertIsNone(_decision(self.run_cmd("git aa feature")))


class TestTheOperandRuleAsksGit(CheckoutCase):
    """The helper on its own, against the four fixture names. Kept because the guard above
    can only show three of the five answers, and `unknown` has to be reachable by something
    other than the guard's own message."""

    def test_each_fixture_name_resolves_the_way_git_resolves_it(self):
        for op, expected in (("README", "path"), ("notes", "path"), (".", "path"),
                             ("feature", "rev"), ("HEAD", "rev"), ("-", "rev"),
                             ("ambig", "both"), ("nosuch-branch", "neither")):
            with self.subTest(op=op):
                self.assertEqual(hooks._checkout_operand_kind(self.root, op), expected)

    def test_a_git_that_will_not_answer_is_unknown_not_a_path(self):
        from charter import util

        with mock.patch("charter.doctor._git_in",
                        side_effect=util.ProcTimeout("git", 5)):
            self.assertEqual(hooks._checkout_operand_kind(self.root, "README"), "unknown")


if __name__ == "__main__":
    unittest.main()
