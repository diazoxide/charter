"""`charter workspace todo` — recording and reading a workspace's intent.

Follows the shape `workspace remember` already established: the verb with text records,
the verb bare lists. A literal `todo list` subcommand would be indistinguishable from
recording a todo whose text is "list".
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands_workspace, todos, workspace
from tests._isolation import PersonaIso


def _args(**kw):
    kw.setdefault("text", None)
    kw.setdefault("workspace", None)
    kw.setdefault("query", None)
    return SimpleNamespace(**kw)


class TodoCommandCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def run_todo(self, **kw):
        """Returns (rc, stdout). Progress messages go to stderr via util.ok/info —
        `run_todo_all` is for the tests that care what the user was told."""
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = commands_workspace.cmd_workspace_todo(_args(workspace="alpha", **kw))
        return rc, buf.getvalue()

    def run_todo_all(self, **kw):
        """Returns (rc, stdout + stderr) — for assertions about what was reported."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_workspace.cmd_workspace_todo(_args(workspace="alpha", **kw))
        return rc, out.getvalue() + err.getvalue()


class TestRecording(TodoCommandCase):
    def test_recording_a_todo_succeeds_and_stores_it(self):
        rc, _ = self.run_todo(text="prove the live gh path")
        self.assertEqual(rc, 0)
        self.assertEqual(len(todos.open_todos("alpha")), 1)

    def test_recording_reports_where_it_went(self):
        """Naming the workspace matters — todos are scoped, so 'recorded' without saying
        where is exactly the ambiguity the scoping exists to remove."""
        _, out = self.run_todo_all(text="prove the live gh path")
        self.assertIn("alpha", out)

    def test_whitespace_only_text_lists_rather_than_recording(self):
        """Consistent with the bare verb, and with `workspace remember`: no text means
        no todo to record, so the useful answer is to show what is already there."""
        rc, _ = self.run_todo(text="   ")
        self.assertEqual(rc, 0)
        self.assertEqual(todos.open_todos("alpha"), [])


class TestListing(TodoCommandCase):
    def test_the_bare_verb_lists(self):
        self.run_todo(text="first thing")
        self.run_todo(text="second thing")
        rc, out = self.run_todo()
        self.assertEqual(rc, 0)
        self.assertIn("first thing", out)
        self.assertIn("second thing", out)

    def test_an_empty_list_says_so_rather_than_printing_nothing(self):
        """Silence is indistinguishable from a broken command, so an empty list has to
        say it is empty — and say how to add the first one."""
        rc, out = self.run_todo_all()
        self.assertEqual(rc, 0)
        self.assertIn("No open todos", out)
        self.assertIn("charter ws todo", out)

    def test_listing_shows_age(self):
        self.run_todo(text="something")
        _, out = self.run_todo()
        self.assertIn("0d", out)

    def test_listing_is_oldest_first(self):
        for t in ("aaa first", "bbb second", "ccc third"):
            self.run_todo(text=t)
        _, out = self.run_todo()
        self.assertLess(out.index("aaa first"), out.index("bbb second"))
        self.assertLess(out.index("bbb second"), out.index("ccc third"))

    def test_listing_shows_a_slug_to_address_each_todo(self):
        self.run_todo(text="prove the live path")
        _, out = self.run_todo()
        self.assertIn(todos.open_todos("alpha")[0]["slug"], out)


class TestDuplicates(TodoCommandCase):
    def test_a_near_duplicate_is_refused(self):
        self.run_todo(text="prove the live gh issue create path works")
        rc, _ = self.run_todo(text="prove the live gh issue create path works")
        self.assertNotEqual(rc, 0)

    def test_a_near_duplicate_does_not_grow_the_list(self):
        self.run_todo(text="prove the live gh issue create path works")
        self.run_todo(text="prove the live gh issue create path works")
        self.assertEqual(len(todos.open_todos("alpha")), 1)

    def test_the_original_survives_untouched(self):
        self.run_todo(text="prove the live gh issue create path works")
        before = todos.open_todos("alpha")[0]
        self.run_todo(text="prove the live gh issue create path works")
        self.assertEqual(todos.open_todos("alpha")[0]["slug"], before["slug"])

    def test_an_unrelated_todo_is_still_accepted(self):
        self.run_todo(text="prove the live gh issue create path works")
        rc, _ = self.run_todo(text="rewrite the status line frame")
        self.assertEqual(rc, 0)
        self.assertEqual(len(todos.open_todos("alpha")), 2)


class TestSearching(TodoCommandCase):
    def test_query_finds_a_matching_todo(self):
        self.run_todo(text="prove the live gh issue path")
        self.run_todo(text="rewrite the status line frame")
        _, out = self.run_todo(query="statusline status frame")
        self.assertIn("status line frame", out)

    def test_query_excludes_non_matches(self):
        self.run_todo(text="prove the live gh issue path")
        self.run_todo(text="rewrite the status line frame")
        _, out = self.run_todo(query="statusline status frame")
        self.assertNotIn("live gh issue path", out)


class TestScoping(TodoCommandCase):
    def test_the_command_targets_the_named_workspace(self):
        workspace.ensure("beta")
        self.run_todo(text="alpha work")
        buf = io.StringIO()
        with redirect_stdout(buf):
            commands_workspace.cmd_workspace_todo(_args(workspace="beta"))
        self.assertNotIn("alpha work", buf.getvalue())


if __name__ == "__main__":
    unittest.main()
