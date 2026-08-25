"""F2 — the forge abstraction must not leak into a generated sub-agent's wording.

`commands_persona._render_agent` used to hardcode `glab`/GitLab prose (git = "the glab
token over HTTPS", "`glab auth status` checks the credential") into every generated
`.claude/agents/<name>.md`, regardless of which forge(s) the control plane actually
declares in its own `charter.toml` (`[[forge]]` blocks, resolved via
`charter.forge.registry`). Those files land in the USER'S OWN repo — a GitHub-only
control plane's generated sub-agent must never tell the reader about `glab`, a tool it
never uses (and a GitLab-only one must never mention `gh`). Only charter's own
generated prose is in scope here: a persona's own `tools:` declaration (e.g. `tools:
kubectl, glab`) is THAT persona's choice and must be preserved as written — untouched
by this fix.
"""
from __future__ import annotations

import re
import unittest

from tests._isolation import PersonaIso
from charter import commands_persona, config


def _mentions(word: str, text: str) -> bool:
    """Whole-word match only — `gh` must not false-positive on `through`/`right`/…"""
    return re.search(rf"\b{re.escape(word)}\b", text) is not None


def _agent_text(name: str) -> str:
    return (config.ROOT / ".claude" / "agents" / f"{name}.md").read_text()


class TestGeneratedAgentForgeWordingMatchesDeclaredForges(PersonaIso):
    def _declare(self, toml: str) -> None:
        (config.ROOT / "charter.toml").write_text(toml)

    def test_github_only_control_plane_never_mentions_glab(self):
        self._declare('[[forge]]\nkind = "github"\nowner = "acme"\n')
        self.make_persona("dev", role="Developer", vault="dev")
        self.assertEqual(commands_persona._write_agent("dev"), "written")
        text = _agent_text("dev")
        self.assertFalse(_mentions("glab", text), text)
        self.assertIn("gh auth status", text)

    def test_gitlab_only_control_plane_never_mentions_gh(self):
        self._declare('[[forge]]\nkind = "gitlab"\ngroup = "acme"\n')
        self.make_persona("dev", role="Developer", vault="dev")
        self.assertEqual(commands_persona._write_agent("dev"), "written")
        text = _agent_text("dev")
        self.assertFalse(_mentions("gh", text), text)
        self.assertIn("glab auth status", text)

    def test_no_charter_toml_falls_back_to_the_historical_gitlab_wording(self):
        """Back-compat: no `[[forge]]` blocks at all (the shape every control plane had
        before multi-forge support existed) still gets the original glab wording."""
        self.make_persona("dev", role="Developer", vault="dev")
        self.assertEqual(commands_persona._write_agent("dev"), "written")
        text = _agent_text("dev")
        self.assertIn("glab auth status", text)
        self.assertFalse(_mentions("gh", text), text)

    def test_mixed_forge_control_plane_mentions_both_clis(self):
        self._declare(
            '[[forge]]\nkind = "gitlab"\ngroup = "acme"\n\n'
            '[[forge]]\nkind = "github"\nowner = "acme"\n'
        )
        self.make_persona("dev", role="Developer", vault="dev")
        self.assertEqual(commands_persona._write_agent("dev"), "written")
        text = _agent_text("dev")
        self.assertTrue(_mentions("glab", text), text)
        self.assertTrue(_mentions("gh", text), text)

    def test_a_personas_own_tools_declaration_is_preserved_verbatim(self):
        """Only charter's OWN generated prose adapts to the declared forge — a persona
        that names a specific CLI in `tools:` (echoed into the generated description,
        "Runs kubectl, glab and pulls credentials from …") keeps that literal
        declaration untouched, even for a GitHub-only control plane where charter's own
        credential-rule prose has switched to `gh`."""
        self._declare('[[forge]]\nkind = "github"\nowner = "acme"\n')
        self.make_persona("dev", role="Developer", vault="dev", tools="kubectl, glab")
        self.assertEqual(commands_persona._write_agent("dev"), "written")
        text = _agent_text("dev")
        self.assertIn("Runs kubectl, glab and pulls credentials", text)
        # charter's own generated credential-rule prose still adapted correctly:
        self.assertIn("gh auth status", text)


class GeneratedAgentBody(unittest.TestCase):
    """What `_render_agent` emits — the reviewer-reported gaps (#7, #8, #9)."""

    def _render(self, meta, name="p"):
        from charter.commands_persona import _render_agent
        return _render_agent(name, meta, "# charter body\n")

    def test_no_persona_name_is_hardcoded_in_the_handoff(self):
        """#9: it used to name `devops` literally, so a control plane whose infra
        persona is `sre` silently got no handoff line — while the sentence beside it
        already pointed at `charter persona list`, which is always correct."""
        body = self._render({"role": "R", "vault": "v"})
        self.assertNotIn("devops", body)
        self.assertIn("charter persona list", body)

    def test_memory_split_is_stated_when_the_charter_sets_memory(self):
        """#7: `memory:` gives the agent a second store. Say which is for what, or an
        agent told to record what's durable has two plausible places and no precedence."""
        body = self._render({"role": "R", "vault": "v", "memory": "true"}, name="qa")
        self.assertIn(".claude/agent-memory/qa/", body)
        self.assertIn("personas/qa/memory/", body)
        self.assertIn("charter recall", body)

    def test_no_memory_split_note_when_the_charter_does_not_set_memory(self):
        body = self._render({"role": "R", "vault": "v"}, name="qa")
        self.assertNotIn("agent-memory", body)

    def test_passthrough_keys_are_emitted(self):
        body = self._render({"role": "R", "vault": "v", "model": "opus", "color": "red"})
        self.assertIn("model: opus", body)
        self.assertIn("color: red", body)


class DispatchIsolationHint(unittest.TestCase):
    """#12, revised by #185: `isolation` WAS only an Agent-tool parameter chosen by the
    caller, so a persona that writes code could not isolate itself and the `description` was
    the only lever charter had — advisory by construction.

    The host has since gained an `isolation:` subagent frontmatter field. The persona now
    isolates ITSELF (`_render_agent` emits it from the `dispatch-isolation:` key that already
    meant this), so the description no longer asks the router for anything. It still says
    WHY this persona behaves differently, because that is what the router reads when picking
    one — but stale advice telling a caller to pass a parameter the agent now sets is worse
    than none.
    """

    def _desc(self, **extra):
        from charter.commands_persona import _agent_description
        meta = {"role": "Dev", "delegate-when": "writing code", "tools": "glab"}
        meta.update(extra)
        return _agent_description("dev", meta)

    def test_absent_by_default(self):
        self.assertNotIn("isolation", self._desc())

    def test_present_when_the_charter_asks_for_it(self):
        d = self._desc(**{"dispatch-isolation": "worktree"})
        self.assertIn("own git worktree", d)
        self.assertIn("share one working tree", d)   # says why, not just what
        self.assertNotIn("Dispatch with", d)         # no longer asks the caller to pass it

    def test_the_agent_declares_it_rather_than_requesting_it(self):
        """The upgrade #185 made possible: advisory prose became an enforced field."""
        from charter.commands_persona import _render_agent
        body = _render_agent("dev", {"role": "Dev", "vault": "v",
                                     "dispatch-isolation": "worktree"}, "# body\n")
        self.assertIn("isolation: worktree", body.split("---")[1])

    def test_only_worktree_is_recognised(self):
        """An unknown value must not silently produce a nonsense instruction. Asserted on
        `worktree` rather than the word "isolation", which the description no longer uses —
        the old assertion would now pass for every input."""
        self.assertNotIn("worktree", self._desc(**{"dispatch-isolation": "sandbox"}))

    def test_the_hint_lands_at_the_end_where_truncation_costs_least(self):
        """delegate-when triggers drive routing; the hint must not displace them."""
        d = self._desc(**{"dispatch-isolation": "worktree"})
        self.assertLess(d.index("writing code"), d.index("own git worktree"))

    def test_the_key_is_in_the_lint_vocabulary(self):
        """Otherwise the whitelist added for #8 would flag charter's own key."""
        from charter.commands_persona import _AGENT_PASSTHROUGH_KEYS, _CHARTER_OWN_KEYS
        self.assertIn("dispatch-isolation",
                      set(_AGENT_PASSTHROUGH_KEYS) | set(_CHARTER_OWN_KEYS))

    def test_it_is_not_emitted_as_frontmatter(self):
        """It is charter's own key — emitting it would be an invented agent field,
        exactly the mistake #1 was closed for."""
        from charter.commands_persona import _render_agent
        body = _render_agent("dev", {"role": "Dev", "vault": "v",
                                     "dispatch-isolation": "worktree"}, "# body\n")
        head = body.split("---")[1]
        self.assertNotIn("dispatch-isolation:", head)


if __name__ == "__main__":
    unittest.main()
