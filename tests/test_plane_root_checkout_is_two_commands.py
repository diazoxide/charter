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
"""

from __future__ import annotations

import shlex
import shutil
import subprocess
import tempfile
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
            shutil.copytree(self.root, self._oracle_base / "template")
        self._oracle_n += 1
        scratch = self._oracle_base / f"run-{self._oracle_n}"
        shutil.copytree(self._oracle_base / "template", scratch)
        before = self._head(scratch)
        subprocess.run(shlex.split(cmd), cwd=scratch, capture_output=True, text=True,
                       timeout=60)
        return self._head(scratch) != before


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
    CORPUS = (
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
        "git switch --orphan neu", "git switch --orphan=neu",
        "git switch --detach feature", "git switch -d feature",
        # Restores and no-ops: nothing here may move HEAD, and the guard's verdict on them
        # is deliberately not asserted from this list.
        "git checkout", "git checkout README", "git checkout .", "git checkout notes",
        "git checkout feature README", "git checkout -- README",
        "git checkout --ours README", "git checkout -f README", "git checkout -q README",
        "git checkout --conflict=merge README", "git restore README",
    )

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
        self.assertGreaterEqual(len(movers), 20, movers)
        for must in ("git checkout --orphan README", "git checkout -bREADME",
                     "git switch -cneu", "git checkout -d feature"):
            self.assertIn(must, movers)

    def test_no_promised_restore_moves_head_and_every_one_is_allowed(self):
        """The other half, and the reason the fix is not "deny everything": these are the
        sentences charter's denial message and news entry make true."""
        for cmd in self.PROMISED_RESTORES:
            with self.subTest(cmd=cmd):
                self.assertFalse(self.moves_head(cmd), cmd)
                self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)


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
