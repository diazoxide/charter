"""Declaring a piece done or abandoned (#97).

The only two declarations there are. `failed`, `blocked` and `timed-out` are absent on
purpose: charter can verify none of them, and a state nobody can verify is the marker that
lies (ADR 0009, ADR 0011). A worker that dies declares nothing at all, and that absence is
the subject of #98 — not a third value here.

Neither verb takes a piece argument. The piece comes from where you are standing, because
every argument is an opportunity to name someone else's piece.
"""

from __future__ import annotations

import io
import os
import subprocess
import unittest
from contextlib import chdir, redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import pieces, workspace, worktree
from charter import commands_worktree as cwt
from tests._isolation import PersonaIso

_GIT_ENV = {"GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
            "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}


class DeclareCase(PersonaIso):
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

    def declare(self, verb: str, reason: str | None = None, cwd=None):
        """Returns (rc, stdout + stderr). Run from *cwd*, since that is what names the
        piece — the whole point of the two verbs taking no argument."""
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace() if reason is None else SimpleNamespace(reason=reason)
        fn = cwt.cmd_worktree_done if verb == "done" else cwt.cmd_worktree_abandon
        with chdir(cwd or self.wt), redirect_stdout(out), redirect_stderr(err):
            rc = fn(args)
        return rc, out.getvalue() + err.getvalue()

    def listing(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cwt.cmd_worktree_list(SimpleNamespace(repo="svc", workspace="alpha"))
        return out.getvalue() + err.getvalue()


class TestDeclaringDone(DeclareCase):
    def test_done_records_a_declaration_for_the_piece_you_are_in(self):
        rc, _ = self.declare("done")
        self.assertEqual(rc, 0)
        d = pieces.declaration_for("alpha", "svc", "slice")
        self.assertEqual(d["event"], "done")

    def test_done_takes_no_piece_argument(self):
        """Asserted against the signature, because the protection is the *absence* of the
        argument — a later refactor that adds one back would pass every other test here."""
        import inspect
        params = inspect.signature(cwt.cmd_worktree_done).parameters
        self.assertEqual(list(params), ["args"])
        rc, _ = self.declare("done")
        self.assertEqual(rc, 0)

    def test_the_declaration_shows_in_the_listing(self):
        self.declare("done")
        self.assertIn("done", self.listing())


class TestDeclaringAbandoned(DeclareCase):
    def test_abandon_records_the_reason(self):
        rc, _ = self.declare("abandon", "the fixture needs a live forge token")
        self.assertEqual(rc, 0)
        d = pieces.declaration_for("alpha", "svc", "slice")
        self.assertEqual(d["event"], "abandoned")
        self.assertIn("forge token", d["reason"])

    def test_abandon_refuses_an_empty_reason(self):
        """The reason is the most useful thing an abandoning worker produces — it is what
        the next worker reads instead of re-deriving why this stopped."""
        rc, out = self.declare("abandon", "   ")
        self.assertNotEqual(rc, 0)
        self.assertIsNone(pieces.declaration_for("alpha", "svc", "slice"))
        self.assertIn("reason", out.lower())

    def test_the_reason_shows_in_the_listing(self):
        self.declare("abandon", "blocked on a denial")
        self.assertIn("blocked on a denial", self.listing())


class TestDeclaringOutsideAWorktree(DeclareCase):
    def test_done_outside_a_worktree_refuses(self):
        rc, out = self.declare("done", cwd=self.clone)
        self.assertNotEqual(rc, 0)
        self.assertIn("worktree", out.lower())

    def test_abandon_outside_a_worktree_refuses(self):
        rc, out = self.declare("abandon", "whatever", cwd=self.clone)
        self.assertNotEqual(rc, 0)
        self.assertIn("worktree", out.lower())

    def test_nothing_is_recorded_when_it_refuses(self):
        self.declare("done", cwd=self.clone)
        self.assertEqual([e["event"] for e in pieces.events("alpha")], ["claimed"])


class TestDeclaringTwice(DeclareCase):
    def test_the_latest_declaration_wins(self):
        """A worker that declares done and then finds it was not done must be able to say
        so. The log is append-only, so the first declaration survives as history."""
        self.declare("done")
        self.declare("abandon", "the tests only passed locally")
        d = pieces.declaration_for("alpha", "svc", "slice")
        self.assertEqual(d["event"], "abandoned")
        self.assertEqual([e["event"] for e in pieces.events("alpha")],
                         ["claimed", "done", "abandoned"])

    def test_redeclaring_says_so(self):
        """Predictable, not silent: overwriting an outcome is worth one line, because the
        common cause is a worker that did not know the piece was already declared."""
        self.declare("done")
        _, out = self.declare("done")
        self.assertIn("already", out.lower())


class TestTheVocabularyIsClosed(DeclareCase):
    def test_no_state_other_than_done_or_abandoned_can_be_recorded(self):
        """`failed`, `blocked` and `timed-out` are absent because charter can verify none
        of them. This raises rather than returning None: an unknown event is a programming
        mistake, not the runtime condition that `record`'s best-effort contract covers."""
        for bogus in ("failed", "blocked", "timed-out", "stuck"):
            with self.assertRaises(ValueError):
                pieces.record("alpha", bogus, "svc", "slice")

    def test_the_record_keys_stay_a_closed_set(self):
        self.declare("abandon", "why")
        allowed = set(pieces.FIELDS)
        for e in pieces.events("alpha"):
            self.assertEqual(set(e) - allowed, set())


if __name__ == "__main__":
    unittest.main()
