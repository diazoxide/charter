"""Claiming a piece — `charter worktree add` as the act that takes it (#96).

Real git, per `tests/test_worktree.py`: git's own atomicity *is* the mutual exclusion here,
so a mocked git would prove nothing about the property that matters.

The record under test holds only what git cannot know. Which worktrees exist, which branch
each is on, whether it is dirty — all of that is read from git every time, and none of it
may appear here (ADR 0011). What is recorded is that a session took a piece.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import threading
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import pieces, workspace, worktree
from charter import commands_worktree as cwt
from tests._isolation import PersonaIso

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


class ClaimCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.enterContext(redirect_stdout(io.StringIO()))
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

    def add(self, piece: str = "slice", branch: str | None = None, repo: str = "svc"):
        """Returns (rc, stdout + stderr)."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cwt.cmd_worktree_add(SimpleNamespace(
                repo=repo, piece=piece, branch=branch, workspace="alpha"))
        return rc, out.getvalue() + err.getvalue()

    def listing(self, repo: str | None = "svc"):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cwt.cmd_worktree_list(SimpleNamespace(repo=repo, workspace="alpha"))
        return out.getvalue() + err.getvalue()


class TestClaimingRecordsWhoTookIt(ClaimCase):
    def test_a_claim_is_recorded(self):
        self.add()
        events = pieces.events("alpha")
        self.assertEqual([e["event"] for e in events], ["claimed"])
        self.assertEqual(events[0]["piece"], "slice")
        self.assertEqual(events[0]["repo"], "svc")

    def test_the_claim_carries_the_session_and_host(self):
        """Visibility needs a name. A worktree's existence proves *someone* took the piece
        and says nothing about who — git records an author only once a commit lands."""
        os.environ["CLAUDE_CODE_SESSION_ID"] = "sess-abc"
        self.addCleanup(os.environ.pop, "CLAUDE_CODE_SESSION_ID", None)
        self.add()
        claim = pieces.claim_for("alpha", "svc", "slice")
        self.assertEqual(claim["session"], "sess-abc")
        self.assertTrue(claim["host"])

    def test_a_claim_with_no_session_is_still_recorded(self):
        """Absence is representable — `session.current()` returns None rather than a shared
        sentinel, and a claim made outside a Claude session is still a claim."""
        os.environ.pop("CLAUDE_CODE_SESSION_ID", None)
        self.add()
        claim = pieces.claim_for("alpha", "svc", "slice")
        self.assertIsNone(claim["session"])

    def test_nothing_derivable_from_git_is_recorded(self):
        """ADR 0011's whole discipline, asserted as a closed set. A record with one extra
        convenient field looks like a convenience and is the reversal of that decision."""
        self.add()
        allowed = {"ts", "event", "repo", "piece", "session", "host", "persona"}
        for e in pieces.events("alpha"):
            self.assertEqual(set(e) - allowed, set())
        text = pieces.log_path("alpha").read_text()
        for derivable in ("branch", "path", "dirty", "pushed", "upstream", "detached"):
            self.assertNotIn(derivable, text)


class TestTheListingNamesTheClaimant(ClaimCase):
    def test_the_claimant_appears_in_the_listing(self):
        os.environ["CLAUDE_CODE_SESSION_ID"] = "sess-abc"
        self.addCleanup(os.environ.pop, "CLAUDE_CODE_SESSION_ID", None)
        self.add()
        self.assertIn("sess-abc", self.listing())

    def test_a_hand_made_worktree_is_listed_as_claimant_unknown(self):
        """`worktree.py` holds that a worktree made with plain git is first-class. This
        design does not weaken that — it has no claim event, so it has no claimant, and
        saying so is different from refusing it."""
        wt = worktree.path_for("alpha", "svc", "byhand")
        wt.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "-C", str(self.clone), "worktree", "add", "-q",
                        "-b", "byhand", str(wt)], check=True, capture_output=True,
                       env={**os.environ, **_GIT_ENV})
        out = self.listing()
        self.assertIn("byhand", out)
        self.assertIn("unknown", out)
        self.assertEqual(pieces.claim_for("alpha", "svc", "byhand"), None)


class TestExactlyOneClaimWins(ClaimCase):
    def test_a_second_claim_on_the_same_piece_loses(self):
        rc1, _ = self.add()
        rc2, _ = self.add()
        self.assertEqual(rc1, 0)
        self.assertEqual(rc2, cwt.CLAIM_TAKEN)

    def test_the_loser_exits_distinctly_from_every_other_failure(self):
        """The exit code is what makes this usable by a worker moving to the next name in
        its plan. An invalid piece name must not look like a lost race."""
        self.add()
        taken, _ = self.add()
        invalid, _ = self.add(piece=".hidden")
        missing, _ = self.add(repo="nope")
        self.assertEqual(taken, cwt.CLAIM_TAKEN)
        self.assertNotEqual(invalid, cwt.CLAIM_TAKEN)
        self.assertNotEqual(missing, cwt.CLAIM_TAKEN)

    def test_the_loser_names_the_recognised_cause(self):
        self.add()
        _, out = self.add()
        self.assertIn("slice", out)
        self.assertIn("claim", out.lower())

    def test_a_branch_held_by_another_piece_is_a_lost_claim(self):
        """Same collision wearing a different name: the branch is checked out by a live
        worktree, so somebody holds it even though this piece's path is free."""
        self.add(piece="first")
        rc, out = self.add(piece="second", branch="first")
        self.assertEqual(rc, cwt.CLAIM_TAKEN)
        self.assertIn("first", out)

    def test_a_branch_that_exists_but_nobody_holds_is_not_a_lost_claim(self):
        """Specificity. A leftover branch with no worktree on it is not a claim, and
        reporting it as one would teach workers to skip names that were free."""
        subprocess.run(["git", "-C", str(self.clone), "branch", "leftover"], check=True,
                       capture_output=True, env={**os.environ, **_GIT_ENV})
        rc, _ = self.add(piece="leftover")
        self.assertNotEqual(rc, 0)
        self.assertNotEqual(rc, cwt.CLAIM_TAKEN)

    def test_losing_the_race_inside_git_is_still_a_lost_claim(self):
        """The pre-check is not the mutex — git is. A racer that wins between our check and
        git's lock makes `git worktree add` fail, and that failure must classify the same
        way, or which error a worker sees becomes a matter of timing.

        The cause is established by re-reading reality, never by parsing git's English
        (ADR 0009): after the failure, the path is there.
        """
        real_run = cwt.util.run

        def racer(cmd, *a, **kw):
            if "worktree" in cmd and "add" in cmd:
                worktree.path_for("alpha", "svc", "slice").mkdir(parents=True, exist_ok=True)
                return subprocess.CompletedProcess(cmd, 1, "", "fatal: whatever git says")
            return real_run(cmd, *a, **kw)

        cwt.util.run = racer
        self.addCleanup(setattr, cwt.util, "run", real_run)
        rc, out = self.add()
        self.assertEqual(rc, cwt.CLAIM_TAKEN)

    def test_concurrent_claims_leave_exactly_one_winner(self):
        """The property the whole design rests on, exercised concurrently rather than
        argued for. Git's lock is the arbiter; charter adds nothing to it."""
        results, barrier = [], threading.Barrier(4)
        lock = threading.Lock()

        def claim():
            barrier.wait()
            rc, _ = self.add(piece="contested")
            with lock:
                results.append(rc)

        threads = [threading.Thread(target=claim) for _ in range(4)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(results.count(0), 1, results)


class TestTheRecordIsParallelWriterSafe(ClaimCase):
    def test_concurrent_appends_all_survive_intact(self):
        """Invisible from a single-process CLI test, so it gets a direct one. `O_APPEND`
        with no lock is the `dispatch.py` pattern — small lines interleave atomically."""
        barrier = threading.Barrier(8)

        def write(i: int):
            barrier.wait()
            for j in range(25):
                pieces.record("alpha", "claimed", "svc", f"p{i}-{j}")

        threads = [threading.Thread(target=write, args=(i,)) for i in range(8)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        lines = pieces.log_path("alpha").read_text().splitlines()
        self.assertEqual(len(lines), 200)
        for line in lines:
            json.loads(line)  # every line is whole — no interleaved fragments

    def test_a_write_failure_never_breaks_the_caller(self):
        """A record is bookkeeping. Claiming a piece must not fail because the log could
        not be written — the worktree is the claim, and it already exists."""
        d = pieces.log_path("alpha").parent
        d.mkdir(parents=True, exist_ok=True)
        d.chmod(0o500)
        self.addCleanup(d.chmod, 0o700)
        self.assertIsNone(pieces.record("alpha", "claimed", "svc", "slice"))


class TestTheRecordIsNotShared(ClaimCase):
    def test_the_record_is_not_in_the_live_sharing_block(self):
        """It describes worktrees that exist on exactly one disk. Committing it recreates
        the mismatch ADR 0010 dissects — a portable file describing a local reality."""
        block = workspace._live_block(["alpha"])
        self.assertNotIn(pieces.DIR_NAME, block)

    def test_the_record_lives_inside_the_workspace(self):
        self.assertEqual(pieces.log_path("alpha").parent.parent,
                         workspace.workspace_dir("alpha"))


if __name__ == "__main__":
    unittest.main()
