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
"""

from __future__ import annotations

import subprocess
import unittest
from pathlib import Path
from unittest import mock

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook


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
        self._git("init", "-q", "-b", "main", str(self.root))
        (self.root / "README").write_text("plane\n")
        (self.root / "ambig").write_text("both\n")
        (self.root / "notes").mkdir()
        (self.root / "notes" / "a.md").write_text("a\n")
        self._in(self.root, "add", "-A")
        self._in(self.root, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
                 "-c", "user.name=t", "commit", "-qm", "init")
        self._in(self.root, "branch", "feature")
        self._in(self.root, "branch", "ambig")

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
        """git: `git checkout --ours README` → "Updated 1 path from the index". Allowed by
        the rule rather than by a list of restore-only flags — nothing here knows `--ours`
        exists, and it does not have to."""
        self.assertIsNone(_decision(self.run_cmd("git checkout --ours README")))

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
        """`-b` is a ref move whatever the operand looks like, so the operand rule must not
        be reached at all — otherwise `git checkout -b README` would resolve as a path and
        walk straight past the guard."""
        self._denied_as_the_branch_guard("git checkout -b README")

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

    def test_switch_detach_with_no_operand_is_refused(self):
        self.assertEqual(_decision(self.run_cmd("git switch --detach")), "deny")

    def test_detach_at_a_named_ref_is_refused(self):
        self.assertEqual(_decision(self.run_cmd("git checkout --detach feature")), "deny")

    def test_detach_does_not_let_a_path_operand_excuse_it(self):
        """`--detach` is never a restore, so the operand rule must not be reached — a
        `--detach` with a tracked path is nonsense git rejects, and reading it as a restore
        would be a two-word bypass on the case above."""
        self.assertEqual(_decision(self.run_cmd("git checkout --detach README")), "deny")


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
