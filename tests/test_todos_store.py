"""The todo store — a workspace's **intent**, kept apart from its memory.

charter records what a task learned (memory) and what happened (the journal). A **todo** is
neither: it is a claim about the future, and the only one of the three that stops being true
by being acted on. It therefore gets its own store rather than a flag on memory — see
docs/adr/0004, and `workspaces/todos/workspace.md` for the glossary.
"""
from __future__ import annotations

import datetime
import unittest

from charter import memstore, todos, workspace
from tests._isolation import PersonaIso


class TestRecordingATodo(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_a_recorded_todo_is_listed(self):
        todos.add("alpha", "prove the live gh issue create path")
        self.assertEqual([t["title"] for t in todos.open_todos("alpha")],
                         ["prove the live gh issue create path"])

    def test_nothing_recorded_means_an_empty_list(self):
        self.assertEqual(todos.open_todos("alpha"), [])

    def test_a_todo_survives_a_separate_read(self):
        """Persistence is the entire point — the harness's own list already covers the
        within-session case."""
        todos.add("alpha", "come back to the labels")
        self.assertEqual(len(todos.open_todos("alpha")), 1)
        self.assertEqual(len(todos.open_todos("alpha")), 1)

    def test_each_todo_carries_an_age_in_days(self):
        todos.add("alpha", "something")
        self.assertEqual(todos.open_todos("alpha")[0]["age_days"], 0)

    def test_a_todo_is_addressable_by_slug(self):
        todos.add("alpha", "prove the live path")
        self.assertTrue(todos.open_todos("alpha")[0]["slug"])


class TestIntentIsNotMemory(PersonaIso):
    """ADR 0004. A `charter recall` hit on a todo somebody finished last month would read
    with the same authority as a durable fact — worse than no hit at all."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_todos_live_beside_memory_not_inside_it(self):
        todos.add("alpha", "a todo")
        self.assertFalse(
            todos.todos_dir("alpha").is_relative_to(workspace.memory_dir("alpha")))
        self.assertEqual(todos.todos_dir("alpha").parent,
                         workspace.memory_dir("alpha").parent)

    def test_a_todo_never_appears_among_the_workspace_memories(self):
        todos.add("alpha", "a todo about migrations")
        workspace.remember("alpha", "a memory about migrations")
        titles = [t for _, t, _ in memstore.entries(workspace.memory_dir("alpha"))]
        self.assertNotIn("a todo about migrations", titles)

    def test_a_memory_never_appears_among_the_todos(self):
        workspace.remember("alpha", "a memory about migrations")
        todos.add("alpha", "a todo about migrations")
        self.assertEqual([t["title"] for t in todos.open_todos("alpha")],
                         ["a todo about migrations"])


class TestWorkspaceIsolation(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.ensure("beta")

    def test_a_todo_belongs_to_exactly_one_workspace(self):
        todos.add("alpha", "alpha work")
        todos.add("beta", "beta work")
        self.assertEqual([t["title"] for t in todos.open_todos("alpha")], ["alpha work"])
        self.assertEqual([t["title"] for t in todos.open_todos("beta")], ["beta work"])

    def test_one_workspaces_list_is_empty_while_anothers_is_not(self):
        todos.add("alpha", "alpha work")
        self.assertEqual(todos.open_todos("beta"), [])


class TestOldestFirst(PersonaIso):
    """The only ranking in the feature. Oldest-first makes the session reminder
    self-correcting: what surfaces is what is being avoided."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_todos_come_back_oldest_first(self):
        """Titles chosen to sort alphabetically *against* insertion order, and stamped
        days apart. Recorded in the same second with ascending names, this test would pass
        on alphabetical ordering alone and prove nothing about time."""
        base = datetime.datetime(2026, 1, 1, 12, 0, 0)
        todos.add("alpha", "zzz recorded first", stamp=base)
        todos.add("alpha", "mmm recorded second", stamp=base + datetime.timedelta(days=1))
        todos.add("alpha", "aaa recorded last", stamp=base + datetime.timedelta(days=2))
        self.assertEqual([t["title"] for t in todos.open_todos("alpha")],
                         ["zzz recorded first", "mmm recorded second", "aaa recorded last"])

    def test_age_reflects_when_it_was_recorded(self):
        todos.add("alpha", "an old one",
                  stamp=datetime.datetime.now() - datetime.timedelta(days=30))
        self.assertEqual(todos.open_todos("alpha")[0]["age_days"], 30)

    def test_todos_in_the_same_second_are_all_kept(self):
        """Second granularity must never cost a todo — ties sort by name, but nothing
        is overwritten."""
        base = datetime.datetime(2026, 1, 1, 12, 0, 0)
        for t in ("one", "two", "three"):
            todos.add("alpha", t, stamp=base)
        self.assertEqual(len(todos.open_todos("alpha")), 3)


class TestNearDuplicates(PersonaIso):
    """Duplicate intent is worse than duplicate memory: closing one of a near-identical
    pair leaves its twin looking outstanding, so the list starts lying about what is left."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_an_almost_identical_todo_is_reported(self):
        todos.add("alpha", "prove the live gh issue create path works")
        self.assertIsNotNone(
            todos.duplicate_of("alpha", "prove the live gh issue create path works"))

    def test_an_unrelated_todo_is_not_reported(self):
        todos.add("alpha", "prove the live gh issue create path works")
        self.assertIsNone(todos.duplicate_of("alpha", "rewrite the status line frame"))

    def test_the_first_todo_in_an_empty_list_is_never_a_duplicate(self):
        self.assertIsNone(todos.duplicate_of("alpha", "anything at all"))

    def test_a_duplicate_in_another_workspace_does_not_count(self):
        """Scoping is the point — an identical todo elsewhere is a different task's."""
        workspace.ensure("beta")
        todos.add("beta", "prove the live path")
        self.assertIsNone(todos.duplicate_of("alpha", "prove the live path"))


if __name__ == "__main__":
    unittest.main()
