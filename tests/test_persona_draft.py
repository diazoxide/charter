"""`draft: true` — the honest label for a persona whose charter isn't written yet.

The defect this closes: `charter persona create x` wrote a scaffold full of
author-facing slots — `(describe what this persona owns and does)` — and
immediately generated `.claude/agents/x.md` from it. That file *is* a
sub-agent's system prompt, so dispatching the persona told an agent its
responsibilities were a parenthetical instruction to a human.

The rule is asymmetric on purpose, and the asymmetry is mechanical rather than
stylistic: **dispatch** bakes the whole charter into an agent's system prompt
with no human in the loop, while **adoption** injects only the identity line
(``You are the `x` persona for this session``, with the committed `role:` quoted
under a data label) and a pointer to `charter persona show`. So a draft may be adopted — that is how you work on it
— and may never be dispatched.
"""

from __future__ import annotations

import unittest

from charter import config, persona
from tests._isolation import PersonaIso


class _Base(PersonaIso):
    def _persona(self, name, body="Real charter text.", **meta):
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        fm = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{fm}\n---\n\n{body}\n")
        persona.scaffold_memory(name)


class IsDraft(_Base):
    """Frontmatter values are plain strings — `parse()` does no coercion — so the
    truthiness test must read the string, never `bool(meta.get("draft"))`."""

    def test_absent_is_not_draft(self):
        self._persona("p", role="R", vault="v")
        self.assertFalse(persona.is_draft("p"))

    def test_true_is_draft(self):
        self._persona("p", role="R", vault="v", draft="true")
        self.assertTrue(persona.is_draft("p"))

    def test_accepts_the_usual_affirmatives(self):
        for val in ("true", "True", "TRUE", "yes", "1"):
            with self.subTest(value=val):
                self._persona("p", role="R", vault="v", draft=val)
                self.assertTrue(persona.is_draft("p"))

    def test_false_is_not_draft(self):
        """`bool("false")` is True — the trap this test exists to catch."""
        for val in ("false", "False", "no", "0", ""):
            with self.subTest(value=val):
                self._persona("p", role="R", vault="v", draft=val)
                self.assertFalse(persona.is_draft("p"))

    def test_a_missing_persona_is_not_draft(self):
        self.assertFalse(persona.is_draft("nope"))

    def test_draft_inherits_through_extends(self):
        """A child extending a draft parent ships the parent's unfinished text in
        its own dispatched prompt, so it inherits the label like any other scalar."""
        self._persona("parent", role="P", vault="p", draft="true")
        self._persona("child", role="C", vault="c", extends="parent")
        self.assertTrue(persona.is_draft("child"))

    def test_a_child_may_explicitly_override_the_inherited_draft(self):
        self._persona("parent", role="P", vault="p", draft="true")
        self._persona("child", role="C", vault="c", extends="parent", draft="false")
        self.assertFalse(persona.is_draft("child"))


class Lint(_Base):
    def test_draft_is_a_warning(self):
        self._persona("p", role="R", vault="v", draft="true",
                      **{"delegate-when": "things"})
        msgs = [m for lvl, m in persona.lint("p") if lvl == "warn"]
        self.assertTrue(any("draft" in m for m in msgs), msgs)

    def test_draft_is_not_reported_as_an_unknown_key(self):
        """`draft` is read by charter, so it must be in the known-key whitelist —
        otherwise it trips the 'does nothing (typo?)' check it is meant to survive."""
        self._persona("p", role="R", vault="v", draft="true",
                      **{"delegate-when": "things"})
        msgs = [m for _lvl, m in persona.lint("p")]
        self.assertFalse(any("does nothing" in m for m in msgs), msgs)

    def test_a_finished_persona_lints_clean(self):
        self._persona("p", role="R", vault="v", **{"delegate-when": "things"})
        self.assertEqual(persona.lint("p"), [])


class ShallowLint(_Base):
    """`deep=False` drops the plugin-cache walk so the status line can afford it.

    One implementation, one flag — a parallel 'cheap health' function would drift
    from `lint` the first time a check was added to only one of them.
    """

    def test_shallow_skips_the_skill_reference_check(self):
        calls = []
        orig = persona._installed_skills
        persona._installed_skills = lambda: (calls.append(1), orig())[1]
        try:
            self._persona("p", "Uses `superpowers:not-a-real-skill` for agent work.",
                          role="R", vault="v", **{"delegate-when": "x"})
            persona.lint("p", deep=False)
        finally:
            persona._installed_skills = orig
        self.assertEqual(calls, [], "shallow lint must not walk the plugin cache")

    def test_shallow_still_reports_structural_errors(self):
        self._persona("p", role="R", vault="v", extends="ghost",
                      **{"delegate-when": "x"})
        errs = [m for lvl, m in persona.lint("p", deep=False) if lvl == "error"]
        self.assertTrue(any("dangling" in m for m in errs), errs)

    def test_shallow_still_reports_draft(self):
        self._persona("p", role="R", vault="v", draft="true",
                      **{"delegate-when": "x"})
        msgs = [m for _l, m in persona.lint("p", deep=False)]
        self.assertTrue(any("draft" in m for m in msgs), msgs)

    def test_deep_is_the_default(self):
        calls = []
        orig = persona._installed_skills
        persona._installed_skills = lambda: (calls.append(1), orig())[1]
        try:
            self._persona("p", "Uses `superpowers:test-driven-development`.",
                          role="R", vault="v", **{"delegate-when": "x"})
            persona.lint("p")
        finally:
            persona._installed_skills = orig
        self.assertEqual(len(calls), 1)


class SkillCacheIsCached(_Base):
    """The plugin-cache walk cost 27ms and ran once per persona — 358ms of a
    364ms roster lint, which is why `doctor` could not afford to call it."""

    def setUp(self) -> None:
        super().setUp()
        persona._reset_skill_cache()          # a cache is process-global; don't
        self.addCleanup(persona._reset_skill_cache)   # inherit or leak one

    def test_the_walk_happens_once_across_many_personas(self):
        walks = []
        orig = persona._walk_installed_skills
        persona._walk_installed_skills = lambda: (walks.append(1), orig())[1]
        try:
            for n in ("a", "b", "c", "d"):
                self._persona(n, "Uses `superpowers:test-driven-development`.",
                              role="R", vault=n, **{"delegate-when": "x"})
            for n in ("a", "b", "c", "d"):
                persona.lint(n)
        finally:
            persona._walk_installed_skills = orig
        self.assertEqual(len(walks), 1, f"walked {len(walks)}× — cache not shared")

    def test_the_cache_can_be_reset(self):
        walks = []
        orig = persona._walk_installed_skills
        persona._walk_installed_skills = lambda: (walks.append(1), orig())[1]
        try:
            persona._installed_skills()
            persona._reset_skill_cache()
            persona._installed_skills()
        finally:
            persona._walk_installed_skills = orig
        self.assertEqual(len(walks), 2)

    def test_a_none_result_is_cached_too(self):
        """No plugin cache on this machine is a stable answer, not a reason to
        re-walk a missing tree once per persona."""
        walks = []
        orig = persona._walk_installed_skills
        persona._walk_installed_skills = lambda: (walks.append(1), None)[1]
        try:
            self.assertIsNone(persona._installed_skills())
            self.assertIsNone(persona._installed_skills())
        finally:
            persona._walk_installed_skills = orig
            persona._reset_skill_cache()
        self.assertEqual(len(walks), 1)


if __name__ == "__main__":
    unittest.main()
