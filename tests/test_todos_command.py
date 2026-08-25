"""`charter workspace todo` — recording, reading and closing a workspace's intent.

Follows the shape `workspace remember` already established: the verb with text records,
the verb bare lists. A literal `todo list` subcommand would be indistinguishable from
recording a todo whose text is "list"; `done`/`forget` escape that trap only because they
take a slug after them, which recording never does.
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
    kw.setdefault("slug", None)
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


class ClosingCase(TodoCommandCase):
    """A todo is closed by slug — the handle the list already prints beside each one."""

    def record(self, text: str) -> str:
        """Record one todo and return the slug that addresses it.

        By difference, not by position: todos recorded in the same second sort
        alphabetically among themselves, so "the last one listed" is not "the one just
        added" — which made a test close the wrong todo and still pass its own assertion.
        """
        before = {t["slug"] for t in todos.open_todos("alpha")}
        self.run_todo(text=text)
        new = [t["slug"] for t in todos.open_todos("alpha") if t["slug"] not in before]
        self.assertEqual(len(new), 1, "the todo under test was not recorded")
        return new[0]

    def journal(self, name: str = "alpha") -> str:
        return "\n".join(p.read_text() for p in workspace.memories(name))


class TestFinishingATodo(ClosingCase):
    """`done` — finished, so the list drops it and the journal keeps the trace.

    The trace is the load-bearing half. An agent may close its own todos, so without a
    record in the journal it could create and tick off work unobserved and the list would
    always read as finished — which is worse than having no list.
    """

    def test_a_finished_todo_leaves_the_list(self):
        slug = self.record("prove the live gh path")
        rc, _ = self.run_todo_all(text="done", slug=slug)
        self.assertEqual(rc, 0)
        self.assertEqual(todos.open_todos("alpha"), [])

    def test_finishing_writes_a_journal_entry_naming_the_todo(self):
        self.assertEqual(self.journal(), "")  # nothing in the journal to mistake for the trace
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="done", slug=slug)
        self.assertIn("prove the live gh path", self.journal())

    def test_finishing_reports_what_it_closed(self):
        """The slug is a truncated stamp+title; echoing the todo back is how the developer
        confirms the right one went, not the one recorded in the same second."""
        slug = self.record("prove the live gh path")
        _, out = self.run_todo_all(text="done", slug=slug)
        self.assertIn("prove the live gh path", out)

    def test_a_finished_todo_leaves_no_file_behind(self):
        """Closing DELETES. The journal is already the permanent record of what happened,
        so a kept-but-marked todo would build a second one that every later read has to
        filter past."""
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="done", slug=slug)
        left = [p.name for p in todos.todos_dir("alpha").glob("*.md") if p.name != "MEMORY.md"]
        self.assertEqual(left, [])

    def test_a_finished_todo_leaves_the_index(self):
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="done", slug=slug)
        self.assertNotIn(slug, todos.index("alpha").read_text())

    def test_the_closing_verb_is_not_itself_recorded_as_a_todo(self):
        """`todo done <slug>` must never fall through to the recording path — a todo
        called "done" is the one entry nobody could act on."""
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="done", slug=slug)
        self.assertNotIn("done", [t["title"] for t in todos.open_todos("alpha")])

    def test_finishing_leaves_the_other_todos_alone(self):
        keep = self.record("keep this one")
        slug = self.record("close this one")
        self.run_todo_all(text="done", slug=slug)
        self.assertEqual([t["slug"] for t in todos.open_todos("alpha")], [keep])


class TestAbandoningATodo(ClosingCase):
    """`forget` — given up on, so it goes silently: nothing happened worth journalling."""

    def test_an_abandoned_todo_leaves_the_list(self):
        slug = self.record("prove the live gh path")
        rc, _ = self.run_todo_all(text="forget", slug=slug)
        self.assertEqual(rc, 0)
        self.assertEqual(todos.open_todos("alpha"), [])

    def test_abandoning_writes_no_journal_entry(self):
        """The distinction from `done` is the entire point of having two verbs: a journal
        full of work that was never done is a journal nobody trusts."""
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="forget", slug=slug)
        self.assertNotIn("prove the live gh path", self.journal())

    def test_abandoning_leaves_no_file_behind(self):
        slug = self.record("prove the live gh path")
        self.run_todo_all(text="forget", slug=slug)
        left = [p.name for p in todos.todos_dir("alpha").glob("*.md") if p.name != "MEMORY.md"]
        self.assertEqual(left, [])


class TestClosingWhatIsNotThere(ClosingCase):
    def test_an_unknown_slug_fails(self):
        self.record("prove the live gh path")
        rc, _ = self.run_todo_all(text="done", slug="no-such-todo")
        self.assertNotEqual(rc, 0)

    def test_an_unknown_slug_says_how_to_list_the_real_ones(self):
        """Slugs are typed by hand from a listing, so a typo is the normal failure — and
        the answer to "what are the real ones?" is one command away."""
        self.record("prove the live gh path")
        _, out = self.run_todo_all(text="done", slug="no-such-todo")
        self.assertIn("charter ws todo", out)

    def test_an_unknown_slug_closes_nothing(self):
        self.record("prove the live gh path")
        self.run_todo_all(text="done", slug="no-such-todo")
        self.assertEqual(len(todos.open_todos("alpha")), 1)

    def test_an_unknown_slug_writes_no_journal_entry(self):
        """A trace of finishing something that was never there is a lie in the one record
        the list's trustworthiness rests on."""
        self.record("prove the live gh path")
        self.run_todo_all(text="done", slug="no-such-todo")
        self.assertEqual(self.journal(), "")

    def test_abandoning_an_unknown_slug_fails(self):
        self.record("prove the live gh path")
        rc, _ = self.run_todo_all(text="forget", slug="no-such-todo")
        self.assertNotEqual(rc, 0)

    def test_a_closing_verb_without_a_slug_asks_for_one(self):
        """`todo done` with nothing after it is a forgotten argument, not a request to
        record a todo whose text is "done" — recording it would be a silent wrong action."""
        rc, out = self.run_todo_all(text="done")
        self.assertNotEqual(rc, 0)
        self.assertEqual(todos.open_todos("alpha"), [])
        self.assertIn("slug", out)


class TestClosingIsWorkspaceScoped(ClosingCase):
    """A todo belongs to exactly one workspace, and so does the command that closes it —
    otherwise one task could quietly tick off another's work."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("beta")

    def _close(self, ws: str, verb: str, slug: str) -> int:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            return commands_workspace.cmd_workspace_todo(
                _args(workspace=ws, text=verb, slug=slug))

    def test_done_cannot_reach_another_workspaces_todo(self):
        slug = self.record("alpha work")
        self.assertNotEqual(self._close("beta", "done", slug), 0)
        self.assertEqual(len(todos.open_todos("alpha")), 1)

    def test_forget_cannot_reach_another_workspaces_todo(self):
        slug = self.record("alpha work")
        self.assertNotEqual(self._close("beta", "forget", slug), 0)
        self.assertEqual(len(todos.open_todos("alpha")), 1)


class TestTheReservedWordsAreEscapable(TodoCommandCase):
    """Issue #59. `done` and `forget` are reserved as closing verbs, so a todo whose text
    is exactly one of them cannot be recorded. Degenerate, but the escape existed all
    along — matching is exact and lowercase — and nothing told you, which turned a
    recoverable slip into a dead end."""

    def test_the_exact_lowercase_word_is_still_refused(self):
        rc, _ = self.run_todo(text="done")
        self.assertNotEqual(rc, 0)

    def test_the_refusal_says_how_to_record_it_anyway(self):
        _, out = self.run_todo_all(text="done")
        self.assertIn("capitalise", out.lower())

    def test_any_other_capitalisation_records_normally(self):
        rc, _ = self.run_todo(text="Done")
        self.assertEqual(rc, 0)
        self.assertEqual([t["title"] for t in todos.open_todos("alpha")], ["Done"])

    def test_the_word_inside_a_longer_todo_is_untouched(self):
        rc, _ = self.run_todo(text="done with the migration, write it up")
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
