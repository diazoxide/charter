"""A removed piece leaves the listing but keeps its history (#101).

Two questions, two answers, neither allowed to impersonate the other. *What is running
here* is git's question, and a piece whose worktree is gone has no reality left to report.
*What happened here* is the log's, and it survives the worktree — which is the only way to
answer "did that abandoned piece ever get retried?" once it has been curated away.

ADR 0010 is the same lesson learned the expensive way, with the manifest and the directory
scan: where two sources answer a question, name which question each answers.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import pieces, workspace, worktree
from charter import commands_worktree as cwt
from tests._isolation import PersonaIso

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


class HistoryCase(PersonaIso):
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

    def add(self, piece: str):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return cwt.cmd_worktree_add(SimpleNamespace(repo="svc", piece=piece,
                                                        branch=None, workspace="alpha"))

    def remove(self, piece: str):
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return cwt.cmd_worktree_remove(SimpleNamespace(
                repo="svc", piece=piece, force=True, delete_branch=False, workspace="alpha"))

    def listing(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cwt.cmd_worktree_list(SimpleNamespace(repo="svc", workspace="alpha"))
        return out.getvalue() + err.getvalue()

    def history(self, repo=None, piece=None):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = cwt.cmd_worktree_history(SimpleNamespace(repo=repo, piece=piece,
                                                          workspace="alpha"))
        return rc, out.getvalue() + err.getvalue()


class TestRemovalEndsTheListing(HistoryCase):
    def test_a_removed_piece_drops_out_of_the_listing(self):
        self.add("slice")
        self.assertIn("slice", self.listing())
        self.remove("slice")
        self.assertNotIn("slice", self.listing())

    def test_the_listing_never_sources_existence_from_the_record(self):
        """The sharp version: a claim with no worktree. Keeping it visible is exactly the
        stale marker `worktree.py` refuses to carry, wearing this design's clothes."""
        pieces.record("alpha", "claimed", "svc", "ghost")
        self.assertNotIn("ghost", self.listing())


class TestHistorySurvivesRemoval(HistoryCase):
    def test_a_removed_pieces_history_is_still_readable(self):
        self.add("slice")
        pieces.record("alpha", "abandoned", "svc", "slice", reason="ran out of rope")
        self.remove("slice")
        rc, out = self.history()
        self.assertEqual(rc, 0)
        self.assertIn("slice", out)
        self.assertIn("abandoned", out)
        self.assertIn("ran out of rope", out)

    def test_removal_prunes_nothing(self):
        """The log is a few lines per piece and is the last evidence an abandoned piece
        leaves once curated away. A retention policy deserves a reason, not a default."""
        self.add("slice")
        before = pieces.events("alpha")
        self.remove("slice")
        self.assertEqual(pieces.events("alpha"), before)

    def test_history_covers_live_pieces_as_well_as_removed_ones(self):
        self.add("gone")
        self.add("living")
        self.remove("gone")
        _, out = self.history()
        self.assertIn("gone", out)
        self.assertIn("living", out)


class TestReadingHistory(HistoryCase):
    def test_history_shows_events_oldest_first(self):
        self.add("slice")
        pieces.record("alpha", "done", "svc", "slice")
        _, out = self.history()
        self.assertLess(out.index("claimed"), out.index("done"))

    def test_history_can_narrow_to_one_piece(self):
        self.add("wanted")
        self.add("other")
        _, out = self.history(repo="svc", piece="wanted")
        self.assertIn("wanted", out)
        self.assertNotIn("other", out)

    def test_history_with_nothing_recorded_says_so_rather_than_printing_nothing(self):
        rc, out = self.history()
        self.assertEqual(rc, 0)
        self.assertTrue(out.strip())

    def test_history_names_the_claimant(self):
        os.environ["CLAUDE_CODE_SESSION_ID"] = "worker-A"
        self.addCleanup(os.environ.pop, "CLAUDE_CODE_SESSION_ID", None)
        self.add("slice")
        _, out = self.history()
        self.assertIn("worker-A", out)


if __name__ == "__main__":
    unittest.main()
