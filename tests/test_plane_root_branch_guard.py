"""Branch switching in the plane root is REFUSED, not merely reported (#157).

ADR 0008 chose signal over refusal and said why prevention was not what shipped first:

    Preventing it outright — refusing commands that would operate in the root — is the real
    answer and is deliberately not what ships first. Which commands count is a judgement
    that wants evidence.

#157 is that evidence. One session switched the plane root between branches six times;
`doctor` printed its correct, complete, actionable warning on every run in between, and the
agent rationalised past it each time — the exact consumer charter is built for. The
operator's notes add the cost: two background agents in one working tree clobber each other
through `git checkout`, and the symptom looks like an unrelated mystery.

So this implements ADR 0008's own next step rather than contradicting it, and is scoped to
what the evidence shows: **moving HEAD between branches, in the plane root, and nothing
else**. In particular `doctor`'s remedy — `git -C <plane> checkout main` — has to keep
working, because a guard that blocks the fix it recommends is a trap rather than a guard.
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


class PlaneRootCase(PersonaIso):
    """A plane root that is a real git repo on `main`, plus a clone that is not the root."""

    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.root = Path(config.ROOT)
        self._git("init", "-q", "-b", "main", str(self.root))
        (self.root / "README").write_text("plane\n")
        self._in(self.root, "add", "-A")
        self._in(self.root, "-c", "commit.gpgsign=false", "-c", "user.email=t@e",
                 "-c", "user.name=t", "commit", "-qm", "init")
        self._in(self.root, "branch", "feature")
        self.clone = config.WORKSPACES_DIR / "ws" / "svc"
        self.clone.mkdir(parents=True, exist_ok=True)
        self._git("init", "-q", "-b", "main", str(self.clone))

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


class TestSwitchingBranchesInTheRootIsRefused(PlaneRootCase):
    def test_git_switch_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git switch feature")), "deny")

    def test_git_checkout_of_another_branch_is_denied(self):
        self.assertEqual(_decision(self.run_cmd("git checkout feature")), "deny")

    def test_creating_a_branch_is_denied(self):
        for flag in ("-b", "-B"):
            self.assertEqual(_decision(self.run_cmd(f"git checkout {flag} chore/x")),
                             "deny", flag)
        self.assertEqual(_decision(self.run_cmd("git switch -c chore/x")), "deny")

    def test_switching_back_to_the_previous_branch_is_denied(self):
        """`git checkout -` is the move that made the six-switch session cheap to repeat."""
        self.assertEqual(_decision(self.run_cmd("git checkout -")), "deny")

    def test_it_fires_from_anywhere_when_the_root_is_targeted(self):
        """`git -C <plane> checkout` is how a session in a workspace reaches the shared
        tree, so scoping the guard to the cwd alone would leave the door open."""
        self.assertEqual(_decision(self.run_cmd(f"git -C {self.root} checkout feature",
                                                cwd=self.clone)), "deny")

    def test_the_denial_names_the_alternative(self):
        """A refusal that only says no gets worked around. The remedy is the same one
        `doctor` prints: the work belongs in a workspace clone."""
        r = self.run_cmd("git checkout feature")
        self.assertIn("workspace", _reason(r).lower())


class TestTheRemedyStaysExecutable(PlaneRootCase):
    def test_checking_out_the_default_branch_is_allowed(self):
        """`doctor` tells you to run exactly this to undo the damage. Blocking it would
        make the guard a trap — the one carve-out that keeps this a guard rather than a
        cage."""
        self._in(self.root, "checkout", "-q", "feature")
        self.assertIsNone(_decision(self.run_cmd("git checkout main")))
        self.assertIsNone(_decision(self.run_cmd(f"git -C {self.root} checkout main")))

    def test_creating_a_branch_named_like_the_default_is_still_denied(self):
        """The carve-out is for RETURNING to the default, not for the name."""
        self.assertEqual(_decision(self.run_cmd("git checkout -b main")), "deny")


class TestItDoesNotOverreach(PlaneRootCase):
    def test_restoring_a_file_is_allowed(self):
        """`git checkout -- <path>` never moves HEAD. Denying it would block an ordinary
        undo and teach people the guard is noise."""
        self.assertIsNone(_decision(self.run_cmd("git checkout -- README")))

    def test_committing_in_the_root_is_allowed(self):
        """`charter save` commits in the root by design. Advancing HEAD along the branch
        you are already on is not the failure this guards."""
        self.assertIsNone(_decision(self.run_cmd("git commit -m x")))

    def test_switching_inside_a_workspace_clone_is_allowed(self):
        """The whole point is that branch work belongs in a clone."""
        self.assertIsNone(_decision(self.run_cmd("git checkout -b feature/x",
                                                 cwd=self.clone)))

    def test_status_and_log_are_allowed(self):
        for cmd in ("git status", "git log --oneline", "git branch --list"):
            self.assertIsNone(_decision(self.run_cmd(cmd)), cmd)

    def test_a_commit_message_mentioning_checkout_is_not_a_switch(self):
        """The prose trap every other guard in this module already had to survive."""
        self.assertIsNone(_decision(
            self.run_cmd('git commit -m "docs: explain git checkout main"')))


class TestItIsScopedToAPlane(PlaneRootCase):
    def test_no_control_plane_means_no_opinion(self):
        """The plugin installs per user; the handler runs everywhere. Outside a plane there
        is no plane root to protect, and denying there explains a plane that does not
        exist — the mistake `_single_credential_reason` was gated to stop repeating."""
        config.HAS_CONTROL_PLANE = False
        self.assertIsNone(_decision(self.run_cmd("git checkout feature")))


if __name__ == "__main__":
    unittest.main()
