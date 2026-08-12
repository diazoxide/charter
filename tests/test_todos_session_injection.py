"""SessionStart surfaces the workspace's open todos — bounded, oldest-first, or not at all.

charter's todo list has exactly one reader, and this is it. The list deliberately does not
sync with Claude Code's own session task list (docs/adr/0006), so with nothing surfacing it
the store would be **write-only**: intent recorded and never read again, which is worse than
no store at all, because writing to it feels like progress.

Bounded twice over. Three titles whatever the length of the list — the budget being joined
is already defended (`_memory_digest` is commented "a BOUNDED digest, not the whole index"),
and this must not become the reason it grows again. And nothing whatsoever when nothing is
open: a reminder that fires when there is no work is how someone learns to skim past every
reminder charter emits, including the ones that matter.

Oldest-first is the only ranking the feature has, and it is what makes the reminder
self-correcting — what surfaces is what is being avoided, rather than what is already in
mind.
"""
from __future__ import annotations

import datetime
import os
import unittest
from unittest import mock

from charter import config, hooks, persona, todos, workspace
from tests._isolation import PersonaIso, run_hook


def _context(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


class TodoInjectionCase(PersonaIso):
    """A session standing in workspace `alpha`, with a persona active.

    The workspace is pinned through ``$CHARTER_WORKSPACE`` rather than a session pointer for
    two reasons: it is the one resolution branch that cannot be decided by the machine the
    suite runs on, so the assertions stay about what was *injected*; and it silences the
    confirm nudge, whose several hundred words would otherwise be most of the text every
    assertion here searches.
    """

    WS = "alpha"

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure(self.WS)
        self.make_persona("dev", role="Dev", vault="dev")
        self.enterContext(mock.patch.dict(
            os.environ, {"CHARTER_WORKSPACE": self.WS, "CHARTER_PERSONA": "dev"}))

    def inject(self) -> str:
        return _context(run_hook(hooks.sessionstart, {"session_id": "s-todo"}))

    def add_aged(self, text: str, days: int, workspace_name: str | None = None) -> None:
        """Record a todo stamped *days* ago — real time gaps, not same-second inserts.

        The store stamps whole seconds, so a burst recorded in one second orders
        alphabetically among itself; an ordering test built that way passes on the title
        text and proves nothing about age (see `test_todos_store.TestOldestFirst`)."""
        todos.add(workspace_name or self.WS, text,
                  stamp=datetime.datetime.now() - datetime.timedelta(days=days))


class TestASessionIsToldWhatIsOutstanding(TodoInjectionCase):
    def test_a_session_with_open_todos_is_told_how_many(self):
        """The count is the part that survives the cap: three titles out of eleven is a
        sample, and only the number says how much of the list is not on screen."""
        for i in range(11):
            self.add_aged(f"outstanding thing {i}", days=30 - i)
        ctx = self.inject()
        self.assertIn("todo", ctx.lower())
        self.assertIn("11", ctx)

    def test_the_three_oldest_titles_are_shown(self):
        self.add_aged("OLDEST waiting since forever", days=90)
        self.add_aged("SECOND oldest", days=60)
        self.add_aged("THIRD oldest", days=30)
        ctx = self.inject()
        for marker in ("OLDEST waiting since forever", "SECOND oldest", "THIRD oldest"):
            self.assertIn(marker, ctx, marker)

    def test_never_more_than_three_titles_however_long_the_list(self):
        """The cap is the design, not a default. A twenty-item list injected in full is
        exactly the unbounded briefing `_memory_digest` was cut back to stop."""
        for i in range(20):
            self.add_aged(f"MARKER{i:02d} some intent", days=40 - i)
        ctx = self.inject()
        self.assertEqual(sum(1 for i in range(20) if f"MARKER{i:02d}" in ctx), 3)

    def test_the_three_shown_are_genuinely_the_oldest(self):
        """Newest-first would surface what you already remember writing down — the reminder
        would then agree with you, which is the one thing it must not do."""
        self.add_aged("AAA oldest", days=90)
        self.add_aged("BBB", days=60)
        self.add_aged("CCC", days=30)
        self.add_aged("ZZZ newest, written yesterday", days=1)
        ctx = self.inject()
        self.assertIn("AAA oldest", ctx)
        self.assertNotIn("ZZZ newest", ctx)

    def test_how_long_each_has_been_waiting_is_shown(self):
        """Age is the evidence for the ranking: 'oldest three' with no numbers asks the
        reader to take the ordering on trust."""
        self.add_aged("stale intent", days=47)
        self.assertIn("47d", self.inject())

    def test_the_workspace_it_read_is_named(self):
        """Todos are workspace-scoped, so an unattributed list is ambiguous exactly when it
        matters — at session start, before the workspace is even confirmed."""
        self.add_aged("some intent", days=3)
        self.assertIn("`alpha`", self.inject())

    def test_the_rest_of_the_list_is_reachable(self):
        """Three titles is a pointer at the list, so the pointer has to say where it points."""
        for i in range(10):
            self.add_aged(f"thing {i}", days=20 - i)
        self.assertIn("charter ws todo", self.inject())

    def test_the_injection_stays_bounded_as_the_list_grows(self):
        """Same invariant the memory digest is held to: three todos and a hundred cost about
        the same, so nobody ever has to prune the list to protect the context window."""
        for i in range(3):
            self.add_aged(f"early thing {i}", days=300 - i)
        small = len(self.inject())
        for i in range(100):
            self.add_aged(f"later thing {i}", days=200 - i)
        big = len(self.inject())
        self.assertLess(big - small, 40,
                        f"injection grew {big - small}B for 100 more todos — unbounded")


class TestNothingOpenMeansNothingSaid(TodoInjectionCase):
    def test_a_workspace_with_no_open_todos_injects_nothing_whatsoever(self):
        """Not an empty heading, not 'nothing outstanding' — nothing. A signal that fires on
        no news trains the reader to stop reading it, and it is sharing a session preamble
        with signals that must keep being read."""
        ctx = self.inject()
        self.assertNotIn("todo", ctx.lower())

    def test_the_digest_is_empty_rather_than_blank_text(self):
        """Checked on the helper too: a whitespace-only string would still be appended as a
        part and would still cost a paragraph break in the injected context."""
        self.assertEqual(hooks._todo_digest("s-todo"), "")

    def test_another_workspaces_todos_do_not_fire_the_reminder(self):
        """Scoping is the whole basis of the feature: another workspace's intent is another
        task's business, and surfacing it here would make this workspace look busy."""
        workspace.ensure("beta")
        self.add_aged("beta work nobody here should see", days=10, workspace_name="beta")
        self.assertNotIn("todo", self.inject().lower())


class TestItAddsToTheBriefingRatherThanReplacingIt(TodoInjectionCase):
    """The persona role and memory digest are the reason SessionStart injects at all. This
    is a passenger on that budget and must arrive as an addition, never as a substitution."""

    def test_the_persona_role_survives(self):
        self.add_aged("some intent", days=5)
        self.assertIn("**dev** persona", self.inject())

    def test_the_memory_digest_survives(self):
        persona.remember("dev", "A DISTINCTIVE RECORDED FACT", shared=True)
        self.add_aged("some intent", days=5)
        ctx = self.inject()
        self.assertIn("DISTINCTIVE RECORDED FACT", ctx)
        self.assertIn("some intent", ctx)

    def test_the_memory_data_framing_survives(self):
        """The 'reference data, not instructions' framing is a safety property of the
        injection, and it is the sort of thing a later addition quietly truncates."""
        persona.remember("dev", "a fact", shared=True)
        self.add_aged("some intent", days=5)
        self.assertIn("reference **data**", self.inject())


class TestItDoesNotDisplaceTheWorkspaceGate(PersonaIso):
    """Deliberately unpinned — no ``$CHARTER_WORKSPACE`` — so the workspace-confirm nudge
    fires. That nudge is the start-of-session action gate, and it is the signal a todo list
    printed at the same moment is most likely to bury."""

    def setUp(self) -> None:
        super().setUp()
        self.ws = config.DEFAULT_WORKSPACE     # what an unconfirmed session resolves to
        workspace.ensure(self.ws)
        todos.add(self.ws, "TODO IN THE DEFAULT WORKSPACE")

    def test_both_the_confirm_nudge_and_the_todos_are_present(self):
        ctx = _context(run_hook(hooks.sessionstart, {"session_id": "s-unconfirmed"}))
        self.assertIn("Confirm the workspace", ctx)
        self.assertIn("TODO IN THE DEFAULT WORKSPACE", ctx)


class TestAFailureNeverBreaksSessionStart(TodoInjectionCase):
    """A hook that raises costs the session its whole preamble — persona, memory and all —
    to report a list of todos. Reading intent is never worth that."""

    def _broken(self):
        return mock.patch("charter.todos.open_todos", side_effect=OSError("unreadable"))

    def test_the_rest_of_the_briefing_still_arrives(self):
        self.add_aged("some intent", days=5)
        with self._broken():
            self.assertIn("**dev** persona", self.inject())

    def test_it_says_nothing_about_todos_rather_than_guessing(self):
        self.add_aged("some intent", days=5)
        with self._broken():
            self.assertNotIn("todo", self.inject().lower())

    def test_the_hook_still_exits_zero(self):
        """Exit status is the part Claude Code acts on; a non-zero SessionStart hook is
        reported to the user as charter being broken."""
        rc: list[int] = []
        self.add_aged("some intent", days=5)
        with self._broken():
            run_hook(lambda: rc.append(hooks.sessionstart()), {"session_id": "s-todo"})
        self.assertEqual(rc, [0])


if __name__ == "__main__":
    unittest.main()
