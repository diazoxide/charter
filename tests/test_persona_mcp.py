"""A persona declares its own MCP servers, and charter wraps them in its vault.

Claude Code scopes MCP servers per sub-agent: `mcpServers:` in the agent frontmatter takes
inline definitions that are "connected when the subagent starts and disconnected when it
finishes", and whose tool descriptions never reach the parent conversation. charter
generates those agent files, so a hand-written block there is destroyed by the next
`sync-agents` — charter has to emit it or the capability is unreachable from a persona.

The definition lives in a sidecar, `personas/<name>/mcp.json`, for two reasons that are not
style: charter's frontmatter parser is line-based and charter carries no runtime
dependencies, so nested YAML cannot be parsed in `persona.md` — only emitted. A sidecar
also takes a server's own README snippet unchanged.

The wrapper is the point. `secrets` maps an env var the third-party server demands to a key
in the persona's vault, and charter turns the whole entry into a `charter secret exec …
--exec` invocation. The generated file names the keys; the values never enter a context
window. `--exec` is required because an MCP stdio server never returns.
"""

from __future__ import annotations

import json
import unittest

from charter import mcpseen, persona
from tests._isolation import PersonaIso


SERVER = {
    "mcpServers": {
        "reddit": {
            "type": "stdio",
            "command": "uvx",
            "args": ["some-reddit-mcp", "--read-only"],
            "secrets": {"REDDIT_CLIENT_ID": "client-id",
                        "REDDIT_CLIENT_SECRET": "client-secret"},
        }
    }
}


class McpBase(PersonaIso):
    def _persona(self, name="reddit", servers=SERVER, **meta):
        self.make_persona(name, role="Reddit Manager", vault=meta.pop("vault", "reddit"),
                          **{"delegate-when": "reddit things", **meta})
        if servers is not None:
            (persona.dir_of(name) / "mcp.json").write_text(json.dumps(servers))
        # `mcp_render_entry` hands the vault's value to the command a COMMITTED file
        # names, so it now renders the wrapper only for a command this operator has
        # approved (#330). Everything below is about the SHAPE of that wrapper, so the
        # fixture consents and the tests assert what consent produces. The refusing half
        # lives in tests/test_committed_config_and_credentials.py.
        mcpseen.approve(name, [fp for _s, _e, fp, _l in persona.mcp_credentialed(name)])
        return name


class TestReadingTheSidecar(McpBase):
    def test_no_sidecar_means_no_servers(self):
        self._persona(servers=None)
        self.assertEqual(persona.mcp_servers("reddit"), {})

    def test_it_reads_the_declared_servers(self):
        self._persona()
        self.assertIn("reddit", persona.mcp_servers("reddit"))

    def test_malformed_json_does_not_explode(self):
        """A sidecar is edited by hand; a broken one must not take down sync-agents."""
        self._persona()
        (persona.dir_of("reddit") / "mcp.json").write_text("{ not json")
        self.assertEqual(persona.mcp_servers("reddit"), {})

    def test_servers_are_inherited_and_unioned(self):
        """Matching `tools`/`uses`: list-like fields union, parent first, child wins on a
        name collision. A child silently losing its parent's server would be a surprise."""
        self.make_persona("base", role="Base", vault="reddit",
                          **{"delegate-when": "base"})
        (persona.dir_of("base") / "mcp.json").write_text(json.dumps(SERVER))
        self.make_persona("child", role="Child", extends="base",
                          **{"delegate-when": "child"})
        self.assertIn("reddit", persona.mcp_servers("child"))


class TestTheVaultWrapper(McpBase):
    def _entry(self, name="reddit"):
        return persona.mcp_render_entry(name, "reddit",
                                        persona.mcp_servers(name)["reddit"])

    def test_the_command_becomes_charter(self):
        self._persona()
        self.assertEqual(self._entry()["command"], "charter")

    def test_it_execs_the_original_command_after_a_separator(self):
        self._persona()
        args = self._entry()["args"]
        self.assertIn("--exec", args)
        self.assertEqual(args[args.index("--") + 1:], ["uvx", "some-reddit-mcp", "--read-only"])

    def test_every_secret_becomes_an_env_flag(self):
        self._persona()
        args = self._entry()["args"]
        self.assertIn("REDDIT_CLIENT_ID=client-id", args)
        self.assertIn("REDDIT_CLIENT_SECRET=client-secret", args)

    def test_it_names_the_personas_vault(self):
        self._persona()
        args = self._entry()["args"]
        self.assertEqual(args[:3], ["secret", "exec", "reddit"])

    def test_no_secrets_means_no_wrapper(self):
        """A server needing no credential should not be dragged through charter."""
        plain = {"mcpServers": {"reddit": {"type": "stdio", "command": "uvx",
                                           "args": ["public-server"]}}}
        self._persona(servers=plain)
        e = self._entry()
        self.assertEqual(e["command"], "uvx")
        self.assertEqual(e["args"], ["public-server"])

    def test_the_secrets_key_is_consumed_not_emitted(self):
        """It is charter's own field. Leaving it in would hand Claude Code an unknown key
        and put the vault key names in the server config for no reason."""
        self._persona()
        self.assertNotIn("secrets", self._entry())

    def test_no_secret_VALUE_appears_anywhere(self):
        self._persona()
        blob = json.dumps(self._entry())
        self.assertNotIn("hunter2", blob)   # nothing resolves at render time
        self.assertIn("client-id", blob)    # the KEY is named, which is the point


class TestTheGeneratedAgent(McpBase):
    def _rendered(self, name="reddit"):
        from charter.commands_persona import _render_agent
        d = persona.resolve(name)
        return _render_agent(name, d["meta"], d["charter"])

    def test_the_agent_declares_the_server(self):
        self._persona()
        self.assertIn("mcpServers:", self._rendered())

    def test_the_block_is_parseable_as_the_host_expects(self):
        """A list whose entries are single-key maps. JSON is valid YAML, so the value is
        emitted as JSON rather than by hand-rolling a YAML writer."""
        self._persona()
        out = self._rendered()
        line = next(l for l in out.splitlines() if l.strip().startswith("- reddit:"))
        payload = json.loads(line.split("- reddit:", 1)[1].strip())
        self.assertEqual(payload["command"], "charter")

    def test_declaring_a_server_grants_its_tools(self):
        """Two hand-kept lists that must agree is a divergence generator, and this one
        fails at dispatch with an error about unresolved entries."""
        self._persona(**{"agent-tools": "Read, Bash"})
        self.assertIn("mcp__reddit__*", self._rendered())

    def test_no_agent_tools_means_no_derived_grant(self):
        """Omitting `tools:` inherits everything, so adding a narrowing line would be a
        downgrade, not a grant."""
        self._persona()
        self.assertNotIn("tools:", self._rendered())

    def test_a_persona_without_servers_is_unchanged(self):
        self._persona(servers=None)
        self.assertNotIn("mcpServers", self._rendered())


class TestLintReportsWhatItCannotResolve(McpBase):
    def _levels(self, name="reddit"):
        return {msg for _lvl, msg in persona.lint(name)}

    def test_secrets_without_a_vault_is_an_error(self):
        self._persona(vault="")
        self.assertTrue(any("vault" in m and "mcp" in m.lower() for m in self._levels()),
                        f"expected an mcp/vault complaint, got: {self._levels()}")

    def test_a_clean_declaration_lints_clean(self):
        self._persona()
        self.assertFalse([m for m in self._levels() if "mcp" in m.lower()])


if __name__ == "__main__":
    unittest.main()
