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


class TestItIsScopedToAPlane(PlaneRootAheadCase):
    def test_no_control_plane_means_no_opinion(self):
        """The plugin installs per user and this handler runs everywhere. Outside a plane
        there is no plane root, and denying there explains a control plane that does not
        exist on that machine."""
        config.HAS_CONTROL_PLANE = False
        self.assertIsNone(_decision(self.run_cmd("git reset --hard origin/main")))


if __name__ == "__main__":
    unittest.main()
