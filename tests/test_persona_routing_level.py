"""A persona declares its own outbound routing posture: `routing: off | advise | require`.

`delegate-when` is inbound — an advert other agents read when choosing. Nothing has ever
expressed the other direction: when the persona *currently acting* should hand work away.
That gap is what the dispatch tally measures (#256).

The posture is per-persona and nowhere else. There is deliberately no `[routing]` section
in `charter.toml`: the level is only ever read from the ONE acting persona in a session, so
a plane-wide floor would reach personas that never asked for it. A fresh plane gets a
sensible default because the generated front-door template carries `routing: advise` in its
own frontmatter — config in a file the consumer owns, not a constant in charter.
"""
from __future__ import annotations

import unittest

from charter import config, persona
from tests._isolation import PersonaIso


class RoutingIso(PersonaIso):
    def p(self, name, **meta):
        return self.make_persona(name, role=name.title(), vault="none",
                                 **{"delegate-when": f"{name} work", **meta})


class TestTheLevel(RoutingIso):
    def test_absent_is_off(self):
        """Absence is the default, so upgrading a plane changes nothing until someone
        opts in — and the default that new planes get comes from a template they own."""
        self.p("steward")
        self.assertEqual(persona.routing_level("steward"), "off")

    def test_a_declared_level_is_read(self):
        self.p("steward", routing="advise")
        self.assertEqual(persona.routing_level("steward"), "advise")

    def test_require_is_a_level(self):
        self.p("steward", routing="require")
        self.assertEqual(persona.routing_level("steward"), "require")

    def test_an_unknown_level_falls_back_to_off(self):
        """Fail toward no change: a typo must not silently enable a gate, and must not
        crash the hook that reads it on every prompt."""
        self.p("steward", routing="loud")
        self.assertEqual(persona.routing_level("steward"), "off")

    def test_case_and_padding_do_not_matter(self):
        self.p("steward", routing="  Advise ")
        self.assertEqual(persona.routing_level("steward"), "advise")

    def test_it_is_inherited(self):
        """Frontmatter is inheritance-merged everywhere else; a posture that ignored
        `extends:` would be the one field a child could not receive."""
        self.p("base", routing="advise")
        self.p("child", extends="base")
        self.assertEqual(persona.routing_level("child"), "advise")

    def test_a_child_overrides_its_parent(self):
        self.p("base", routing="require")
        self.p("child", extends="base", routing="off")
        self.assertEqual(persona.routing_level("child"), "off")

    def test_an_unknown_persona_is_off_not_an_error(self):
        self.assertEqual(persona.routing_level("ghost"), "off")


class TestItIsAKnownKey(RoutingIso):
    def test_lint_does_not_call_routing_an_unknown_key(self):
        """`persona lint` flags unknown frontmatter, which is how a silently inert typo
        gets caught — so a real key missing from that vocabulary is reported as a mistake."""
        self.p("steward", routing="advise")
        issues = persona.lint("steward")
        self.assertNotIn("routing", " ".join(m for _lvl, m in issues))

    def test_routes_to_is_a_known_key_too(self):
        self.p("steward", **{"routes-to": "forge"})
        issues = persona.lint("steward")
        self.assertNotIn("routes-to", " ".join(m for _lvl, m in issues))


class TestRoutesTo(RoutingIso):
    def test_absent_is_empty(self):
        self.p("steward")
        self.assertEqual(persona.routes_to("steward"), [])

    def test_it_reads_a_list(self):
        self.p("steward", **{"routes-to": "forge, release"})
        self.assertEqual(persona.routes_to("steward"), ["forge", "release"])


class TestTheRoster(RoutingIso):
    def test_it_excludes_the_acting_persona(self):
        """Telling the front door it could route to itself is noise in the one block that
        cannot afford any."""
        self.p("steward")
        self.p("forge")
        names = [r["name"] for r in persona.roster_for("steward")]
        self.assertEqual(names, ["forge"])

    def test_it_carries_the_advert_each_persona_wrote(self):
        self.p("steward")
        self.p("forge", **{"delegate-when": "GitHub APIs, CI state"})
        row = persona.roster_for("steward")[0]
        self.assertEqual(row["delegate_when"], "GitHub APIs, CI state")

    def test_routes_to_comes_first_but_hides_nobody(self):
        """Priority, never restriction — a persona created after `routes-to:` was written
        must not become invisible to routing forever, with nothing reporting it."""
        self.p("steward", **{"routes-to": "release"})
        self.p("forge")
        self.p("release")
        names = [r["name"] for r in persona.roster_for("steward")]
        self.assertEqual(names[0], "release")
        self.assertIn("forge", names)

    def test_a_draft_persona_is_left_out(self):
        """charter generates no sub-agent for a draft, so it cannot be dispatched —
        offering it as a destination would advertise a route that does not exist."""
        self.p("steward")
        self.p("halfbaked", draft="true")
        self.assertEqual([r["name"] for r in persona.roster_for("steward")], [])

    def test_it_reports_when_each_was_last_dispatched(self):
        """The date is what makes the block evidence rather than advice: a persona that
        has never been dispatched is the whole finding."""
        self.p("steward")
        self.p("forge")
        row = persona.roster_for("steward")[0]
        self.assertIn("last_dispatched", row)
        self.assertIsNone(row["last_dispatched"])

    def test_an_empty_roster_is_empty_not_an_error(self):
        self.p("steward")
        self.assertEqual(persona.roster_for("steward"), [])


if __name__ == "__main__":
    unittest.main()
