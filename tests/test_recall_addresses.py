"""`charter recall` returns an address, not a hint (#94).

`recall` is the command charter tells every agent to run before deciding something is
unknown. It printed `date · label · title` and then "find the file under its base
(workspaces/… or personas/…)" — which is a direction, not an address. `Hit.path` was
already carried on every result and never rendered.

The cost was a round trip per hit: eight results meant inferring eight filesystem paths,
with slug rules that differ between stores (workspace journal files carry a
`YYYYMMDD-HHMMSS-` prefix, persona files are slug-only), then eight `Read` calls to find
out which one mattered.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands, config, memstore, persona, workspace
from tests._isolation import PersonaIso


class RecallOutputCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        workspace.ensure("w")
        workspace.scaffold("w")
        # Title and body deliberately differ: for a one-line memory the title IS the body,
        # so a snippet test written against such a memory passes without a snippet.
        memstore.write(workspace.memory_dir("w"),
                       "the keycloak token rotates every ninety days",
                       title="keycloak token policy", timestamped=True)
        self.make_persona("dev", role="Dev", vault="d")
        persona.remember("dev", "keycloak deploys need the staging realm first")

    def recall(self, query="keycloak", **kw):
        args = SimpleNamespace(query=query, scope=None, ephemeral=False, persona="dev",
                               workspace="w", all_workspaces=False, since=None, limit=8,
                               full=False)
        for k, v in kw.items():
            setattr(args, k, v)
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_recall(args)
        return rc, out.getvalue() + err.getvalue()


class TestEveryHitCarriesItsPath(RecallOutputCase):
    def test_the_path_is_printed(self):
        rc, out = self.recall()
        self.assertEqual(rc, 0)
        self.assertIn("workspaces/w/memory/", out)

    def test_both_bases_render_their_own_path(self):
        """The slug rules differ per store, which is exactly why inferring them was the
        expensive part: the journal timestamps its filenames, persona memory does not."""
        _, out = self.recall()
        self.assertIn("workspaces/w/memory/", out)
        self.assertIn("personas/dev/memory/", out)

    def test_the_path_is_relative_to_the_plane_root(self):
        """An absolute path from a temp dir is noise in a transcript, and every other
        charter surface prints plane-relative paths."""
        _, out = self.recall()
        self.assertNotIn(str(config.ROOT), out)

    def test_it_no_longer_tells_the_reader_to_go_find_the_file(self):
        """The line this replaces. Leaving it beside real addresses would read as though
        the addresses were insufficient."""
        _, out = self.recall()
        self.assertNotIn("find the file under its base", out)

    def test_listing_without_a_query_also_carries_paths(self):
        _, out = self.recall(query=None)
        self.assertIn("workspaces/w/memory/", out)


class TestTheBodyIsAvailableWithoutASecondCall(RecallOutputCase):
    def test_full_prints_a_body_snippet(self):
        _, out = self.recall(full=True)
        self.assertIn("ninety days", out)

    def test_the_default_stays_compact(self):
        """Eight hits × a body is most of a screen. The address is what every caller needs;
        the body is what some do, so it is opt-in rather than the default."""
        _, out = self.recall()
        self.assertNotIn("ninety days", out)

    def test_full_still_prints_the_path(self):
        _, out = self.recall(full=True)
        self.assertIn("workspaces/w/memory/", out)

    def test_an_unreadable_body_does_not_break_the_listing(self):
        """`recall` runs constantly and must degrade to less, never to an error."""
        d = workspace.memory_dir("w")
        for p in d.glob("*.md"):
            if p.name != "MEMORY.md":
                p.chmod(0o000)
                self.addCleanup(p.chmod, 0o600)
        rc, out = self.recall(full=True)
        self.assertEqual(rc, 0)
        self.assertIn("personas/dev/memory/", out)


if __name__ == "__main__":
    unittest.main()
