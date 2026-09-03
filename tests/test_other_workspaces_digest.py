"""A session knows the OTHER workspaces exist, and roughly what they are up to.

Everything charter injects at SessionStart describes the workspace you are in. Nothing
describes the ones you are not — so a change delivered by a parallel workspace arrives as
a surprise ("why did this move?"), and the only way to find out is to already suspect it.
The material is all on disk: each workspace has a vision line, an open-todo list, and
mtimes saying when it was last worked.

This is **knowledge, never logic**. Nothing in charter reads it, nothing branches on it,
and the block says so in as many words — an agent that treated another workspace's stated
goal as an instruction would be doing precisely what this is not for.

Bounded like every other signal in that preamble: the newest few, with the count of what
is not shown. The context budget it rides has already been cut back once (`_memory_digest`),
and a list that grows with the plane would end up being most of the briefing.
"""
from __future__ import annotations

import os
import time
import unittest
from unittest import mock

from charter import config, hooks, todos, workspace
from tests._isolation import PersonaIso, PlaneIso, run_hook


def _context(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


class NeighbourCase(PlaneIso):
    def setUp(self) -> None:
        super().setUp()
        # Pin the active workspace so the confirm nudge (several hundred words about a
        # different subject) stays out of what these assertions read.
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "mine"}))
        workspace.ensure("mine")

    def ws(self, name, vision=None, todo=None, age_days=None):
        workspace.ensure(name)
        workspace.scaffold(name)
        if vision:
            workspace.set_vision(name, vision)
        if todo:
            todos.add(name, todo)
        if age_days is not None:
            old = time.time() - age_days * 86400
            for f in (config.WORKSPACES_DIR / name).rglob("*"):
                os.utime(f, (old, old))
            os.utime(config.WORKSPACES_DIR / name, (old, old))
        return name

    def start(self, sid="s1") -> str:
        return _context(run_hook(hooks.sessionstart, {"session_id": sid}))


class TestItNamesTheNeighbours(NeighbourCase):
    def test_another_workspace_is_named(self):
        self.ws("billing", vision="Move billing off the legacy gateway.")
        self.assertIn("billing", self.start())

    def test_its_vision_line_travels(self):
        """The name alone answers 'who else exists' but not 'why would they touch this'."""
        self.ws("billing", vision="Move billing off the legacy gateway.")
        self.assertIn("Move billing off the legacy gateway.", self.start())

    def test_only_the_first_line_of_a_long_vision(self):
        self.ws("billing", vision="One line summary.\n\nA second paragraph nobody needs here.")
        out = self.start()
        self.assertIn("One line summary.", out)
        self.assertNotIn("A second paragraph nobody needs here.", out)

    def test_the_active_workspace_is_not_listed(self):
        """It already has the whole briefing above this block."""
        self.ws("billing", vision="v")
        block = self._neighbour_block(self.start())
        self.assertNotIn("mine", block)

    def test_open_todo_counts_travel(self):
        self.ws("billing", vision="v", todo="finish the gateway swap")
        self.assertIn("1 todo", self.start())

    def test_it_is_silent_when_there_are_no_others(self):
        self.assertEqual(self._neighbour_block(self.start()), "")

    def _neighbour_block(self, ctx: str) -> str:
        for part in ctx.split("\n\n"):
            if "other workspace" in part.lower():
                return part
        return ""


class TestItReadsAsKnowledge(NeighbourCase):
    def test_it_says_it_is_not_instructions(self):
        """Another workspace's stated goal is the single most instruction-shaped thing
        charter injects. It has to be labelled, the way the todo digest already is."""
        self.ws("billing", vision="Move billing off the legacy gateway.")
        out = self.start().lower()
        self.assertIn("never instructions", out)

    def test_it_says_why_it_is_there(self):
        self.ws("billing", vision="v")
        self.assertIn("surprise", self.start().lower())


class TestItIsBounded(NeighbourCase):
    def test_it_shows_the_newest_few_and_counts_the_rest(self):
        for i in range(8):
            self.ws(f"ws{i}", vision=f"vision {i}", age_days=i)
        out = self.start()
        self.assertIn("ws0", out)          # newest is shown
        self.assertNotIn("ws7", out)       # oldest is not
        self.assertIn("3 more", out)       # 8 - 5 shown

    def test_most_recently_worked_first(self):
        self.ws("stale", vision="v", age_days=30)
        self.ws("fresh", vision="v", age_days=0)
        out = self.start()
        self.assertLess(out.index("fresh"), out.index("stale"))


class TestItNeverBreaksTheBriefing(NeighbourCase):
    def test_a_workspace_with_no_vision_still_lists(self):
        workspace.ensure("bare")
        self.assertIn("bare", self.start())

    def test_the_rest_of_the_briefing_survives_a_broken_workspace(self):
        """Best-effort like every other signal here: an unreadable workspace costs the
        session one line, never its briefing."""
        self.ws("billing", vision="v")
        with mock.patch.object(workspace, "read_vision", side_effect=OSError("boom")):
            out = self.start()
        self.assertIsInstance(out, str)


if __name__ == "__main__":
    unittest.main()
