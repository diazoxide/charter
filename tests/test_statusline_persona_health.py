"""Persona health in the status-line chips.

Two rules govern anything added here, because the status line renders on **every
turn**:

* **cheap** — the full `persona lint` costs ~30ms even memoised, because of the
  plugin-cache walk; the chips use `lint(deep=False)`, which is frontmatter
  arithmetic (~0.1ms per persona) and never touches the plugin cache;
* **silent when healthy** — a mark appears only when something is wrong. A row of
  ✓s becomes furniture within a day, and then a real ✗ in it gets no more attention
  than a zero would. Presence IS the signal.

Soft findings (no role, no `delegate-when`) deliberately stay in `lint`/`doctor`,
where there is room to explain them. The chips carry only what stops the persona
working: it is a draft (undispatchable) or its config is broken.
"""

from __future__ import annotations

import re
import unittest

from charter import config, persona, statusline
from tests._isolation import PersonaIso


def _plain(s: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", s)


def _chip(name: str) -> str:
    for c in statusline._persona_chips():
        if re.search(rf"\b{re.escape(name)}\b", _plain(c)):
            return _plain(c)
    raise AssertionError(f"no chip for {name}")


class Marks(PersonaIso):
    def _persona(self, name, **meta):
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        fm = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{fm}\n---\n\n# {name}\n\ncharter body\n")
        persona.scaffold_memory(name)

    def _ok(self, name):
        self._persona(name, role=name.title(), vault=name,
                      **{"delegate-when": f"{name} work"})

    def test_a_healthy_chip_carries_no_health_mark(self):
        self._ok("alpha")
        self.assertNotIn("⚑", _chip("alpha"))
        self.assertNotIn("✗", _chip("alpha"))

    def test_a_draft_is_flagged(self):
        self._persona("wip", role="W", vault="wip", draft="true",
                      **{"delegate-when": "later"})
        self.assertIn("⚑", _chip("wip"))

    def test_broken_config_is_flagged(self):
        self._persona("orphan", role="O", vault="orphan", extends="ghost",
                      **{"delegate-when": "x"})
        self.assertIn("✗", _chip("orphan"))

    def test_a_dangling_uses_is_flagged(self):
        self._persona("borrower", role="B", vault="borrower", uses="ghost",
                      **{"delegate-when": "x"})
        self.assertIn("✗", _chip("borrower"))

    def test_soft_warnings_are_not_flagged(self):
        """`no delegate-when` is a real lint warning and deliberately invisible here —
        the chips are for what stops the persona working."""
        self._persona("thin", role="T", vault="thin")
        chip = _chip("thin")
        self.assertNotIn("⚑", chip)
        self.assertNotIn("✗", chip)

    def test_one_broken_persona_does_not_mark_the_others(self):
        self._ok("alpha")
        self._persona("orphan", role="O", vault="orphan", extends="ghost")
        self.assertNotIn("✗", _chip("alpha"))
        self.assertIn("✗", _chip("orphan"))


class NeverExpensive(PersonaIso):
    def test_rendering_chips_never_walks_the_plugin_cache(self):
        persona._reset_skill_cache()
        self.addCleanup(persona._reset_skill_cache)
        walks = []
        orig = persona._walk_installed_skills
        persona._walk_installed_skills = lambda: (walks.append(1), orig())[1]
        try:
            for n in ("a", "b", "c"):
                d = config.PERSONAS_DIR / n
                d.mkdir(parents=True, exist_ok=True)
                (d / "persona.md").write_text(
                    f"---\nname: {n}\nrole: R\nvault: {n}\ndelegate-when: x\n---\n\n"
                    "Uses `superpowers:test-driven-development`.\n")
                persona.scaffold_memory(n)
            statusline._persona_chips()
        finally:
            persona._walk_installed_skills = orig
        self.assertEqual(walks, [], "the status line must not walk the plugin cache")

    def test_a_broken_persona_never_breaks_the_render(self):
        d = config.PERSONAS_DIR / "bad"
        d.mkdir(parents=True, exist_ok=True)
        (d / "persona.md").write_text("---\nnot: [valid\n---\n")
        statusline._persona_chips()      # must not raise


class VaultDot(PersonaIso):
    """Three states, not two. `plain-file.health()` returns ``ok=True`` with the detail
    "not created yet (<path>)" for a vault that is registered but has no file — so
    reading only ``ok`` painted a green ✓ on a vault that does not exist, while
    `charter persona list` (which prints the detail) correctly said otherwise.
    """

    def _dot(self, ok, detail, registered=True):
        # `_vault_dot` reads through `_vault_health`, which memoises per process and
        # caches to STATE_DIR with a TTL — so every case here would otherwise get the
        # FIRST case's answer, and (before this class was isolated) write it into the
        # developer's real `.charter/cache/`.
        from charter.secrets import registry
        statusline._vault_memo.clear()
        self.addCleanup(statusline._vault_memo.clear)
        vaults, provider_for = registry.vaults, registry.provider_for
        registry.vaults = lambda: ({"v": {}} if registered else {})
        registry.provider_for = lambda _n: type("P", (), {"health": lambda _s: (ok, detail)})()
        try:
            return _plain(statusline._vault_dot("v"))
        finally:
            registry.vaults, registry.provider_for = vaults, provider_for

    def test_healthy_vault_is_a_check(self):
        self.assertIn("✓", self._dot(True, "3 secret(s)"))

    def test_registered_but_absent_is_distinct_from_healthy(self):
        dot = self._dot(True, "not created yet (/x/y.json)")
        self.assertNotIn("✓", dot)
        self.assertIn("◦", dot)

    def test_unhealthy_vault_is_a_bang(self):
        self.assertIn("!", self._dot(False, "unreadable"))

    def test_unregistered_vault_is_a_dim_dot(self):
        self.assertIn("·", self._dot(True, "", registered=False))

    def test_no_vault_named_is_a_dim_dot(self):
        self.assertIn("·", _plain(statusline._vault_dot(None)))


if __name__ == "__main__":
    unittest.main()
