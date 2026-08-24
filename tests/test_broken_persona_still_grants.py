"""A broken persona that still auto-approves tools must SAY so (#343, #329).

`toolgate.decide` asks the active persona what its tools are and never asks whether the
persona is well-formed. `charter persona lint` calls it broken; the PreToolUse gate honours
it anyway. Two checks read the same file, answer different questions about it, and only one
of them sits on the path that removes a prompt.

**#343's direction (1) — have the gate ignore such a persona — is deliberately NOT what
this file holds, and the reason is measured rather than assumed.** After #342 that gate
buys no containment at all. `verify` runs below in three shapes (a `uses:` that is a path
out of the plane, a `uses:` that is a plain typo, an `extends:` cycle) and in every one the
surviving grant is exactly the persona's own `tools:` plus what a reader of those same
files would compute. The only tool reachable *through* a broken reference is refused
already, because `load()` returns None for a reference that is not a name and contributes
nothing. So gating on `structural_errors` would revoke a grant the operator wrote by hand,
in exchange for closing nothing — and it would revoke it silently, because the gate's
production output path (`hooks.pretooluse`) emits nothing at all when `decide` declines.

What was actually missing is the sentence. `doctor` reported "4 with error(s)" about the
roster and, separately, nothing about the fact that one of those broken personas was the
*active* one and was still pre-approving binaries. `check_ask_rules` is the precedent and
the mirror image — *"the persona's tools quietly stop being pre-approved, with nothing
naming the cause"* — pointed the other way: here they quietly keep working.
"""

from __future__ import annotations

import unittest

from charter import config, doctor, persona, toolgate
from tests._isolation import PersonaIso


class BrokenPersonaStillGrants(PersonaIso):

    def broken(self, name: str, **meta) -> str:
        """A persona with a real structural error and a real tool grant."""
        self.make_persona(name, role="front door", **{"delegate-when": "everything"},
                          **meta)
        self.assertTrue(persona.structural_errors(name),
                        f"precondition: '{name}' must actually be broken")
        return name

    def activate(self, name: str) -> None:
        (config.PERSONAS_DIR / ".default").write_text(name + "\n")
        self.assertEqual(name, persona.resolve_active(),
                         "precondition: the broken persona must be the ACTIVE one")
        # What SessionStart does (#432): the gate answers within the tools declared before
        # the session began, so a persona this test *invents* mid-run has to be part of
        # that roster or the gate correctly grants it nothing. Nothing asserted below
        # changes — this is the fixture catching up with a session boundary that exists in
        # production and not in a single test process.
        toolgate.snapshot()

    # ------------------------------------------------------- the measured claim
    def test_a_broken_reference_contributes_no_tools(self):
        """The evidence #343 is closed on. Three shapes of breakage; in each the gate
        grants the persona's OWN tools and nothing the broken reference names.

        `curl` is in every case for the same reason a benign half is: a gate that allowed
        everything would pass every other assertion here.
        """
        self.make_persona("lender", tools="sh")
        for label, meta in (
            ("uses: a path out of the plane", {"uses": "../../elsewhere/lender"}),
            ("uses: a plain typo", {"uses": "lendr"}),
            ("extends: a dangling parent", {"extends": "nobody"}),
        ):
            with self.subTest(shape=label):
                name = f"front-{len(label)}"
                self.broken(name, tools="gh", **meta)
                tools = persona.effective_tools(name)
                self.assertIn("gh", tools, "its own grant must survive")
                self.assertNotIn("sh", tools,
                                 "a broken reference must contribute nothing")
                self.activate(name)
                self.assertIsNotNone(toolgate.decide("gh --version"))
                self.assertIsNone(toolgate.decide("curl example.com"),
                                  "the gate is not simply allowing everything")

    def test_an_inheritance_cycle_grants_no_more_than_the_files_read(self):
        """The one shape where `lineage` truncates rather than refusing, so it is the
        one that could plausibly grant something unexpected. It does not: `a extends b`
        and `b extends a` gives each the other's tools, which is what the two files say.
        """
        self.make_persona("cyc-a", tools="gh", extends="cyc-b")
        self.make_persona("cyc-b", tools="kubectl", extends="cyc-a")
        self.assertTrue(any("cycle" in m for _l, m in persona.structural_errors("cyc-a")),
                        "precondition: this must be reported as a cycle")
        self.assertEqual({"gh", "kubectl"}, persona.effective_tools("cyc-a"))
        self.activate("cyc-a")
        self.assertIsNone(toolgate.decide("curl example.com"))

    # ------------------------------------------------------------ the legibility
    def test_doctor_says_the_active_persona_is_broken_and_still_granting(self):
        """The divergence, closed where it can be: not by revoking the grant, but by
        refusing to leave the two answers unconnected.

        `check_personas` already reports "N with error(s)" about the roster. It does not
        say that one of them is the ACTIVE persona and is still pre-approving binaries,
        which is the half that decides whether a prompt appears.
        """
        self.broken("front", tools="gh, kubectl", uses="lendr")
        self.activate("front")

        r = doctor.check_persona_grant()
        self.assertNotEqual(doctor.OK, r.status,
                            "doctor left a broken persona granting silently")
        said = f"{r.detail} {r.hint or ''}"
        self.assertIn("front", said, "it must name the persona")
        self.assertIn("gh", said, "it must name what is still auto-approved")
        self.assertIn("lint", said, "it must point at the command that explains the break")

    def test_doctor_is_quiet_when_the_active_persona_is_well_formed(self):
        """The half that catches a check which warns about every plane. A persona with a
        `tools:` grant and no structural errors is the *designed* configuration — #329
        settled that the grant itself stays — and must not be reported as a problem."""
        self.make_persona("clean", role="front door", tools="gh",
                          **{"delegate-when": "everything"})
        self.assertEqual([], persona.structural_errors("clean"), "precondition")
        self.activate("clean")
        self.assertEqual(doctor.OK, doctor.check_persona_grant().status)

    def test_doctor_is_quiet_when_a_broken_persona_grants_nothing(self):
        """Breakage alone is `check_personas`' business. This check earns its line only
        when a broken persona is also removing prompts — otherwise it is a second, louder
        copy of a report that already exists."""
        self.broken("front", uses="lendr")           # no tools: at all
        self.activate("front")
        self.assertEqual(set(), persona.effective_tools("front"), "precondition")
        self.assertEqual(doctor.OK, doctor.check_persona_grant().status)

    def test_doctor_is_quiet_with_no_active_persona(self):
        (config.PERSONAS_DIR / ".default").unlink(missing_ok=True)
        self.assertIsNone(persona.resolve_active(), "precondition")
        self.assertEqual(doctor.OK, doctor.check_persona_grant().status)

    def test_the_check_is_wired_into_the_preflight(self):
        """A check nobody runs is the shape this repo has paid for twice (#177, #197),
        and `check_personas`' own docstring says so: it *"was in no hook and in no other
        command, so it reported drift only to someone who already suspected drift"*."""
        self.broken("front", tools="gh", uses="lendr")
        self.activate("front")
        names = [r.name for r in doctor.run_all()]
        self.assertIn(doctor.check_persona_grant().name, names)


if __name__ == "__main__":
    unittest.main()
