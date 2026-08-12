"""Workspace charter (workspace.md): the living Vision / Context / Glossary doc — seeded
at creation, editable at runtime, committed for LIVE, and inherited by a fork so anyone
can continue with full context."""

from __future__ import annotations

import datetime
import unittest
from types import SimpleNamespace

from charter import config, todos, workspace
from charter import commands_workspace as cw
from tests._isolation import PersonaIso


class CharterCase(PersonaIso):
    def _mk(self, name, *, vision=None, manifest=None, memo=None):
        workspace.ensure(name)
        workspace.scaffold(name)
        if vision:
            workspace.set_vision(name, vision)
        if manifest is not None:
            workspace.write_manifest(name, manifest)
        if memo:
            workspace.note(name, memo)

    # --- scaffolding + vision ---
    def test_scaffold_creates_charter_with_placeholder(self):
        self._mk("w")
        cf = workspace.charter_file("w")
        self.assertTrue(cf.exists())
        text = cf.read_text()
        self.assertIn("## Vision", text)
        self.assertIn("## Glossary", text)
        self.assertEqual(workspace.read_vision("w"), "")  # placeholder → empty

    def test_set_and_read_vision(self):
        self._mk("w")
        workspace.set_vision("w", "Ship the SSO migration")
        self.assertEqual(workspace.read_vision("w"), "Ship the SSO migration")
        # replacing keeps other sections intact
        workspace.set_vision("w", "Refined goal")
        c = workspace.read_charter("w")
        self.assertEqual(workspace.read_vision("w"), "Refined goal")
        self.assertNotIn("Ship the SSO migration", c)
        self.assertIn("## Glossary", c)

    def test_replace_md_section_appends_when_absent(self):
        out = workspace._replace_md_section("# t\n\n## Vision\n\nx\n", "Glossary", "a — b")
        self.assertIn("## Glossary", out)
        self.assertIn("a — b", out)
        self.assertIn("## Vision", out)  # untouched

    # --- create --vision ---
    def _create_args(self, name, vision=None, live=False):
        return SimpleNamespace(name=name, use=False, force=False, live=live,
                               vision=vision, repos=[])

    def test_create_with_vision(self):
        cw.cmd_workspace_create(self._create_args("w", vision="Do the thing"))
        self.assertEqual(workspace.read_vision("w"), "Do the thing")

    def test_create_without_vision_still_has_charter(self):
        cw.cmd_workspace_create(self._create_args("w"))
        self.assertTrue(workspace.charter_file("w").exists())
        self.assertEqual(workspace.read_vision("w"), "")

    # --- vision command ---
    def test_vision_command_set_then_show(self):
        self._mk("w")
        self.assertEqual(cw.cmd_workspace_vision(
            SimpleNamespace(workspace="w", text="the goal")), 0)
        self.assertEqual(workspace.read_vision("w"), "the goal")
        self.assertEqual(cw.cmd_workspace_vision(
            SimpleNamespace(workspace="w", text=None)), 0)  # show

    def test_vision_command_missing_workspace(self):
        self.assertEqual(cw.cmd_workspace_vision(
            SimpleNamespace(workspace="ghost", text="x")), 1)

    # --- fork ---
    def _fork_args(self, src, new, live=False, restore=False):
        return SimpleNamespace(src=src, new=new, live=live, restore=restore)

    def _memtext(self, name):
        return "\n".join(p.read_text() for p in workspace.memories(name))

    def test_fork_inherits_vision_memo_and_manifest(self):
        self._mk("src", vision="Big goal",
                 manifest={"name": "src", "repos": [{"name": "svc", "branch": "b"}]},
                 memo="did a thing")
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("src", "fork1")), 0)
        self.assertEqual(workspace.read_vision("fork1"), "Big goal")
        mem = self._memtext("fork1")
        self.assertIn("did a thing", mem)          # inherited memory files travelled
        self.assertIn("Forked from 'src'", mem)    # + the fork's own memory
        m = workspace.read_manifest("fork1")
        self.assertEqual(m["name"], "fork1")
        self.assertEqual(m["forked_from"], "src")
        self.assertEqual(m["repos"], [{"name": "svc", "branch": "b"}])

    def test_fork_starts_local(self):
        self._mk("src", vision="g")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        self.assertFalse(workspace.is_live("fork1"))

    def test_fork_live_flag(self):
        (config.ROOT / ".gitignore").write_text("/workspaces/*/*\n")
        self._mk("src", vision="g")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1", live=True))
        self.assertTrue(workspace.is_live("fork1"))

    def test_fork_refuses_existing_and_missing(self):
        self._mk("src")
        self._mk("taken")
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("src", "taken")), 1)
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("ghost", "new")), 1)
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("src", "src")), 1)

    # --- fork inherits open todos ---
    #
    # Fork exists so someone can pick a task up with full context, and what is still to be
    # done is the most actionable part of it. Inheriting everything the task LEARNED while
    # dropping everything it still INTENDED leaves the fork re-deriving the plan — the exact
    # problem todos were built to end.

    def test_a_fork_starts_with_the_sources_open_todos(self):
        self._mk("src", vision="g")
        todos.add("src", "prove the live gh path")
        todos.add("src", "rewrite the status line frame")
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("src", "fork1")), 0)
        self.assertEqual([t["title"] for t in todos.open_todos("fork1")],
                         ["prove the live gh path", "rewrite the status line frame"])

    def test_forking_leaves_the_sources_list_untouched(self):
        self._mk("src", vision="g")
        todos.add("src", "prove the live gh path")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        self.assertEqual([t["title"] for t in todos.open_todos("src")],
                         ["prove the live gh path"])

    def test_closing_in_the_fork_does_not_close_it_in_the_source(self):
        """A fork is a copy, so the two lists diverge from that moment. Sharing them would
        make finishing work in one workspace silently rewrite another's plan."""
        self._mk("src", vision="g")
        todos.add("src", "prove the live gh path")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        slug = todos.open_todos("fork1")[0]["slug"]
        cw.cmd_workspace_todo(SimpleNamespace(workspace="fork1", text="done", slug=slug,
                                              query=None))
        self.assertEqual(todos.open_todos("fork1"), [])
        self.assertEqual(len(todos.open_todos("src")), 1)

    def test_closing_in_the_source_does_not_close_it_in_the_fork(self):
        self._mk("src", vision="g")
        todos.add("src", "prove the live gh path")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        slug = todos.open_todos("src")[0]["slug"]
        cw.cmd_workspace_todo(SimpleNamespace(workspace="src", text="done", slug=slug,
                                              query=None))
        self.assertEqual(todos.open_todos("src"), [])
        self.assertEqual(len(todos.open_todos("fork1")), 1)

    def test_forking_a_workspace_with_no_todos_succeeds_with_none(self):
        """Most workspaces have no todos at all, so the empty case is the common one — it
        must not be an error, and must not leave the fork with a phantom list."""
        self._mk("src", vision="g")
        self.assertEqual(cw.cmd_workspace_fork(self._fork_args("src", "fork1")), 0)
        self.assertEqual(todos.open_todos("fork1"), [])

    def test_an_inherited_todo_keeps_its_original_age(self):
        """Age is the only ranking the feature has, and it is what makes a stale intent
        visible. Resetting it at the fork would hide a three-week-old todo behind a fresh
        date — precisely the todo most worth surfacing."""
        self._mk("src", vision="g")
        todos.add("src", "an old one",
                  stamp=datetime.datetime.now() - datetime.timedelta(days=30))
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        self.assertEqual(todos.open_todos("fork1")[0]["age_days"], 30)

    def test_an_inherited_todo_can_be_closed_by_the_slug_the_fork_lists(self):
        """Inheriting the files but not their index lines would leave todos that list but
        cannot be addressed — the store's index is how a slug resolves."""
        self._mk("src", vision="g")
        todos.add("src", "prove the live gh path")
        cw.cmd_workspace_fork(self._fork_args("src", "fork1"))
        slug = todos.open_todos("fork1")[0]["slug"]
        self.assertIn(slug, todos.index("fork1").read_text())

    # --- committed for LIVE ---
    def test_charter_committed_for_live(self):
        self._mk("w", vision="g")
        rels = cw._ws_meta_paths("w")
        self.assertIn("workspaces/w/workspace.md", rels)

    def test_live_block_unignores_charter(self):
        self.assertIn("!/workspaces/w/workspace.md", workspace._live_block(["w"]))


if __name__ == "__main__":
    unittest.main()
