"""A check that could not run is not a healthy check (#171).

The audit filed out of #168, whose shape is: **the absence of a protection renders as
health.** A reader scanning `doctor`'s glyph column sees ✓ and concludes the thing is fine;
what it actually meant was that there was nothing to look at, or that looking failed.

Two confirmed instances led here — #168's `✓ not running under the Claude Code plugin` over
a guard that never fires, and the empty-vault case before it — and reading every `OK` in
`doctor.py` turned up three more shapes.

The sharpest is that six checks returned **OK** with the detail ``not checked (<error>)``.
Two of them carried the comment *"a check that silently does nothing is worse than no
check"* directly above the line that returned OK: the principle was already written down
and the status contradicted it.

Deliberately NOT changed: `check_inventory`, which already distinguishes "not built, and
derivable anyway" (OK, with the reason) from "empty and not derivable" (WARN). It is the
model the rest of this audit was measured against.
"""

from __future__ import annotations

import unittest
from pathlib import Path

from charter import config, doctor
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN


class AuditCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True


class TestACheckThatCouldNotRunWarns(AuditCase):
    """Every site that reports `not checked` must say so in the STATUS, not only in prose
    a scanner never reads."""

    def _raising(self, module, attr):
        def boom(*a, **kw):
            raise OSError("unreadable")
        real = getattr(module, attr)
        setattr(module, attr, boom)
        self.addCleanup(setattr, module, attr, real)

    def test_workspace_clones(self):
        from charter import workspace
        self._raising(workspace, "list_workspaces")
        r = doctor.check_workspace_clones()
        self.assertEqual(r.status, WARN)
        self.assertIn("not checked", r.detail)

    def test_memory_indexes(self):
        from charter import workspace
        self._raising(workspace, "list_workspaces")
        self.assertEqual(doctor.check_memory_indexes().status, WARN)

    def test_personas(self):
        from charter import persona
        self._raising(persona, "list_personas")
        self.assertEqual(doctor.check_personas().status, WARN)

    def test_nested_plane(self):
        from charter import root as _root
        self._raising(_root, "enclosing_plane")
        self.assertEqual(doctor.check_nested_plane().status, WARN)

    def test_version_lock(self):
        from charter import instance
        self._raising(instance, "load")
        self.assertEqual(doctor.check_version_lock().status, WARN)

    def test_the_hint_says_the_silence_means_nothing(self):
        """The point of the row. Without this a reader treats a failed check the way they
        treat a passed one, which is the entire class."""
        from charter import persona
        self._raising(persona, "list_personas")
        self.assertIn("means nothing", doctor.check_personas().hint)

    def test_it_warns_rather_than_fails(self):
        """`cmd_doctor` exits non-zero only on FAIL, and that exit code drives the
        SessionStart preflight banner. An unreadable tree is not "you cannot work" — it is
        "charter cannot tell you either way"."""
        from charter import persona
        self._raising(persona, "list_personas")
        self.assertNotEqual(doctor.check_personas().status, doctor.FAIL)


class TestItDoesNotClaimAnAgreementItNeverChecked(AuditCase):
    def test_no_vaults_says_there_was_nothing_to_compare(self):
        """`check_plugin_skew` already records this exact failure in this file — it "must
        not claim agreement it hasn't checked". With no vaults registered, "shared and local
        halves agree" is vacuously true and reads as a verified result."""
        r = doctor.check_vault_registry_divergence()
        self.assertEqual(r.status, OK)
        self.assertIn("nothing to compare", r.detail)

    def test_registered_vaults_still_report_agreement(self):
        from charter.secrets import registry
        registry.add_vault("alpha", "plain-file", {"file": ".charter/vaults/alpha.json"})
        r = doctor.check_vault_registry_divergence()
        self.assertEqual(r.status, OK)
        self.assertIn("agree", r.detail)
        self.assertNotIn("nothing to compare", r.detail)


class TestAZeroCountSaysThereWasNothingToLookAt(AuditCase):
    def test_no_clones_anywhere(self):
        """"0 clone(s) across all workspaces, none behind" is technically true and reads as
        a clean sweep of something. There was nothing to sweep."""
        r = doctor.check_workspace_clones()
        self.assertEqual(r.status, OK)
        self.assertIn("nothing to check", r.detail)


class TestTheModelIsLeftAlone(AuditCase):
    def test_inventory_still_distinguishes_absent_from_broken(self):
        """`check_inventory` was already right and is deliberately unchanged: it separates
        "not built, and this plane's repo is clonable anyway" from "empty and not
        derivable". The audit measured everything else against it."""
        r = doctor.check_inventory()
        self.assertIn(r.status, (OK, WARN))
        self.assertTrue(r.detail)


if __name__ == "__main__":
    unittest.main()
