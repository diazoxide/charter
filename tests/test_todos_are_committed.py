"""A LIVE workspace's todos are actually **committed**, not merely un-ignored.

Ticket 05 added `todos/` to the managed `.gitignore` block, which is only half the job.
Un-ignoring a path tells git it *may* be tracked; something still has to stage it. The
paths that `workspace save`, the Stop-hook autosave, `live --off` and `rename` act on come
from one list, and `todos/` was missing from it — so a LIVE workspace's todo list was
visible to git and committed by nothing.

That is the exact failure ADR 0004 predicted for this feature and the one the ticket called
"the quietest possible": every other part keeps working, and the list simply never travels.
"""
from __future__ import annotations

import unittest

from charter import commands_workspace as cw
from charter import todos, workspace
from tests._isolation import PersonaIso


class TestTheSharedPathSet(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")

    def test_todos_are_in_the_set_of_paths_that_get_committed(self):
        todos.add("alpha", "something worth sharing")
        self.assertIn("workspaces/alpha/todos", cw._ws_meta_paths("alpha"))

    def test_the_existing_shared_paths_are_still_there(self):
        """The manifest, charter and memory must keep travelling — this adds, never
        replaces."""
        todos.add("alpha", "something")
        paths = cw._ws_meta_paths("alpha")
        self.assertIn("workspaces/alpha/workspace.md", paths)
        self.assertIn("workspaces/alpha/memory", paths)

    def test_a_workspace_with_no_todos_does_not_list_the_directory(self):
        """The list is filtered by existence, so an absent `todos/` must not appear —
        `git rm --cached` on a path that was never tracked fails the whole call, taking
        the manifest and memory down with it."""
        self.assertNotIn("workspaces/alpha/todos", cw._ws_meta_paths("alpha"))

    def test_recording_the_first_todo_is_what_adds_it(self):
        before = cw._ws_meta_paths("alpha")
        todos.add("alpha", "the first one")
        after = cw._ws_meta_paths("alpha")
        self.assertNotIn("workspaces/alpha/todos", before)
        self.assertIn("workspaces/alpha/todos", after)

    def test_every_listed_path_actually_exists(self):
        """The whole list is fed to git as literal paths; one that is not there fails the
        command for all of them."""
        todos.add("alpha", "something")
        from charter import config
        for rel in cw._ws_meta_paths("alpha"):
            self.assertTrue((config.ROOT / rel).exists(), rel)


class TestUnIgnoringAndCommittingAgree(PersonaIso):
    """The two halves have to name the same paths. They are edited in different files —
    the gitignore block in `workspace.py`, the staged set in `commands_workspace.py` — so
    nothing but a test stops one from moving without the other. That is precisely how
    `todos/` came to be un-ignored and never committed."""

    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("alpha")
        workspace.scaffold("alpha")
        todos.add("alpha", "something")
        workspace.set_live("alpha", True)

    def test_everything_committed_is_also_un_ignored(self):
        from charter import config
        gitignore = (config.ROOT / ".gitignore").read_text()
        for rel in cw._ws_meta_paths("alpha"):
            self.assertIn(f"!/{rel}", gitignore,
                          f"{rel} is staged for commit but still git-ignored")


if __name__ == "__main__":
    unittest.main()
