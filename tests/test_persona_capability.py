"""Executable capability: where a persona's servers apply, and what it may declare.

Two issues, one theme — charter says what it can actually deliver.

**#186.** The reporter migrated persona knowledge into charter and concluded the executable
half was missing: `charter persona use <a>` left another persona's five analytics servers
live for the whole session. The MCP story existed — servers are declared per persona, emitted
inline into the generated sub-agent, and launched through `charter secret exec` so the
credential never reaches a context window — but it applies at **dispatch**, which is not the
boundary they were using.

Charter names that boundary rather than moving it. Moving it would mean writing
`disabledMcpServers` into a user-owned settings file and then owning it forever: stateful,
subtractive, and effective only on the *next* session — so `persona use` would print a
scoping claim untrue of the session you are in. Naming beats resolving where the tool cannot
honestly deliver (the call #140 made, for the same reason).

**#185.** `isolation` used to be a dispatch-time parameter with no agent-side way to declare
it — the code said so, "confirmed against the shipped schema". The host has since gained an
`isolation:` frontmatter field, so a persona that writes code can now isolate *itself*
instead of asking the router to remember. That is emitted from the `dispatch-isolation:` key
that already meant it, not a second spelling of the same idea.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import persona
from charter import commands_persona as cp
from tests._isolation import PersonaIso


class CapabilityCase(PersonaIso):
    def make(self, name="dev", **front):
        self.make_persona(name, role="Dev", vault="none")
        if front:
            p = persona.def_path(name)
            block = "".join(f"{k}: {v}\n" for k, v in front.items())
            p.write_text(p.read_text().replace("---\n", f"---\n{block}", 1))
        return name

    def agent(self, name="dev") -> str:
        d = persona.load(name)
        return cp._render_agent(name, d["meta"], d["charter"])

    def use(self, name="dev") -> str:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            cp.cmd_persona_use(SimpleNamespace(name=name))
        return out.getvalue() + err.getvalue()


class TestIsolationIsDeclaredNotRequested(CapabilityCase):
    def test_the_agent_declares_isolation(self):
        """The host gained the field, so the persona isolates itself. Previously charter
        could only put a sentence in the description and hope the router passed it."""
        self.make("dev", **{"dispatch-isolation": "worktree"})
        self.assertIn("isolation: worktree", self.agent())

    def test_a_persona_that_does_not_ask_for_it_gets_nothing(self):
        self.make("dev")
        self.assertNotIn("isolation:", self.agent())

    def test_there_is_only_one_spelling_in_a_charter(self):
        """`dispatch-isolation:` predates the host's field. Adding `isolation:` as a second
        charter key would churn every persona to say the same thing twice."""
        self.assertIn("dispatch-isolation", cp._CHARTER_OWN_KEYS)
        self.assertNotIn("isolation", cp._CHARTER_OWN_KEYS)

    def test_the_description_no_longer_asks_the_router_to_pass_it(self):
        """Stale advice is worse than none: it tells the caller to do something the agent
        now does for itself."""
        self.make("dev", **{"dispatch-isolation": "worktree"})
        desc = cp._agent_description("dev", persona.load("dev")["meta"])
        self.assertNotIn("Dispatch with isolation", desc)
        self.assertIn("own git worktree", desc)


class TestDisallowedTools(CapabilityCase):
    def test_a_denylist_is_emitted(self):
        """Often the honest shape: "everything except Bash" is one line here and an
        enumeration of every other tool under `agent-tools`."""
        self.make("dev", **{"disallowed-tools": "Bash, WebFetch"})
        self.assertIn("disallowedTools: Bash, WebFetch", self.agent())

    def test_nothing_is_emitted_when_not_declared(self):
        self.make("dev")
        self.assertNotIn("disallowedTools", self.agent())

    def test_permission_widening_fields_are_deliberately_absent(self):
        """`permissionMode` and `maxTurns` would let a persona charter widen its own
        permissions. Charter's stance elsewhere is deliberately conservative about exactly
        that, so they are left out rather than passed through unexamined."""
        self.make("dev", **{"permissionMode": "bypassPermissions", "maxTurns": "99"})
        agent = self.agent()
        self.assertNotIn("permissionMode", agent)
        self.assertNotIn("maxTurns", agent)


class TestTheMcpBoundaryIsNamed(CapabilityCase):
    def declare_server(self, name="dev"):
        d = persona.dir_of(name)
        d.mkdir(parents=True, exist_ok=True)
        (d / "mcp.json").write_text(
            '{"mcpServers": {"analytics": '
            '{"command": "npx", "args": ["-y", "analytics-mcp"]}}}')

    def test_persona_use_says_the_servers_are_dispatch_scoped(self):
        """The reporter's misreading, pre-empted at the moment it forms: they ran
        `persona use` and expected the session to be scoped."""
        self.make("dev")
        self.declare_server()
        out = self.use()
        self.assertIn("analytics", out)
        self.assertIn("DISPATCH", out)

    def test_it_says_servers_already_live_stay_live(self):
        """The exact surprise: another persona's servers remained in scope after switching."""
        self.make("dev")
        self.declare_server()
        self.assertIn("stay live", self.use())

    def test_a_persona_with_no_servers_says_nothing_about_mcp(self):
        """A line that renders for every persona is a line nobody reads."""
        self.make("dev")
        out = self.use()
        self.assertNotIn("DISPATCH", out)

    def test_it_never_breaks_persona_use(self):
        """`persona use` is a navigation command; a malformed mcp.json must not take it
        down — the same rule `mcp_servers` already follows."""
        self.make("dev")
        d = persona.dir_of("dev")
        (d / "mcp.json").write_text("{not json")
        out = self.use()
        self.assertIn("Active persona set to", out)


if __name__ == "__main__":
    unittest.main()
