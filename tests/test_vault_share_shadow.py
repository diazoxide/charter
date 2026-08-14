"""`--share` over an existing local registration, and the divergence it used to leave.

The two registry halves merge per FIELD, shared as the base with local layered over it
(`registry.load_registry`). So a vault registered with the bare form and then re-registered
with `--share --force` kept resolving through the OLD local entry: the shared half got the
new provider, item and persona, and every one of them was shadowed. Not a race — guaranteed
by the merge order, for any field the local copy defines.

`vault add` then printed a success line quoting the item name it read back from the merged
view, which is to say the stale one. Reported from the field: registration announced ✓, the
vault resolved to nothing, `vault verify` said "no references to verify", and `doctor`
stayed green because reachability is not resolution.

Every existing test in `test_vault_registration.py` starts from `share=True`, which is why
the sequence that bit — bare first, then shared — had no coverage at all.

ADR 0013: `--force` now replaces both halves, and a registry that disagrees with itself is
named rather than silently resolved.
"""

from __future__ import annotations

import unittest

from charter import doctor
from charter.secrets import registry
from tests._isolation import PersonaIso


class ShareOverLocalBase(PersonaIso):
    def _bare(self, **cfg):
        registry.add_vault("ops", "1password",
                           {"op-vault": "Engineering", "op-item": "SECRET", **cfg},
                           persona="ops")

    def _shared(self, **cfg):
        registry.add_vault("ops", "1password",
                           {"op-vault": "Engineering", "op-item": "charter-ops", **cfg},
                           persona="ops", force=True, share=True)


class TestShareClearsTheShadow(ShareOverLocalBase):
    def test_the_merged_view_resolves_to_the_new_item(self):
        """The whole bug in one assertion."""
        self._bare()
        self._shared()
        self.assertEqual(registry.vaults()["ops"]["config"]["op-item"], "charter-ops")

    def test_the_local_half_keeps_no_substantive_field(self):
        """Anything it kept would shadow the shared entry again on the next change."""
        self._bare()
        self._shared()
        local = registry.load_local().get("vaults", {}).get("ops", {})
        self.assertNotIn("op-item", local.get("config", {}))
        self.assertIsNone(local.get("provider"))

    def test_the_account_pin_survives(self):
        """It is the one field that legitimately belongs to this machine, so clearing the
        shadow must not take it — that would silently unpin the developer's account."""
        self._bare(account="me@corp.com")
        self._shared()
        self.assertEqual(registry.vaults()["ops"]["config"]["account"], "me@corp.com")
        self.assertNotIn("account", registry.load_shared()["vaults"]["ops"]["config"])

    def test_a_new_account_pin_still_lands_locally(self):
        self._bare()
        self._shared(account="me@corp.com")
        self.assertEqual(registry.load_local()["vaults"]["ops"]["config"]["account"],
                         "me@corp.com")
        self.assertNotIn("account", registry.load_shared()["vaults"]["ops"]["config"])

    def test_sharing_from_scratch_is_unchanged(self):
        self._shared()
        self.assertEqual(registry.vaults()["ops"]["config"]["op-item"], "charter-ops")
        self.assertEqual(registry.scope_of("ops"), "shared")

    def test_it_still_refuses_without_force(self):
        """Clearing the shadow is what `--force` now means; it is not a new licence."""
        self._bare()
        with self.assertRaises(Exception):
            registry.add_vault("ops", "plain-file", {}, persona="ops", share=True)


class TestDoctorNamesADisagreeingRegistry(ShareOverLocalBase):
    def _result(self):
        return doctor.check_vault_registry_divergence()

    def test_a_registry_that_disagrees_with_itself_fails(self):
        """FAIL, not WARN: doctor exits non-zero only on FAIL, and that exit code is the
        only thing that makes the SessionStart wrapper print (ADR 0013)."""
        registry.save_shared({"vaults": {"ops": {"provider": "1password", "persona": "ops",
                                                 "config": {"op-item": "charter-ops"}}}})
        registry.save_registry({"vaults": {"ops": {"provider": "1password", "persona": "ops",
                                                   "config": {"op-item": "SECRET"}}}})
        r = self._result()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("ops", r.detail + (r.hint or ""))

    def test_it_names_the_field_and_both_values(self):
        registry.save_shared({"vaults": {"ops": {"config": {"op-item": "charter-ops"}}}})
        registry.save_registry({"vaults": {"ops": {"config": {"op-item": "SECRET"}}}})
        text = " ".join(filter(None, [self._result().detail, self._result().hint]))
        for expected in ("op-item", "SECRET", "charter-ops"):
            self.assertIn(expected, text)

    def test_an_account_pin_is_not_a_divergence(self):
        """Layering a local account over a shared entry is the design, not a fault."""
        registry.save_shared({"vaults": {"ops": {"config": {"op-item": "charter-ops"}}}})
        registry.save_registry({"vaults": {"ops": {"config": {"account": "me@corp.com"}}}})
        self.assertEqual(self._result().status, doctor.OK)

    def test_a_vault_in_one_half_only_is_not_a_divergence(self):
        registry.save_shared({"vaults": {"team": {"config": {"op-item": "x"}}}})
        registry.save_registry({"vaults": {"solo": {"config": {"op-item": "y"}}}})
        self.assertEqual(self._result().status, doctor.OK)

    def test_agreeing_halves_are_fine(self):
        registry.save_shared({"vaults": {"ops": {"config": {"op-item": "same"}}}})
        registry.save_registry({"vaults": {"ops": {"config": {"op-item": "same"}}}})
        self.assertEqual(self._result().status, doctor.OK)

    def test_the_fix_leaves_nothing_for_the_check_to_find(self):
        """The two halves of this PR, meeting in the middle."""
        self._bare(account="me@corp.com")
        self._shared()
        self.assertEqual(self._result().status, doctor.OK)

    def test_the_check_runs_in_the_real_preflight(self):
        """A check nobody calls is a check nobody has."""
        self.assertIn("vault registry",
                      [r.name for r in doctor.run_all()])


if __name__ == "__main__":
    unittest.main()
