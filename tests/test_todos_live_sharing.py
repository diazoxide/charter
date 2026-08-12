"""Todos travel with a LIVE workspace.

A shared todo list is one of the better arguments for making a workspace LIVE at all: a
team picking up a task gets what is left to do, not only what was learned. The mechanism
already exists — marking a workspace LIVE rewrites a managed `.gitignore` block that
un-ignores its shareable paths — and the todo directory has to join the four already there.

This is the quietest possible failure if missed. Everything else about todos keeps working
and the list simply never travels, noticed only by the person who most needed it.
"""
from __future__ import annotations

import unittest

from charter import config, todos, workspace
from tests._isolation import PersonaIso


def _gitignore() -> str:
    p = config.ROOT / ".gitignore"
    return p.read_text() if p.exists() else ""


class TestLiveWorkspacesShareTheirTodos(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def test_a_live_workspace_un_ignores_its_todos(self):
        workspace.set_live("alpha", True)
        self.assertIn("!/workspaces/alpha/todos", _gitignore())

    def test_the_contents_are_un_ignored_too_not_just_the_directory(self):
        """`!/workspaces/alpha/todos` alone un-ignores the directory entry and none of
        the files in it — the same reason `memory` is paired with `memory/**`."""
        workspace.set_live("alpha", True)
        self.assertIn("!/workspaces/alpha/todos/**", _gitignore())

    def test_a_local_workspace_keeps_its_todos_private(self):
        todos.add("alpha", "something private")
        self.assertNotIn("!/workspaces/alpha/todos", _gitignore())

    def test_going_local_again_re_ignores_them(self):
        workspace.set_live("alpha", True)
        workspace.set_live("alpha", False)
        self.assertNotIn("!/workspaces/alpha/todos", _gitignore())

    def test_several_live_workspaces_each_get_their_own_line(self):
        workspace.ensure("beta")
        workspace.set_live("alpha", True)
        workspace.set_live("beta", True)
        gi = _gitignore()
        self.assertIn("!/workspaces/alpha/todos/**", gi)
        self.assertIn("!/workspaces/beta/todos/**", gi)

    def test_making_one_live_does_not_share_anothers_todos(self):
        workspace.ensure("beta")
        workspace.set_live("alpha", True)
        self.assertNotIn("!/workspaces/beta/todos", _gitignore())

    def test_the_existing_shared_paths_are_untouched(self):
        """Adding todos must not disturb what LIVE already shared."""
        workspace.set_live("alpha", True)
        gi = _gitignore()
        for path in ("workspace.json", "workspace.md", "memory", "memory/**"):
            self.assertIn(f"!/workspaces/alpha/{path}", gi)


class TestUpgradingAPlaneThatPredatesTodos(PersonaIso):
    """A plane made LIVE before todos existed has a managed block with four paths and no
    todo line, and nothing re-runs `set_live` on its own. `reinit` is the documented
    upgrade path — "after a charter upgrade" — so it is what repairs the block."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")

    def _stale_block(self) -> None:
        """Recreate the pre-todos block: LIVE, then strip the todo lines back out."""
        workspace.set_live("alpha", True)
        gi = config.ROOT / ".gitignore"
        gi.write_text("\n".join(ln for ln in gi.read_text().splitlines()
                                if "/todos" not in ln) + "\n")

    def test_the_stale_block_really_is_missing_the_todo_path(self):
        """Guards the guard: if this ever stops being true the upgrade test below would
        pass without proving anything."""
        self._stale_block()
        self.assertNotIn("!/workspaces/alpha/todos", _gitignore())

    def test_reinit_restores_the_todo_path(self):
        self._stale_block()
        workspace.reinit("alpha")
        self.assertIn("!/workspaces/alpha/todos/**", _gitignore())

    def test_reinit_leaves_a_local_workspace_local(self):
        """Repairing the block must never quietly promote a private workspace."""
        workspace.reinit("alpha")
        self.assertNotIn("!/workspaces/alpha/todos", _gitignore())


if __name__ == "__main__":
    unittest.main()
