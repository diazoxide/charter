"""Silence — a claim with no declaration, carrying an age (#98).

The case the whole spine exists for. A worker that hits a denial, times out, or is killed
declares nothing, and a branch with three commits and no further activity looks identical
whether its worker finished deliberately or died. Liveness is what separates them, and it
has to be recorded *for* the worker rather than by it: the worker we most need to catch is
precisely the one that did not remember.

What charter says about silence is its age and nothing else. No threshold turns it into a
verdict — a piece marked `failed` that was running a forty-minute test suite is ADR 0009's
exact failure one level up, and a confident wrong answer tells the reader to stop looking.
"""

from __future__ import annotations

import io
import json
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace

from charter import hooks, pieces, workspace, worktree
from charter import commands_worktree as cwt
from tests._isolation import PersonaIso, PlaneIso, run_hook

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


class SilenceCase(PlaneIso):
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
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            cwt.cmd_worktree_add(SimpleNamespace(repo="svc", piece="slice", branch=None,
                                                 workspace="alpha"))
        self.wt = worktree.path_for("alpha", "svc", "slice")

    def touch(self, cwd=None, sid="worker-A"):
        """A turn happening — the hook fires, the worker does nothing."""
        return run_hook(hooks.userpromptsubmit,
                        {"cwd": str(cwd or self.wt), "session_id": sid, "prompt": "carry on"})

    def listing(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cwt.cmd_worktree_list(SimpleNamespace(repo="svc", workspace="alpha"))
        return out.getvalue() + err.getvalue()

    def backdate(self, minutes: int):
        """Rewrite the last-seen mark into the past, since a test cannot wait an hour."""
        when = datetime.now(timezone.utc) - timedelta(minutes=minutes)
        pieces.seen("alpha", "svc", "slice", session="worker-A", when=when)


class TestLivenessIsRecordedForTheWorker(SilenceCase):
    def test_a_hook_records_liveness_with_no_action_by_the_worker(self):
        self.assertIsNone(pieces.last_seen("alpha", "svc", "slice"))
        self.touch()
        self.assertIsNotNone(pieces.last_seen("alpha", "svc", "slice"))

    def test_liveness_is_overwritten_rather_than_appended(self):
        """The hook fires every turn. Appending would bury three meaningful lines per piece
        under thousands, and make the log unbounded — so this is a different store, and
        `last seen at T` is a past observation with no history worth keeping."""
        for _ in range(5):
            self.touch()
        p = pieces.seen_path("alpha", "svc", "slice")
        self.assertEqual(len(p.read_text().strip().splitlines()), 1)

    def test_liveness_never_enters_the_declaration_log(self):
        self.touch()
        self.assertEqual([e["event"] for e in pieces.events("alpha")], ["claimed"])

    def test_a_session_outside_a_worktree_records_nothing(self):
        self.touch(cwd=self.clone)
        self.assertIsNone(pieces.last_seen("alpha", "svc", "slice"))

    def test_the_hook_never_fails_the_session(self):
        """Hooks are silent-on-error by discipline. Liveness is bookkeeping; a session must
        never die because it could not be written."""
        d = pieces.seen_path("alpha", "svc", "slice").parent
        d.mkdir(parents=True, exist_ok=True)
        d.chmod(0o500)
        self.addCleanup(d.chmod, 0o700)
        self.touch()  # must not raise
        self.assertIsNone(pieces.last_seen("alpha", "svc", "slice"))


class TestSilenceIsReported(SilenceCase):
    def test_a_claim_with_no_declaration_reads_as_silent_with_an_age(self):
        self.backdate(12)
        out = self.listing()
        self.assertIn("silent", out)
        self.assertIn("12m", out)

    def test_a_declared_piece_is_not_silent(self):
        """Silence is the absence of a declaration, so a declaration ends it. Reporting
        both would make `done` look like a problem."""
        self.backdate(90)
        pieces.record("alpha", "done", "svc", "slice")
        out = self.listing()
        self.assertIn("done", out)
        self.assertNotIn("silent", out)

    def test_silence_is_measured_from_the_claim_when_nothing_was_ever_seen(self):
        """A worker that died before its first turn left no liveness mark at all. The claim
        is still a moment in the past, so the age is knowable — falling back to "no idea"
        would lose the only case where the worker never got going."""
        self.assertIn("silent", self.listing())


class TestSilenceIsNeverDiagnosed(SilenceCase):
    def test_no_verdict_wording_however_long_the_silence(self):
        """Somebody will want a threshold here. ADR 0011 says no, and this is the test that
        makes adding one fail rather than merely disagree with a document."""
        for minutes in (1, 60, 60 * 24, 60 * 24 * 30):
            self.backdate(minutes)
            out = self.listing().lower()
            for verdict in ("failed", "dead", "stuck", "timed out", "timed-out",
                            "blocked", "abandoned", "orphan"):
                self.assertNotIn(verdict, out, f"{verdict!r} at {minutes}m")

    def test_the_age_grows_with_the_silence(self):
        self.backdate(3)
        self.assertIn("3m", self.listing())
        self.backdate(60 * 5)
        self.assertIn("5h", self.listing())
        self.backdate(60 * 24 * 3)
        self.assertIn("3d", self.listing())


class TestTheLivenessStoreIsSeparate(SilenceCase):
    def test_liveness_lives_beside_the_log_not_inside_it(self):
        self.touch()
        self.assertNotIn(".jsonl", pieces.seen_path("alpha", "svc", "slice").name)
        self.assertEqual(pieces.events("alpha")[0]["event"], "claimed")

    def test_a_malformed_liveness_mark_is_not_fatal(self):
        """An append-only world collects half-written files from killed processes; a
        listing that raises on one is worse than a listing that shows no age."""
        p = pieces.seen_path("alpha", "svc", "slice")
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{not json")
        self.assertIsNone(pieces.last_seen("alpha", "svc", "slice"))
        self.assertIn("silent", self.listing())


if __name__ == "__main__":
    unittest.main()
