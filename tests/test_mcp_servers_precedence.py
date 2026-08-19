"""`mcp_servers()` inverted its own documented precedence (#296).

The docstring states the rule: "parent first, child wins on a name collision. A child that
silently lost the server its parent declared would be a surprise, and this is the rule the
rest of the frontmatter already follows."

The loop iterated `lineage()`, which is child-first, into a `dict.update` — last write wins —
so the most distant ancestor overwrote the child. The guarded-against surprise happened, in
the other direction: `sync-agents` succeeds, the generated agent carries the ancestor's
config, and the only symptom is a server behaving like somebody else's.

This is the one place in the inheritance model where a child lost. `resolve()` merges scalar
frontmatter child-wins; `effective_tools`/`uses` union without a collision concept. That
asymmetry is what marks it a slip rather than a design.
"""
from __future__ import annotations

import json
import unittest

from charter import persona
from tests._isolation import PersonaIso


class McpPrecedence(PersonaIso):
    def declare(self, owner: str, server: str, command: str) -> None:
        d = persona.dir_of(owner)
        d.mkdir(parents=True, exist_ok=True)
        (d / persona.MCP_FILE).write_text(
            json.dumps({"mcpServers": {server: {"command": command}}}))

    def test_a_child_overrides_its_parents_server(self):
        self.make_persona("base", role="Base", vault="none")
        self.declare("base", "api", "parent-cmd")
        self.make_persona("child", role="Child", vault="none", extends="base")
        self.declare("child", "api", "child-cmd")
        self.assertEqual(persona.mcp_servers("child")["api"]["command"], "child-cmd")

    def test_a_grandparent_does_not_win_over_the_child(self):
        """Two levels, because `dict.update` over a child-first list made the FURTHEST
        ancestor win — so a one-level test could pass on a half-fix."""
        self.make_persona("grand", role="G", vault="none")
        self.declare("grand", "api", "grand-cmd")
        self.make_persona("mid", role="M", vault="none", extends="grand")
        self.declare("mid", "api", "mid-cmd")
        self.make_persona("kid", role="K", vault="none", extends="mid")
        self.declare("kid", "api", "kid-cmd")
        self.assertEqual(persona.mcp_servers("kid")["api"]["command"], "kid-cmd")

    def test_an_inherited_server_is_still_inherited(self):
        """The property the docstring was protecting in the first place: a child that
        declares nothing still gets its parent's server."""
        self.make_persona("base", role="Base", vault="none")
        self.declare("base", "api", "parent-cmd")
        self.make_persona("child", role="Child", vault="none", extends="base")
        self.assertEqual(persona.mcp_servers("child")["api"]["command"], "parent-cmd")

    def test_distinct_servers_along_the_chain_all_survive(self):
        self.make_persona("base", role="Base", vault="none")
        self.declare("base", "from-parent", "p")
        self.make_persona("child", role="Child", vault="none", extends="base")
        self.declare("child", "from-child", "c")
        self.assertEqual(sorted(persona.mcp_servers("child")), ["from-child", "from-parent"])


if __name__ == "__main__":
    unittest.main()
