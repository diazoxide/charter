"""A persona declares the skills it starts holding.

A persona is a durable agent; a skill is a repeatable workflow. The link between them
existed only as prose that `_skill_ref_issues` linted — which catches a dead *mention* and
can do nothing else, because prose cannot be acted on.

`skills:` can. The host **preloads** each declared skill's full text into the sub-agent at
startup, so a declared skill is standing equipment rather than something the agent might
discover mid-task. That is the one thing charter can do with a list that reading the charter
cannot, and it is why the key earns its place beside prose rather than duplicating it
(ADR 0010: two sources, two questions — prose says *when and how*, this says *what you start
holding*).

**Not an allowlist.** The host has none: a sub-agent can still invoke unlisted skills through
the Skill tool, and the only real restriction is withholding `Skill` itself. Charter must not
imply an enforcement it cannot deliver.

The cost is the reason lint is strict here. Full text, injected on EVERY dispatch, for as
long as the line exists — so a dead entry is paid forever for nothing.
"""

from __future__ import annotations

import unittest

from charter import persona
from charter import commands_persona as cp
from tests._isolation import PersonaIso


class SkillsCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self._orig_skills = persona._installed_skills
        persona._installed_skills = lambda: {
            "test-driven-development": True,
            "systematic-debugging": True,
            "ask-matt": False,          # human-only
        }
        self.addCleanup(setattr, persona, "_installed_skills", self._orig_skills)

    def make(self, name="dev", skills=None, **kw):
        self.make_persona(name, role="Dev", vault="none", **kw)
        if skills is not None:
            p = persona.def_path(name)
            text = p.read_text()
            text = text.replace("---\n", f"---\nskills: {skills}\n", 1)
            p.write_text(text)
        return name


class TestDeclaringSkills(SkillsCase):
    def test_a_persona_with_no_skills_declares_none(self):
        self.make("dev")
        self.assertEqual(persona.declared_skills("dev"), [])

    def test_declared_skills_are_read_in_order(self):
        self.make("dev", skills="test-driven-development, systematic-debugging")
        self.assertEqual(persona.declared_skills("dev"),
                         ["test-driven-development", "systematic-debugging"])

    def test_a_plugin_qualified_ref_is_kept_whole(self):
        """The host takes the qualified form; charter must not helpfully strip it."""
        self.make("dev", skills="superpowers:test-driven-development")
        self.assertEqual(persona.declared_skills("dev"),
                         ["superpowers:test-driven-development"])


class TestItIsPreloadedIntoTheAgent(SkillsCase):
    def _agent(self, name="dev") -> str:
        d = persona.load(name)
        return cp._render_agent(name, d["meta"], d["charter"])

    def test_the_generated_agent_carries_the_skills_key(self):
        self.make("dev", skills="test-driven-development")
        self.assertIn("skills: test-driven-development", self._agent())

    def test_a_persona_declaring_none_emits_no_key(self):
        """An empty `skills:` line would preload nothing and still read as a declaration."""
        self.make("dev")
        self.assertNotIn("skills:", self._agent())

    def test_it_does_not_call_them_allowed(self):
        """Charter must not imply an enforcement the host cannot deliver: a sub-agent can
        still invoke unlisted skills, and the only real restriction is withholding `Skill`."""
        self.make("dev", skills="test-driven-development")
        agent = self._agent().lower()
        self.assertNotIn("allowed skills", agent)
        self.assertNotIn("allowedskills", agent)


class TestLintingWhatWasDeclared(SkillsCase):
    def test_an_uninstalled_skill_is_an_error(self):
        """Worse than a dead prose mention: prose is advice a reader can route around, this
        is emitted into the agent and fails silently at dispatch."""
        self.make("dev", skills="does-not-exist")
        issues = persona.declared_skill_issues("dev")
        self.assertTrue(any(lvl == "error" for lvl, _ in issues), issues)

    def test_a_human_only_skill_is_a_warning_not_an_error(self):
        """Preloading its text is harmless — it just can never be invoked. That is a
        different severity from a skill that is not there at all."""
        self.make("dev", skills="ask-matt")
        issues = persona.declared_skill_issues("dev")
        self.assertTrue(issues)
        self.assertTrue(all(lvl == "warn" for lvl, _ in issues), issues)

    def test_a_good_declaration_is_clean(self):
        self.make("dev", skills="test-driven-development, systematic-debugging")
        self.assertEqual(persona.declared_skill_issues("dev"), [])

    def test_lint_includes_it(self):
        self.make("dev", skills="does-not-exist")
        self.assertTrue(any("does-not-exist" in m for _l, m in persona.lint("dev")))

    def test_the_shallow_lint_skips_it(self):
        """`deep=False` is what the status line renders on every turn — it must not walk
        the plugin cache."""
        self.make("dev", skills="does-not-exist")
        self.assertFalse(any("does-not-exist" in m
                             for _l, m in persona.lint("dev", deep=False)))

    def test_no_plugin_cache_means_no_opinion(self):
        """On a fresh clone or CI without plugins the skills cannot be resolved, and
        inventing failures there would make lint useless exactly where it runs unattended."""
        persona._installed_skills = lambda: None
        self.make("dev", skills="does-not-exist")
        self.assertEqual(persona.declared_skill_issues("dev"), [])


if __name__ == "__main__":
    unittest.main()
