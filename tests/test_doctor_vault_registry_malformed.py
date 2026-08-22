"""`doctor`'s registry check, against a `vaults.json` entry that is not an object (#363).

#346 taught `load_registry` to drop such an entry and `check_vaults` to name what it
dropped (#347). `check_vault_registry_divergence` was not part of that change and still
read the two halves raw, so it

* **crashed** — a malformed entry is a *string*, a non-empty string is truthy, so
  ``shared[name] or {}`` handed a `str` to ``.get`` — taking the entire preflight with it,
  because `_checks()` is eager and `cmd_doctor` has no per-check guard; and
* **counted** an entry `load_registry` had already dropped, one line below the check that
  said it was ignored.

Every test here **proves the drop happened first**. A test that asserted a count without
establishing that the entry was malformed *and* dropped would pass against a fixture that
was never malformed at all — which is the vacuous pass this module exists to avoid.
"""
from __future__ import annotations

import unittest
from unittest import mock

from charter import doctor
from charter.secrets import registry
from tests._isolation import PersonaIso

#: Not an object. The shape #347 found in the field.
MALFORMED = "op://Team/devops"


class MalformedEntryCase(PersonaIso):
    def assert_dropped(self, name: str) -> None:
        """Precondition: *name*'s committed entry really is malformed, and really was
        dropped from the merged view every reader gets.

        Deliberately **not** ``assertNotIn(name, registry.vaults())``. When the same name
        is also declared in the local half — which is the shape that reached the crash —
        the name survives into the merged view carrying the *local* entry, and the shared
        string is what was dropped. Asserting the name's absence looked right and was
        wrong for exactly the case this module is about; what has to be true is that the
        malformed VALUE never reaches a reader.
        """
        raw = registry.load_shared().get("vaults", {})
        self.assertIn(name, raw, f"precondition: {name} is not in the shared half at all")
        self.assertNotIsInstance(
            raw[name], dict,
            f"precondition: {name} is a well-formed entry, so nothing would drop it")
        self.assertIn(name, registry.malformed_shared(),
                      f"precondition: {name} is not reported as malformed, so nothing "
                      f"dropped it and this test would assert against an intact registry")
        self.assertNotEqual(registry.vaults().get(name), raw[name],
                            f"precondition: the malformed value for {name} survived into "
                            f"the merged view")

    def assert_absent_from_merged(self, name: str) -> None:
        """The stronger form, for a name the local half does not rescue."""
        self.assert_dropped(name)
        self.assertNotIn(name, registry.vaults())


class TestItNoLongerCrashes(MalformedEntryCase):
    def test_a_malformed_shared_entry_also_named_locally_does_not_raise(self):
        """The crash, exactly: the name has to be in BOTH halves to reach the `.get`, and
        a shared vault with a local `account` pin is the ordinary shape of that."""
        registry.save_shared({"vaults": {"devops": MALFORMED}})
        registry.save_registry({"vaults": {"devops": {"config": {"account": "me@corp"}}}})
        self.assert_dropped("devops")
        r = doctor.check_vault_registry_divergence()          # must not raise
        self.assertEqual(r.status, doctor.OK)

    def test_the_rest_of_the_preflight_still_runs(self):
        """What the crash actually cost: `_checks()` is eager, so one raising check ended
        the command before a single line was printed — including the twelve that passed."""
        registry.save_shared({"vaults": {"devops": MALFORMED}})
        registry.save_registry({"vaults": {"devops": {"config": {"account": "me@corp"}}}})
        results = doctor.run_all()
        self.assertGreater(len(results), 12)
        self.assertIn("vault registry", [r.name for r in results])

    def test_a_malformed_entry_in_the_local_half_does_not_raise_either(self):
        registry.save_shared({"vaults": {"devops": {"config": {"op-item": "x"}}}})
        registry.save_registry({"vaults": {"devops": MALFORMED}})
        self.assertEqual(doctor.check_vault_registry_divergence().status, doctor.OK)

    def test_a_malformed_entry_in_both_halves_does_not_raise(self):
        registry.save_shared({"vaults": {"devops": MALFORMED}})
        registry.save_registry({"vaults": {"devops": MALFORMED}})
        self.assertEqual(doctor.check_vault_registry_divergence().status, doctor.OK)


class TestTheCountMatchesWhatWasKept(MalformedEntryCase):
    def test_a_dropped_entry_is_not_counted(self):
        registry.save_shared({"vaults": {"devops": MALFORMED,
                                         "media": {"config": {"op-item": "m"}}}})
        self.assert_absent_from_merged("devops")
        detail = doctor.check_vault_registry_divergence().detail
        self.assertIn("1", detail)
        self.assertNotIn("2", detail)

    def test_one_vault_declared_in_both_halves_counts_once(self):
        """`len(shared) + len(local)` reported "2 entries" for one vault — and "entries"
        was the word doing the hiding, because a reader of a vault registry counts vaults."""
        registry.save_shared({"vaults": {"ops": {"config": {"op-item": "same"}}}})
        registry.save_registry({"vaults": {"ops": {"config": {"op-item": "same"}}}})
        detail = doctor.check_vault_registry_divergence().detail
        self.assertIn("1", detail)
        self.assertNotIn("2", detail)

    def test_the_count_says_vaults(self):
        registry.save_shared({"vaults": {"a": {}, "b": {}}})
        detail = doctor.check_vault_registry_divergence().detail
        self.assertIn("2 vaults", detail)

    def test_one_vault_is_singular(self):
        registry.save_shared({"vaults": {"a": {}}})
        self.assertIn("1 vault", doctor.check_vault_registry_divergence().detail)

    def test_distinct_vaults_across_the_halves_are_counted_once_each(self):
        registry.save_shared({"vaults": {"team": {"config": {"op-item": "x"}}}})
        registry.save_registry({"vaults": {"solo": {"config": {"op-item": "y"}}}})
        self.assertIn("2 vaults", doctor.check_vault_registry_divergence().detail)

    def test_no_vaults_at_all_still_says_nothing_to_compare(self):
        """The existing wording, which must survive: claiming the halves agree when
        neither holds anything is an agreement nothing was compared to reach."""
        self.assertIn("nothing to compare",
                      doctor.check_vault_registry_divergence().detail)

    def test_a_registry_that_holds_only_a_malformed_entry_has_nothing_to_compare(self):
        registry.save_shared({"vaults": {"devops": MALFORMED}})
        self.assert_absent_from_merged("devops")
        self.assertIn("nothing to compare",
                      doctor.check_vault_registry_divergence().detail)


class TestTheTwoVaultLinesNoLongerContradict(MalformedEntryCase):
    """#347's actual complaint: one line says the entry was ignored, the next counts it.

    `check_vaults` reaches its notes only on the OK path, which needs a reachable vault —
    so the provider is stubbed. Nothing here touches a real 1Password.
    """

    def _healthy(self):
        prov = mock.Mock()
        prov.env_overlay.return_value = None
        prov.health.return_value = (True, 1)
        return mock.patch.object(registry, "provider_for", return_value=prov)

    def test_ignored_and_counted_agree(self):
        registry.save_shared({"vaults": {"devops": MALFORMED,
                                         "media": {"provider": "op", "persona": "ed"}}})
        self.assert_absent_from_merged("devops")
        with self._healthy():
            vaults_line = doctor.check_vaults()
        registry_line = doctor.check_vault_registry_divergence()

        self.assertIn("devops", vaults_line.detail)
        self.assertIn("ignored", vaults_line.detail)
        # The contradiction: the line above ignored one of two, so the line below must not
        # report two.
        self.assertIn("1 vault", registry_line.detail)
        self.assertNotIn("2", registry_line.detail)


class TestDivergenceIsStillDetected(MalformedEntryCase):
    """The check must not be made quiet by the fix — a malformed entry beside a real
    disagreement still has to report the disagreement."""

    def test_a_real_clash_is_still_found_next_to_a_malformed_entry(self):
        registry.save_shared({"vaults": {"junk": MALFORMED,
                                         "ops": {"config": {"op-item": "charter-ops"}}}})
        registry.save_registry({"vaults": {"ops": {"config": {"op-item": "SECRET"}}}})
        self.assert_absent_from_merged("junk")
        r = doctor.check_vault_registry_divergence()
        self.assertEqual(r.status, doctor.FAIL)
        for expected in ("op-item", "SECRET", "charter-ops"):
            self.assertIn(expected, r.detail)


if __name__ == "__main__":
    unittest.main()
