"""SessionStart tells a worker which piece it holds and what it owes (#99).

Nothing in this design works if workers do not declare, and a worker is just a session that
happens to be sitting in a worktree — it has no reason to know charter wants anything from
it. So it is told, by the mechanism that already announces workspace, persona and todos.

The verbs are stated literally, because an obligation that is not spelled out is one that
will not be met.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace

from charter import hooks, pieces, workspace, worktree
from charter import commands_worktree as cwt
from tests._isolation import PersonaIso, run_hook

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


def _context(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


class AnnounceCase(PersonaIso):
    """Workspace pinned through the env var, as `test_todos_session_injection.py` does, so
    the confirm nudge's several hundred words are not most of the text every assertion
    here has to search."""

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(redirect_stdout(io.StringIO()))
        os.environ["CHARTER_WORKSPACE"] = "alpha"
        self.addCleanup(os.environ.pop, "CHARTER_WORKSPACE", None)
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        self.clone = workspace.workspace_dir("alpha") / "svc"
        self.clone.mkdir(parents=True, exist_ok=True)
        env = {**os.environ, **_GIT_ENV}
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.clone)], check=True,
                       capture_output=True, env=env)
        (self.clone / "README").write_text("base\n")
        subprocess.run(["git", "-C", str(self.clone), "add", "-A"], check=True,
                       capture_output=True, env=env)
        subprocess.run(["git", "-C", str(self.clone), "-c", "commit.gpgsign=false",
                        "commit", "-qm", "base"], check=True, capture_output=True, env=env)
        os.environ["CLAUDE_CODE_SESSION_ID"] = "worker-A"
        self.addCleanup(os.environ.pop, "CLAUDE_CODE_SESSION_ID", None)
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            cwt.cmd_worktree_add(SimpleNamespace(repo="svc", piece="slice", branch=None,
                                                 workspace="alpha"))
        self.wt = worktree.path_for("alpha", "svc", "slice")

    def start(self, cwd=None, sid="worker-A", source="startup") -> str:
        return _context(run_hook(hooks.sessionstart, {
            "cwd": str(cwd if cwd is not None else self.wt),
            "session_id": sid, "source": source}))


class TestTheWorkerIsToldWhatItHolds(AnnounceCase):
    def test_a_session_inside_a_worktree_is_told_its_piece(self):
        out = self.start()
        self.assertIn("slice", out)
        self.assertIn("svc", out)

    def test_the_verbs_are_stated_literally(self):
        """Naming the obligation without naming the commands is how it goes unmet."""
        out = self.start()
        self.assertIn("charter worktree done", out)
        self.assertIn("charter worktree abandon", out)

    def test_a_session_outside_a_worktree_hears_nothing_about_pieces(self):
        out = self.start(cwd=self.clone)
        self.assertNotIn("slice", out)
        self.assertNotIn("charter worktree done", out)

    def test_a_declared_piece_is_not_asked_to_declare_again(self):
        """The obligation is discharged. Repeating it is how a reminder becomes noise."""
        pieces.record("alpha", "done", "svc", "slice")
        out = self.start()
        self.assertNotIn("charter worktree done", out)


class TestCollisionIsReportedNotRefused(AnnounceCase):
    def test_entering_a_piece_another_session_holds_says_so(self):
        out = self.start(sid="worker-B")
        self.assertIn("worker-A", out)

    def test_the_collision_carries_the_holders_last_seen_age(self):
        pieces.seen("alpha", "svc", "slice", session="worker-A",
                    when=datetime.now(timezone.utc) - timedelta(minutes=20))
        out = self.start(sid="worker-B")
        self.assertIn("20m", out)

    def test_the_session_is_not_refused(self):
        """ADR 0008's trade, knowingly repeated: signal, not refusal. There are legitimate
        second sessions — a human inspecting a worker's tree among them."""
        rc = run_hook(hooks.sessionstart,
                      {"cwd": str(self.wt), "session_id": "worker-B", "source": "startup"})
        self.assertIsNotNone(rc)  # emitted context, exited normally

    def test_my_own_claim_does_not_warn_me(self):
        out = self.start(sid="worker-A")
        self.assertNotIn("already", out.lower())

    def test_a_resumed_session_is_not_warned_about_itself(self):
        """A resumed session gets a new id, so the id check alone would fire on every
        resume — which is the fastest way to make the signal ignored."""
        for source in ("resume", "clear", "compact"):
            out = self.start(sid="worker-A-resumed", source=source)
            self.assertNotIn("worker-A", out, source)

    def test_the_holders_liveness_is_not_clobbered_by_the_visitor(self):
        """The ordering trap: liveness is written at the top of the handler, so a naive
        implementation overwrites the holder's mark with the visitor's before reading it —
        and the warning could then never fire twice."""
        pieces.seen("alpha", "svc", "slice", session="worker-A",
                    when=datetime.now(timezone.utc) - timedelta(minutes=20))
        self.start(sid="worker-B")
        self.assertIn("worker-A", self.start(sid="worker-C"))


class TestNoTurnEndNagging(AnnounceCase):
    def test_the_reminder_does_not_repeat_every_turn(self):
        """Liveness is already automatic, so a per-turn reminder adds nothing but noise —
        and ADR 0008 records what becomes of a warning you can work through."""
        out = _context(run_hook(hooks.userpromptsubmit,
                                {"cwd": str(self.wt), "session_id": "worker-A",
                                 "prompt": "carry on"}))
        self.assertNotIn("charter worktree done", out)


if __name__ == "__main__":
    unittest.main()
