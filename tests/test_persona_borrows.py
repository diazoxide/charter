"""`borrows:` splits what `uses:` overloaded — per persona, and only when asked.

`uses:` granted three things in one word: read that persona's vault, **run its tools with
auto-approval**, and delegate to its sub-agent. The middle grant is why delegation loses
every time it is offered: a front door declaring `uses: forge, release` can do both
personas' work with both personas' tools and never pay a permission prompt, while
delegating costs a dispatch, a brief and the context that goes with it. The incentive
points away from the behaviour the roster block exists to produce (#257).

The split is **per persona and never plane-wide**. A persona that declares `borrows:` takes
its auto-approved tools from that list, and `uses:` becomes a routing edge for that persona
alone. A persona that declares nothing keeps today's behaviour exactly. One persona's
opt-in must never change another persona's tool permissions — which is what a
`charter.toml` flag would have done.
"""
from __future__ import annotations

import unittest

from charter import persona
from tests._isolation import PersonaIso


class BorrowsIso(PersonaIso):
    def p(self, name, **meta):
        return self.make_persona(name, role=name.title(), vault="none",
                                 **{"delegate-when": f"{name} work", **meta})


class TestLegacyIsUntouched(BorrowsIso):
    def test_uses_still_grants_tools_when_borrows_is_absent(self):
        """Fail toward no change: an upgrade must alter nothing for a plane that has not
        opted in, or the split becomes a silent permission change."""
        self.p("forge", tools="gh")
        self.p("steward", uses="forge")
        self.assertIn("gh", persona.effective_tools("steward"))


class TestDeclaringBorrows(BorrowsIso):
    def test_tools_come_from_borrows_not_from_uses(self):
        self.p("forge", tools="gh")
        self.p("release", tools="twine")
        self.p("steward", uses="forge", borrows="release")
        tools = persona.effective_tools("steward")
        self.assertIn("twine", tools)
        self.assertNotIn("gh", tools)

    def test_borrows_none_borrows_nothing(self):
        """`vault: none` is the established spelling for 'deliberately nothing here'."""
        self.p("forge", tools="gh")
        self.p("steward", uses="forge", borrows="none", tools="jq")
        tools = persona.effective_tools("steward")
        self.assertEqual(tools, {"jq"})

    def test_its_own_tools_are_always_kept(self):
        self.p("forge", tools="gh")
        self.p("steward", uses="forge", borrows="none", tools="jq")
        self.assertIn("jq", persona.effective_tools("steward"))

    def test_it_is_inherited(self):
        self.p("release", tools="twine")
        self.p("base", borrows="release")
        self.p("child", extends="base")
        self.assertIn("twine", persona.effective_tools("child"))


class TestOnePersonaOptingInChangesNobodyElse(BorrowsIso):
    def test_a_sibling_keeps_its_legacy_grant(self):
        """The reason this is a per-persona field and not a plane-wide switch."""
        self.p("forge", tools="gh")
        self.p("strict", uses="forge", borrows="none")
        self.p("legacy", uses="forge")
        self.assertNotIn("gh", persona.effective_tools("strict"))
        self.assertIn("gh", persona.effective_tools("legacy"))


class TestItIsChecked(BorrowsIso):
    def test_borrows_is_a_known_key(self):
        self.p("forge", tools="gh")
        self.p("steward", borrows="forge")
        self.assertNotIn("borrows", " ".join(m for _lvl, m in persona.lint("steward")))

    def test_a_dangling_borrows_is_a_structural_error(self):
        """Same failure as a dangling `uses:`, which `lint` already had to grow a check
        for: a persona pointing at somebody who is not there."""
        self.p("steward", borrows="ghost")
        self.assertTrue(persona.structural_errors("steward"))

    def test_none_is_not_a_dangling_reference(self):
        self.p("steward", borrows="none")
        self.assertFalse(persona.structural_errors("steward"))


class TestTheGeneratedCharterSaysWhichIsWhich(BorrowsIso):
    def _charter(self, name: str) -> str:
        from charter import commands_persona, config
        commands_persona._write_agent(name)
        return (config.ROOT / ".claude" / "agents" / f"{name}.md").read_text()

    def test_with_borrows_uses_reads_as_routing_only(self):
        """The generated charter is what a dispatched agent believes about itself. If it
        still said 'run their tools' the toolgate would refuse and the agent would not know
        why."""
        self.p("forge", tools="gh")
        self.p("release", tools="twine")
        self.p("steward", uses="forge", borrows="release")
        text = self._charter("steward")
        self.assertIn("release", text)
        self.assertIn("forge", text)
        route_line = [ln for ln in text.splitlines() if "`forge`" in ln]
        self.assertTrue(route_line, "the routed persona must still be named")
        line = route_line[0]
        # This used to assert the word "vault" was ABSENT from the line, as a proxy for
        # "it does not grant vault access". The proxy stopped holding when the line
        # started saying the opposite in as many words — a prohibition mentioning the
        # vault reads identically to a grant when the test greps for the noun (#440). So
        # the assertion now says what it always meant: no grant, and an explicit refusal.
        self.assertNotIn("Read their vault", line)
        self.assertIn("not auto-approved for you", line)
        self.assertIn("not yours to open", line)

    def test_without_borrows_the_wording_is_unchanged(self):
        self.p("forge", tools="gh")
        self.p("steward", uses="forge")
        self.assertIn("vault", self._charter("steward"))


if __name__ == "__main__":
    unittest.main()
