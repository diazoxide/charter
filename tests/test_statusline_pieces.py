"""The fleet in the status line (#100).

Sub-problem 2 lands here because this is the surface people actually look at: the footer
already draws worktree rows, so a piece's declaration — or its silence, with an age — costs
no new row and no new command.

Two constraints govern everything below. The status line **may not fork a git subprocess**
(its own contract: fast, no network, branches read straight from `.git/HEAD`), and it must
**never raise** — a malformed record has to render, not blank the footer. The obvious
implementation of this feature reaches straight for `git worktree list`, which is why the
first of those gets a test rather than a comment.

And no verdict, at any age. That rule is the same one ADR 0009 sets for error text, and the
status line is where a threshold would be most tempting to add.
"""
from __future__ import annotations

import os
import re
import subprocess
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

from charter import config, pieces, statusline, workspace
from tests._isolation import PersonaIso


def git(repo: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(repo), *args],
                          check=True, capture_output=True, text=True)


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


class PieceStatusCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        self.ws = config.DEFAULT_WORKSPACE
        self.repo = config.WORKSPACES_DIR / self.ws / "demo"
        self.repo.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "init", "-q", "-b", "main", str(self.repo)],
                       check=True, capture_output=True)
        git(self.repo, "config", "user.email", "t@example.com")
        git(self.repo, "config", "user.name", "t")
        (self.repo / "README.md").write_text("hello\n")
        git(self.repo, "add", "README.md")
        git(self.repo, "-c", "commit.gpgsign=false", "commit", "-qm", "init")

    def add_piece(self, piece: str):
        from charter import worktree
        p = worktree.path_for(self.ws, "demo", piece)
        p.parent.mkdir(parents=True, exist_ok=True)
        git(self.repo, "worktree", "add", "-q", str(p), "-b", piece)
        pieces.record(self.ws, "claimed", "demo", piece)
        return p

    def backdate(self, piece: str, minutes: int):
        pieces.seen(self.ws, "demo", piece, session="w",
                    when=datetime.now(timezone.utc) - timedelta(minutes=minutes))

    def render(self, width: int = 200) -> str:
        old = os.environ.get("COLUMNS")
        os.environ["COLUMNS"] = str(width)
        try:
            return _plain(statusline.render({"session_id": "t"}))
        finally:
            if old is None:
                os.environ.pop("COLUMNS", None)
            else:
                os.environ["COLUMNS"] = old


class TestRowsCarryTheOutcome(PieceStatusCase):
    def test_a_declared_piece_shows_its_declaration(self):
        self.add_piece("parser")
        pieces.record(self.ws, "done", "demo", "parser")
        self.assertIn("done", self.render())

    def test_an_abandoned_piece_shows_that(self):
        self.add_piece("lexer")
        pieces.record(self.ws, "abandoned", "demo", "lexer", reason="needs a live token")
        self.assertIn("abandoned", self.render())

    def test_an_undeclared_piece_shows_its_silence_age(self):
        self.add_piece("codegen")
        self.backdate("codegen", 45)
        out = self.render()
        self.assertIn("silent", out)
        self.assertIn("45m", out)


class TestTheWorkspaceLineSummarises(PieceStatusCase):
    def test_the_counts_appear_on_the_workspace_line(self):
        for p in ("a", "b", "c"):
            self.add_piece(p)
        pieces.record(self.ws, "done", "demo", "a")
        pieces.record(self.ws, "abandoned", "demo", "b", reason="x")
        out = self.render()
        self.assertIn("pieces", out)
        self.assertRegex(out, r"pieces\s*3")

    def test_a_workspace_with_no_pieces_says_nothing_about_them(self):
        """An empty count is not news. `todo` is already omitted at zero for exactly this
        reason — a line that always fires is a line nobody reads."""
        self.assertNotIn("pieces", self.render())


class TestNoVerdictAndNoSubprocess(PieceStatusCase):
    def test_no_verdict_wording_at_any_age(self):
        self.add_piece("codegen")
        for minutes in (1, 60, 60 * 24, 60 * 24 * 40):
            self.backdate("codegen", minutes)
            out = self.render().lower()
            for verdict in ("failed", "dead", "stuck", "timed out", "timed-out", "orphan"):
                self.assertNotIn(verdict, out, f"{verdict!r} at {minutes}m")

    def test_the_piece_annotation_forks_no_git_subprocess(self):
        """The contract this feature is most likely to break, and it breaks silently — an
        extra subprocess per piece per turn costs every render and nothing fails.

        Asserted against the piece functions rather than the whole render, because the
        render already shells out: `_run_state` runs `git status --porcelain --branch` per
        tree. (The module docstring's "no git subprocess" is narrower than it reads — it is
        about BRANCHES, which come from `.git/HEAD`.) Pinning the whole render at zero
        would therefore be pinning a falsehood; what must stay true is that *this* feature
        adds nothing, and it reaches for `git worktree list` in the obvious implementation.
        """
        self.add_piece("parser")
        pieces.record(self.ws, "done", "demo", "parser")
        calls = []
        real = subprocess.run

        def spy(cmd, *a, **kw):
            calls.append(cmd)
            return real(cmd, *a, **kw)

        subprocess.run = spy
        self.addCleanup(setattr, subprocess, "run", real)
        statusline._piece_summary(self.ws)
        statusline._piece_state(self.ws, "demo", "parser")
        self.assertEqual(calls, [])


class TestItNeverBlanksTheFooter(PieceStatusCase):
    def test_a_malformed_record_still_renders(self):
        self.add_piece("parser")
        pieces.log_path(self.ws).write_text("{not json\n")
        self.assertIn("parser", self.render())

    def test_a_malformed_liveness_mark_still_renders(self):
        self.add_piece("parser")
        p = pieces.seen_path(self.ws, "demo", "parser")
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{nope")
        self.assertIn("parser", self.render())

    def test_an_unreadable_record_directory_still_renders(self):
        self.add_piece("parser")
        d = pieces.dir_for(self.ws)
        d.chmod(0o000)
        self.addCleanup(d.chmod, 0o700)
        self.assertIn("parser", self.render())


if __name__ == "__main__":
    unittest.main()
