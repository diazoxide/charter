"""SessionStart memory injection must be BOUNDED.

Root cause (measured 2026-07-27): the full `_shared` memory index was injected into every
session — 94 entries / ~3,068 tok — growing ~5 entries/day with no cap, and re-read by every
dispatched sub-agent. Since `edm recall` retrieves the same memories on demand, injecting the
whole index is redundant. Inject a digest instead: counts + the newest N titles + a pointer to
the search gate, so cost stays flat no matter how large the corpus grows.
"""

from __future__ import annotations

import unittest

from charter import hooks, persona
from tests._isolation import PersonaIso, PlaneIso, run_hook


class MemoryInjectionBoundedCase(PlaneIso):
    def setUp(self):
        super().setUp()
        self.make_persona("dev", role="Dev", vault="d")
        persona.set_active("dev")

    def _inject(self):
        r = run_hook(hooks.sessionstart, {"session_id": "s-inject"})
        return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")

    def test_injection_stays_bounded_as_corpus_grows(self):
        """The whole point: 10 memories and 200 memories must cost about the same."""
        for i in range(10):
            persona.remember("dev", f"shared fact number {i} about topic {i}", shared=True)
        small = len(self._inject())
        for i in range(10, 200):
            persona.remember("dev", f"shared fact number {i} about topic {i}", shared=True)
        big = len(self._inject())
        self.assertLess(big - small, 400,
                        f"injection grew {big - small}B when corpus grew 190 memories — unbounded")

    def test_caps_the_number_of_titles_listed(self):
        for i in range(60):
            persona.remember("dev", f"shared fact {i}", shared=True)
        ctx = self._inject()
        listed = ctx.count("- [")
        self.assertLessEqual(listed, hooks._MEM_DIGEST_N * 2 + 2,
                             f"{listed} index lines injected — should be capped")

    def test_reports_the_true_total_count(self):
        for i in range(37):
            persona.remember("dev", f"shared fact {i}", shared=True)
        self.assertIn("37", self._inject())      # agent still knows how much it knows

    def test_points_at_the_recall_gate(self):
        persona.remember("dev", "a shared fact", shared=True)
        self.assertIn("charter recall", self._inject())

    def test_keeps_the_newest_memories_visible(self):
        for i in range(40):
            persona.remember("dev", f"old fact {i}", shared=True)
        persona.remember("dev", "THE NEWEST DISTINCTIVE FACT", shared=True)
        self.assertIn("NEWEST DISTINCTIVE FACT", self._inject())

    def test_role_and_data_framing_survive(self):
        # the identity + prompt-injection framing must not be lost in the slimming
        persona.remember("dev", "x", shared=True)
        ctx = self._inject()
        self.assertIn("`dev` persona for this session", ctx)
        self.assertIn("reference **data**", ctx)

    def test_no_memory_still_injects_role(self):
        ctx = self._inject()
        self.assertIn("`dev` persona for this session", ctx)


if __name__ == "__main__":
    unittest.main()
