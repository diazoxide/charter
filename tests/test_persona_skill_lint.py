"""Lint guard: a persona charter that names a `plugin:skill` for AGENT use must resolve to
an installed, MODEL-invokable skill. Human-only skills (disable-model-invocation) belong in
the /slash form, not a sub-agent brief — this is the exact rot that shipped un-caught before
(the steward charter naming the human-only `mattpocock-skills:ask-matt`)."""

from __future__ import annotations

import unittest

from charter import persona


class SkillRefLintCase(unittest.TestCase):
    def setUp(self):
        self._orig_sk = persona._installed_skills
        self._orig_pl = persona._enabled_plugin_names
        # controlled world: two enabled plugins; one model-ok skill, two human-only.
        persona._installed_skills = lambda: {
            "test-driven-development": True, "grilling": True,
            "ask-matt": False, "to-spec": False,
        }
        persona._enabled_plugin_names = lambda: {"superpowers", "mattpocock-skills"}
        self.addCleanup(self._restore)

    def _restore(self):
        persona._installed_skills = self._orig_sk
        persona._enabled_plugin_names = self._orig_pl

    def test_model_invokable_ok(self):
        self.assertEqual(
            persona._skill_ref_issues("use `superpowers:test-driven-development` first"), [])

    def test_human_only_is_flagged(self):
        issues = persona._skill_ref_issues("front it with `mattpocock-skills:ask-matt`")
        self.assertTrue(any("human-only" in m for _l, m in issues), issues)

    def test_unknown_skill_is_flagged(self):
        issues = persona._skill_ref_issues("use `superpowers:does-not-exist`")
        self.assertTrue(any("not an installed skill" in m for _l, m in issues), issues)

    def test_non_enabled_plugin_ignored(self):
        # `edm:foo` / `random:bar` don't reference an enabled plugin → not a skill ref
        self.assertEqual(persona._skill_ref_issues("`edm:foo` and `random:bar`"), [])

    def test_slash_and_bare_forms_not_checked(self):
        # /to-spec (human slash) and `to-spec` (no plugin: prefix) are human steps, not agent refs
        self.assertEqual(
            persona._skill_ref_issues("run /to-spec, then `to-spec` and `/grill-with-docs`"), [])

    def test_no_cache_skips_gracefully(self):
        persona._installed_skills = lambda: None
        self.assertEqual(persona._skill_ref_issues("`mattpocock-skills:ask-matt`"), [])


class UnknownFrontmatterKey(unittest.TestCase):
    """#8: a charter key charter neither reads nor emits reaches nothing.

    `modell:` or `delegate_when:` is silently inert today — not copied into the
    generated sub-agent, not consulted by any charter code. Lint says so.
    """

    def test_the_two_key_sets_cover_every_key_a_real_charter_uses(self):
        """A false positive here would train people to ignore the lint."""
        from charter.commands_persona import _AGENT_PASSTHROUGH_KEYS, _CHARTER_OWN_KEYS
        known = set(_AGENT_PASSTHROUGH_KEYS) | set(_CHARTER_OWN_KEYS)
        for key in ("name", "role", "vault", "extends", "uses", "delegate-when",
                    "tools", "agent-tools", "model", "color", "memory", "activity"):
            self.assertIn(key, known, f"real charters use {key!r}")

    def test_the_two_sets_do_not_overlap(self):
        """A key in both would mean charter reads it AND emits it — say which."""
        from charter.commands_persona import _AGENT_PASSTHROUGH_KEYS, _CHARTER_OWN_KEYS
        self.assertEqual(set(_AGENT_PASSTHROUGH_KEYS) & set(_CHARTER_OWN_KEYS), set())


if __name__ == "__main__":
    unittest.main()
