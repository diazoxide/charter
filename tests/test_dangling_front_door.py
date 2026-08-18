"""A declared front door that names no persona is said out loud.

`persona.declared_default` and `persona.default_persona` both validate their value against
what exists and return ``None`` when it does not — the right resolution (fail toward no
change: no identity beats a broken one), and until now the *entire* response. Rename or
delete a persona and the plane quietly has no front door: no role block in the briefing,
no memory digest, no routing, and nothing anywhere saying why.

Silence is exactly how `personas/.default` came to be shipped, tested, documented nowhere
and adopted by nobody (#255). The same silence must not now guard its replacement.

WARN, not FAIL: doctor's blockers list means "you cannot work", and a plane with no
persona still clones, still reaches the forge, still runs. Loud is the requirement; a
blocker is not.
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, doctor, persona, statusline
from tests._isolation import PersonaIso


class FrontDoorIso(PersonaIso):
    def declare(self, name: str) -> None:
        (config.ROOT / "charter.toml").write_text(
            f'schema = 1\n\n[persona]\ndefault = "{name}"\n')

    def real(self, name: str) -> None:
        self.make_persona(name, role=name.title(), vault="none")


class TestDoctorNamesIt(FrontDoorIso):
    def test_a_declaration_naming_no_persona_warns(self):
        self.declare("ghost")
        r = doctor.check_front_door()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("ghost", r.detail)

    def test_the_hint_says_how_to_fix_it(self):
        self.declare("ghost")
        r = doctor.check_front_door()
        self.assertIn("charter persona default", r.hint or "")

    def test_a_declaration_that_resolves_is_ok(self):
        self.real("steward")
        self.declare("steward")
        r = doctor.check_front_door()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("steward", r.detail)

    def test_no_declaration_at_all_is_not_a_problem(self):
        """A plane may legitimately have no front door — charter never invents one, and a
        check that nags about an intended state teaches people to skim past doctor."""
        self.real("steward")
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        r = doctor.check_front_door()
        self.assertEqual(r.status, doctor.OK)

    def test_a_dangling_legacy_dotfile_warns_too(self):
        """Same failure, older file — it resolves whenever charter.toml is silent."""
        (config.PERSONAS_DIR / ".default").write_text("ghost\n")
        r = doctor.check_front_door()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("ghost", r.detail)

    def test_it_runs_in_the_doctor_sweep(self):
        """A check registered nowhere reports to nobody — the very defect this closes."""
        names = [r.name for r in doctor.run_all()]
        self.assertIn("front door", names)


class TestTheStatusLineSaysIt(FrontDoorIso):
    def _alerts(self) -> str:
        return "\n".join(statusline._alerts("default"))

    def test_a_dangling_declaration_gets_a_row(self):
        self.declare("ghost")
        out = self._alerts()
        self.assertIn("ghost", out)

    def test_a_resolving_declaration_gets_no_row(self):
        """Silence is the design: a row that renders every turn is furniture within a day,
        and then a real one draws no more attention than a zero would."""
        self.real("steward")
        self.declare("steward")
        self.assertNotIn("steward", self._alerts())

    def test_no_declaration_gets_no_row(self):
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.assertEqual(self._alerts().strip(), "")


if __name__ == "__main__":
    unittest.main()
