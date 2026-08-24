"""A `git reset` that would destroy unpushed commits in the plane root is REFUSED (#401).

#373 lost eleven memory commits to one `git reset --hard origin/main` typed in the plane
root. Nothing about that command is exotic: it is the standard move on noticing a branch is
ahead of its remote for reasons you did not intend, and that is precisely the state a
protected-branch rejection of charter's reactive memory push leaves behind. #373 closed the
half that *describes* the hazard — `doctor` and the status line both name it now, the status
line every turn. Describing is not preventing, and the plane-root guard that could prevent it
matched on `checkout`/`switch` alone.

The whole design risk here is the other direction, so most of this file is about what stays
allowed. `reset` is not a rare command and it is not usually destructive: the unstage
(`git reset HEAD -- <path>`) is the single most common one an agent types, `--soft HEAD~1` is
an amend, and `--hard` with no ref throws away uncommitted work and no commits at all. A
guard that refused those would be worked around within a day and would then protect nothing.

So the condition is measured, not assumed: **this reset, in the plane root, would take
commits off the branch that no remote has a copy of.** Every test below is either that fact
being true and the command refused, or that fact being false and the same command allowed.
"""

from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class PlaneRootAheadCase(PersonaIso):
    """A plane root that tracks a real upstream and is two commits ahead of it.

    A bare repo stands in for the forge because the fact under test — "no remote has a copy
    of this commit" — is not expressible in a repo with no remote at all. `origin/main` here
    is a genuine already-fetched ref, which is the only thing the guard ever reads: it must
    never reach the network, and a test whose fixture faked the ref would not prove that.
    """

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        self.remote = self.tmp / "origin.git"
        self._git("init", "-q", "--bare", "-b", "main", str(self.remote))
        self._git("init", "-q", "-b", "main", str(self.root))
        self._in(self.root, "config", "commit.gpgsign", "false")
        self._in(self.root, "config", "user.email", "t@e")
        self._in(self.root, "config", "user.name", "t")
        self._in(self.root, "remote", "add", "origin", str(self.remote))
        self.commit("README", "plane\n", "init")
        self._in(self.root, "push", "-q", "-u", "origin", "main")
        # The two that only exist here — memory commits, in the shape #373 lost them.
        self.commit("personas/p/memory/one.md", "learned one\n", "memory: one")
        self.commit("personas/p/memory/two.md", "learned two\n", "memory: two")
        self.clone = config.WORKSPACES_DIR / "ws" / "svc"
        self.clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(self.clone))

    def commit(self, rel: str, body: str, msg: str) -> None:
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
        self._in(self.root, "add", "-A")
        self._in(self.root, "commit", "-qm", msg)

    def land(self) -> None:
        """Push what is here, which is the remedy the denial names."""
        self._in(self.root, "push", "-q", "origin", "main")

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


class TestThePreconditionHolds(PlaneRootAheadCase):
    """A fixture that stopped being two commits ahead would make every denial below
    unfailable while still passing. Asserted rather than assumed."""

    def test_the_root_is_two_commits_ahead_of_its_upstream(self):
        r = subprocess.run(["git", "-C", str(self.root), "rev-list", "--count",
                            "@{upstream}..HEAD"], capture_output=True, text=True)
        self.assertEqual("2", r.stdout.strip())


class TestDestroyingUnpushedCommitsIsRefused(PlaneRootAheadCase):
    def test_reset_hard_to_the_upstream_is_denied(self):
        """The exact command from #373."""
        self.assertEqual(_decision(self.run_cmd("git reset --hard origin/main")), "deny")

    def test_reset_hard_backwards_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git reset --hard HEAD~2")), "deny")

    def test_dropping_only_one_of_them_is_denied(self):
        """One unpublished commit is enough; the guard is not counting to a threshold."""
        self.assertEqual(_decision(self.run_cmd("git reset --hard HEAD~1")), "deny")

    def test_the_other_tree_overwriting_modes_are_denied_too(self):
        """`--keep` and `--merge` reset the tree to the target exactly as `--hard` does, so
        the dropped commits' files leave the disk the same way. Listing only `--hard` would
        leave a five-character bypass on the one refusal that exists because content was
        lost."""
        for mode in ("--keep", "--merge"):
            self.assertEqual(_decision(self.run_cmd(f"git reset {mode} origin/main")),
                             "deny", mode)

    def test_it_fires_from_a_workspace_clone_via_dash_c(self):
        """`git -C <plane> reset --hard origin/main` is how a session standing somewhere
        else reaches the shared tree."""
        self.assertEqual(_decision(self.run_cmd(f"git -C {self.root} reset --hard origin/main",
                                                cwd=self.clone)), "deny")

    def test_a_global_config_flag_does_not_walk_past_the_guard(self):
        """`git -c <k>=<v> <sub>` is a form agents type already — it is this repo's own
        commit convention. A guard that read `commit.gpgsign=false` as the subcommand would
        be one flag wide."""
        self.assertEqual(
            _decision(self.run_cmd("git -c advice.detachedHead=false reset --hard origin/main")),
            "deny")

    def test_a_flag_after_the_ref_is_still_the_same_command(self):
        """git's own parser accepts options after operands, and does the same thing."""
        self.assertEqual(_decision(self.run_cmd("git reset origin/main --hard")), "deny")


class TestTheDenialCanBeActedOn(PlaneRootAheadCase):
    def test_it_says_how_many_commits_and_that_they_are_unpublished(self):
        reason = _reason(self.run_cmd("git reset --hard origin/main"))
        self.assertIn("2 commits", reason)
        self.assertIn("origin/main", reason)

    def test_it_names_the_command_that_shows_what_would_be_lost(self):
        """A refusal whose subject you cannot inspect is one you route around."""
        reason = _reason(self.run_cmd("git reset --hard origin/main"))
        self.assertIn("@{upstream}..HEAD", reason)
        self.assertIn(str(self.root), reason)

    def test_it_names_charter_save_as_the_way_to_keep_them(self):
        self.assertIn("charter save", _reason(self.run_cmd("git reset --hard origin/main")))

    def test_the_guard_clears_itself_once_the_commits_land(self):
        """The remedy has to actually end the refusal, or the denial is a dead end. Pushing
        is what the denial tells you to do, and after it the same command runs."""
        self.assertEqual(_decision(self.run_cmd("git reset --hard origin/main")), "deny")
        self.land()
        self.assertIsNone(_decision(self.run_cmd("git reset --hard origin/main")))


class TestItDoesNotOverreach(PlaneRootAheadCase):
    def test_unstaging_a_path_is_allowed(self):
        """The single most common `reset` an agent types. It rewrites the index for one
        path and moves no commit anywhere."""
        (self.root / "scratch").write_text("x\n")
        self._in(self.root, "add", "scratch")
        self.assertIsNone(_decision(self.run_cmd("git reset HEAD -- scratch")))

    def test_unstaging_a_bare_path_is_allowed(self):
        """Without the trailing `--` in the guard's own rev-list, git reads this operand as
        a pathspec and answers with a count — and the unstage gets denied."""
        (self.root / "README").write_text("changed\n")
        self._in(self.root, "add", "README")
        self.assertIsNone(_decision(self.run_cmd("git reset README")))

    def test_a_hard_reset_of_a_path_is_not_read_as_a_ref(self):
        """The operand is a FILE — someone reaching for "throw away my edit to this one".
        git refuses `--hard` with paths outright, so nothing happens either way, and a
        charter denial claiming commits were about to die would be about nothing. Left to
        itself git reads such an operand as a *pathspec* and answers with a count, so the
        path here is deliberately one an unpushed commit introduced: the case where a
        pathspec reading comes back non-zero. What is pinned is the outcome, not which of
        the guard's two defences produced it."""
        (self.root / "personas/p/memory/two.md").write_text("edited\n")
        self.assertIsNone(_decision(
            self.run_cmd("git reset --hard personas/p/memory/two.md")))

    def test_a_separator_with_paths_after_it_is_not_a_ref_move(self):
        """`--` means what follows is a path, and the operands before it stop being the
        subject. Same reading the branch guard settled on."""
        self.assertIsNone(_decision(self.run_cmd("git reset --hard origin/main -- README")))

    def test_reset_hard_with_no_ref_is_allowed(self):
        """It discards uncommitted work — a different hazard, with an owner already
        (`doctor` counts dirty files) — and takes no commit off the branch."""
        self.assertIsNone(_decision(self.run_cmd("git reset --hard")))

    def test_reset_hard_to_head_is_allowed(self):
        """Same command spelled out. No commit leaves the branch."""
        self.assertIsNone(_decision(self.run_cmd("git reset --hard HEAD")))

    def test_soft_and_mixed_resets_are_allowed(self):
        """They take the branch off the same commits but leave every byte in the working
        tree, so the next `charter save` commits and pushes that content again. Denying
        them would buy nothing and would cost the ordinary amend."""
        for cmd in ("git reset --soft HEAD~1", "git reset --mixed HEAD~1",
                    "git reset HEAD~1", "git reset origin/main"):
            self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)

    def test_resetting_a_published_commit_away_is_allowed(self):
        """Once everything is on the remote, `git reset --hard HEAD~1` is recoverable in one
        fetch. That is ordinary work, and the guard must be silent through it."""
        self.land()
        self.assertIsNone(_decision(self.run_cmd("git reset --hard HEAD~1")))
        self.assertIsNone(_decision(self.run_cmd("git reset --hard origin/main")))

    def test_resetting_inside_a_workspace_clone_is_allowed(self):
        """A clone is where destructive git belongs; nothing here is shared."""
        self.assertIsNone(_decision(self.run_cmd("git reset --hard HEAD~1", cwd=self.clone)))

    def test_a_root_with_no_upstream_is_not_guessed_about(self):
        """`@{upstream}` does not resolve, so charter cannot say what is published and does
        not pretend to — the same silence `doctor` keeps about drift on a plane that was
        `git init`-ed by hand."""
        self._in(self.root, "branch", "--unset-upstream")
        self.assertIsNone(_decision(self.run_cmd("git reset --hard HEAD~1")))

    def test_committing_and_reading_in_the_root_stay_allowed(self):
        for cmd in ("git commit -m x", "git status", "git log --oneline"):
            self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)

    def test_a_commit_message_mentioning_the_command_is_not_the_command(self):
        """The prose trap every guard in this module has had to survive."""
        self.assertIsNone(_decision(
            self.run_cmd('git commit -m "docs: never run git reset --hard origin/main"')))

    def test_a_cd_into_a_clone_first_moves_where_the_reset_lands(self):
        """#183's fix, which the shared walk carries for both plane-root guards: a `cd` in
        the same command moves where the later segments run."""
        self.assertIsNone(_decision(
            self.run_cmd(f"cd {self.clone} && git reset --hard HEAD~1")))


class TestEveryRouteToTheRootReachesThisGuardToo(PlaneRootAheadCase):
    """The routes are shared with the branch guard, and so are their holes (#477).

    `_plane_root_git` is one pair of eyes for both plane-root guards, precisely so a route
    taught to one is a route both can see. #477 was the case where that paid a debt in the
    other direction: `--git-dir`, `--work-tree` and their env spellings name a repository
    without naming a directory to stand in, and `_git_target` read only the cwd and `-C` —
    so `git --git-dir <plane>/.git reset --hard <ref>` destroyed unpushed commits in the
    plane root from a clone, with no refusal, exactly as the branch half did.

    Run from the CLONE, so the shell's own directory is never the answer.
    """

    def _denied(self, cmd: str) -> None:
        r = self.run_cmd(cmd, cwd=self.clone)
        self.assertEqual(_decision(r), "deny", cmd)
        self.assertIn("2 commits", _reason(r), cmd)

    def test_git_dir_attached(self):
        self._denied(f"git --git-dir={self.root}/.git reset --hard origin/main")

    def test_git_dir_separated(self):
        self._denied(f"git --git-dir {self.root}/.git reset --hard origin/main")

    def test_work_tree_and_git_dir(self):
        self._denied(f"git --work-tree={self.root} --git-dir={self.root}/.git "
                     f"reset --hard origin/main")

    def test_the_environment_spelling(self):
        self._denied(f"GIT_DIR={self.root}/.git git reset --hard origin/main")

    def test_a_git_dir_relative_to_a_dash_C(self):
        up = os.path.relpath(self.root, self.clone)
        self._denied(f"git -C {up} --git-dir=.git reset --hard origin/main")

    def test_the_clones_own_git_dir_is_not_the_root(self):
        """The reach is the repository the option NAMES, not "any command with a --git-dir
        in it". Asserted through the BRANCH guard, which needs no upstream to speak: a reset
        in this clone is silent whatever the route, because nothing here is unpushed, so it
        could not tell a working route from a broken one."""
        self.assertIsNone(_decision(
            self.run_cmd(f"git --git-dir={self.clone}/.git switch -c feature/x",
                         cwd=self.clone)))


class TestAnAliasIsAnotherSpellingOfReset(PlaneRootAheadCase):
    """`git reset` is not the only way to spell `git reset` (#467).

    The branch guard has followed aliases since #461's round two, for a reason its docstring
    states plainly: `co = checkout` is on a large share of developer machines, and `git co
    feature` moves the plane root's HEAD exactly as far. This guard — the one that exists
    because eleven memory commits were destroyed — kept comparing the subcommand token to
    the string `"reset"`, so the SAME alias route walked past it while its sibling refused a
    branch switch through it. One guard resolving what a command will really do and its twin
    matching the spelling is the split that let a fixed route stay open one guard over.

    Every command below is a real `git reset --hard` wearing another name, and each is run
    against the fixture that has two commits no remote holds — so the denial it must produce
    is the measured one, not a guess about a shape.
    """

    def _denied_as_the_reset_guard(self, cmd: str) -> str:
        """Assert the REASON, not the status. `git wipe origin/main` would be refused by the
        branch guard too if the alias resolved to `checkout`, and a test that only read the
        decision could not tell which guard spoke — the failure mode this file's siblings
        record as "a guard passing because a DIFFERENT guard caught it"."""
        r = self.run_cmd(cmd)
        self.assertEqual(_decision(r), "deny", cmd)
        reason = _reason(r)
        self.assertIn("PLANE ROOT", reason, cmd)
        self.assertIn("2 commits", reason, cmd)
        self.assertIn("charter save", reason, cmd)
        return reason

    def test_a_command_line_alias_that_carries_the_whole_command(self):
        """#467's own spelling: the definition and the use are in one command line, and no
        `reset` token survives as the subcommand. git 2.50 runs it."""
        self._denied_as_the_reset_guard("git -c alias.z='reset --hard origin/main' z")

    def test_a_command_line_alias_that_carries_only_the_subcommand(self):
        """The caller's own arguments are appended to the expansion, exactly as git appends
        them — so the mode and the ref arrive from the other half of the argv."""
        self._denied_as_the_reset_guard("git -c alias.zz=reset zz --hard origin/main")

    def test_a_repo_config_alias(self):
        """The spelling the early exit could not see. `_plane_root_reset_reason` used to
        return on `"reset" not in cmd`, which is sound for a subcommand read as written and
        false the moment aliases are followed: these five characters are in the config, not
        in the command."""
        self._in(self.root, "config", "alias.wipe", "reset --hard")
        self._denied_as_the_reset_guard("git wipe origin/main")

    def test_an_alias_that_carries_the_mode_and_takes_the_ref(self):
        self._in(self.root, "config", "alias.nuke", "reset --hard origin/main")
        self._denied_as_the_reset_guard("git nuke")

    def test_an_alias_to_an_alias(self):
        """git follows the chain, so a guard that stopped at one hop would be one alias
        short of the same hole."""
        self._in(self.root, "config", "alias.wipe", "reset --hard")
        self._in(self.root, "config", "alias.w2", "wipe")
        self._denied_as_the_reset_guard("git w2 origin/main")

    def test_a_bang_alias_that_is_a_plain_git_command(self):
        """`!git reset --hard` is a git command wearing a shell alias's clothes."""
        self._in(self.root, "config", "alias.bang", "!git reset --hard")
        self._denied_as_the_reset_guard("git bang origin/main")

    def test_the_alias_route_reaches_the_root_from_a_clone(self):
        self._in(self.root, "config", "alias.wipe", "reset --hard")
        r = self.run_cmd(f"git -C {self.root} wipe origin/main", cwd=self.clone)
        self.assertEqual(_decision(r), "deny")
        self.assertIn("2 commits", _reason(r))

    def test_the_same_alias_inside_a_clone_is_untouched(self):
        """The widening is about WHAT the command is, not about where charter will refuse
        it. Branch and reset work in a clone is the workflow both denials recommend."""
        self._in(self.clone, "config", "alias.wipe", "reset --hard")
        self.assertIsNone(_decision(self.run_cmd("git wipe HEAD~1", cwd=self.clone)))

    def test_an_alias_that_is_not_a_reset_is_not_refused(self):
        """The resolution has to be able to answer "no", or it is not resolution."""
        self._in(self.root, "config", "alias.st", "status --short")
        self._in(self.root, "config", "alias.lg", "log --oneline")
        for cmd in ("git st", "git lg", "git -c alias.z='status --short' z"):
            self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)

    def test_an_alias_to_a_soft_reset_is_not_refused(self):
        """Following the alias does not widen what `reset` means: `--soft` leaves every byte
        on disk for the next `charter save`, and is allowed spelled either way."""
        self._in(self.root, "config", "alias.amend", "reset --soft HEAD~1")
        self.assertIsNone(_decision(self.run_cmd("git amend")))

    def test_a_shell_alias_charter_cannot_read_is_not_pretended_about(self):
        """`s = !sh -c '…'` runs a shell charter does not parse. It stands aside rather than
        refusing every shell alias in the plane root — the documented limit, pinned so a
        later claim of completeness fails here."""
        self._in(self.root, "config", "alias.sh1", "!sh -c 'git reset --hard origin/main'")
        self.assertIsNone(_decision(self.run_cmd("git sh1")))


class TestItIsScopedToAPlane(PlaneRootAheadCase):
    def test_no_control_plane_means_no_opinion(self):
        """The plugin installs per user and this handler runs everywhere. Outside a plane
        there is no plane root, and denying there explains a control plane that does not
        exist on that machine."""
        config.HAS_CONTROL_PLANE = False
        self.assertIsNone(_decision(self.run_cmd("git reset --hard origin/main")))


if __name__ == "__main__":
    unittest.main()
