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


class TestADuplicateJudgedByTitle(PersonaIso):
    """`by_title`, for a writer whose todos all end in the same sentence.

    `charter handoff` is that writer. Scored over the whole text, its fixed nine-word
    provenance tail read as agreement on both sides: two briefs with no word in common
    scored 0.750 and the second was dropped, so the second handoff into a workspace recorded
    nothing at all.
    """

    TAIL = ("\n\nHanded off from chat alpha.1 · workspace alpha. The full brief is private "
            "to the chat it opened.")

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        todos.add("alpha", "Fix the widget" + self.TAIL)

    def test_the_shared_tail_alone_used_to_make_two_todos_the_same_one(self):
        """The measurement the flag exists for, kept as a test so it cannot quietly come
        back: over the whole text these two are a duplicate pair."""
        self.assertIsNotNone(todos.duplicate_of("alpha", "Ship the release" + self.TAIL))

    def test_a_different_first_line_is_a_different_todo(self):
        self.assertIsNone(
            todos.duplicate_of("alpha", "Ship the release" + self.TAIL, by_title=True))

    def test_the_same_first_line_is_still_the_same_todo(self):
        """The other direction, or the flag is a way of never reporting a duplicate."""
        self.assertEqual(
            todos.duplicate_of("alpha", "Fix the widget" + self.TAIL, by_title=True),
            "Fix the widget")

    def test_a_score_of_exactly_the_threshold_is_still_the_same_todo(self):
        """The boundary, on the side that matters. `_DUPLICATE_THRESHOLD` is the lowest score
        that counts, so `>=` and not `>`: these two titles share three words out of a union of
        six — 0.500 exactly — and under `>` they fall through to identity, read as different
        lines, and the second handoff records a genuine duplicate. That is the failure this
        whole rule exists to prevent, arriving at the one score a comparison can land on and
        still be told it does not count."""
        todos.add("alpha", "Rotate the staging tokens" + self.TAIL)
        self.assertEqual(
            len(memstore.wordset("Rotate the staging tokens")
                & memstore.wordset("Rotate the staging tokens before Friday morning")), 3)
        self.assertEqual(
            len(memstore.wordset("Rotate the staging tokens")
                | memstore.wordset("Rotate the staging tokens before Friday morning")), 6)
        self.assertEqual(
            todos.duplicate_of("alpha",
                               "Rotate the staging tokens before Friday morning" + self.TAIL,
                               by_title=True),
            "Rotate the staging tokens")

    def test_the_stored_side_is_its_title_and_not_its_body_either(self):
        """**Both sides**, and this is the case that says so. Every other case here either
        stores a bare title — where the body IS the title, so reading one for the other
        changes nothing — or compares titles that are identical, which the identity fallback
        answers before the overlap is read. So neither reddens if the stored side quietly
        goes back to `title + body`.

        This pair does: two real handoff todos, each carrying the provenance tail, whose
        titles overlap on three words at 0.750. Read as titles they are one todo; read with
        the tail on the stored side the union grows by nine boilerplate words, the score
        falls under the threshold, and a genuine duplicate is recorded twice."""
        todos.add("alpha", "Rotate the staging tokens" + self.TAIL)
        self.assertEqual(
            todos.duplicate_of("alpha", "Rotate the staging tokens again" + self.TAIL,
                               by_title=True),
            "Rotate the staging tokens")


class TestAnOverlapTooThinToBeEvidence(PersonaIso):
    """`todos._MIN_SHARED_WORDS`, on the `by_title` path — where below it the ratio is
    arithmetic with nothing behind it, and wrong in both directions at once.

    `memstore.wordset` keeps only words longer than three characters, so `fix the bug` has
    no comparable word at all — two of them agreed on nothing and scored nothing, so the
    same handoff recorded twice went in twice. At the other end one shared word out of two
    is 0.5 exactly, so `Fix the widget` swallowed `Break the widget`, and for a handoff a
    false duplicate means a real todo silently dropped. Where the overlap cannot
    distinguish, identity decides, which needs no threshold.

    **The whole-text path deliberately does NOT get this rule** —
    `TestTheWholeTextPathStillCatchesARetitle` is the other half, and `duplicate_of`'s own
    docstring is why.
    """

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def _dup(self, text):
        return todos.duplicate_of("alpha", text, by_title=True)

    def test_two_titles_of_only_short_words_that_read_the_same_are_the_same_todo(self):
        todos.add("alpha", "fix the bug")
        self.assertEqual(self._dup("fix the bug"), "fix the bug")

    def test_and_the_same_words_typed_differently_are_still_the_same_todo(self):
        """Case and runs of whitespace are not differences (`todos._normal`)."""
        todos.add("alpha", "fix the bug")
        self.assertEqual(self._dup("Fix   THE bug"), "fix the bug")

    def test_two_titles_of_only_short_words_that_read_differently_are_not(self):
        todos.add("alpha", "fix the bug")
        self.assertIsNone(self._dup("ask the dev"))

    def test_one_shared_word_out_of_two_is_not_the_same_todo(self):
        todos.add("alpha", "Fix the widget")
        self.assertIsNone(self._dup("Break the widget"))

    def test_two_shared_words_out_of_four_are_not_either(self):
        todos.add("alpha", "Update the README file")
        self.assertIsNone(self._dup("Delete the README file"))

    def test_three_shared_words_are_evidence_and_the_metric_decides(self):
        """**Three, spelled out rather than read off the constant** — an expectation
        computed from the value it checks moves with that value and pins nothing, which is
        how `TITLE_MAX` survived two mutations before its own number was written down.
        These titles share exactly three words at 0.750, so at a floor of four they would
        fall to identity, read as different lines, and be recorded twice."""
        self.assertEqual(todos._MIN_SHARED_WORDS, 3)
        todos.add("alpha", "Rotate the staging tokens")
        self.assertEqual(len(memstore.wordset("Rotate the staging tokens")
                             & memstore.wordset("Rotate the staging tokens again")), 3)
        self.assertEqual(self._dup("Rotate the staging tokens again"),
                         "Rotate the staging tokens")

    def test_a_real_overlap_is_still_read_as_one(self):
        """The rule narrows what counts as evidence; it does not switch the metric off."""
        todos.add("alpha", "prove the live gh issue create path works")
        self.assertIsNotNone(self._dup("prove the live gh issue create path also works"))

    def test_the_same_rule_holds_for_a_handoffs_whole_todo_text(self):
        tail = TestADuplicateJudgedByTitle.TAIL
        todos.add("alpha", "fix the bug" + tail)
        self.assertEqual(self._dup("fix the bug" + tail), "fix the bug")
        self.assertIsNone(self._dup("ask the dev" + tail))


class TestTheWholeTextPathStillCatchesARetitle(PersonaIso):
    """The other half of the split, and the reason for it: `charter ws todo` REFUSES on a
    duplicate, so it wants the catching rule even at the cost of a false one.

    Round 2 applied the no-signal rule to this path too. Measured on 20 hand-written pairs
    it changed 12 verdicts, and five of them were one shape — a one-line todo retitled by a
    word — turning from caught into missed, with `commands_workspace.cmd_ws_todo` still
    carrying the comment that says why that is the worse error: *closing one of a
    near-identical pair leaves its twin looking outstanding*.
    """

    RETITLES = (
        ("Fix the login bug", "Fix the login issue"),
        ("Update the README", "Update the README file"),
        ("Review the backlog", "Review the backlog again"),
        ("Rotate the staging tokens", "Rotate the staging token"),
        ("Add a retry to the webhook sender", "Add retries to the webhook sender"),
    )

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_a_one_line_todo_retitled_by_a_word_is_still_caught(self):
        for first, second in self.RETITLES:
            with self.subTest(first=first):
                workspace.ensure("w")
                todos.add("w", first)
                self.assertEqual(todos.duplicate_of("w", second), first)
                for p in memstore.files(todos.todos_dir("w")):
                    p.unlink()

    def test_and_the_by_title_path_reads_those_same_pairs_as_two_todos(self):
        """Not a contradiction — the opposite error, chosen by the caller. A handoff that
        refused one of these would drop a real todo and say only that it did not record it
        twice."""
        for first, second in self.RETITLES:
            with self.subTest(first=first):
                workspace.ensure("w")
                todos.add("w", first)
                self.assertIsNone(todos.duplicate_of("w", second, by_title=True))
                for p in memstore.files(todos.todos_dir("w")):
                    p.unlink()

    def test_a_todo_of_only_short_words_is_still_not_compared_on_this_path(self):
        """`memstore.wordset` has nothing to compare, and this path answers None rather than
        falling back to identity — the behaviour `charter ws todo` had before this branch."""
        todos.add("alpha", "fix the bug")
        self.assertIsNone(todos.duplicate_of("alpha", "fix the bug"))


if __name__ == "__main__":
    unittest.main()
