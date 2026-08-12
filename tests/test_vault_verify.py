"""`charter vault verify` — the check that actually resolves, and `doctor` no longer
claiming health it has not tested.

Reported as issue #55, from a live incident: `doctor` said `✓ vaults 6 configured, all
healthy` at the same moment every secret resolution through those vaults was failing. Both
statements were true. `health()` asks whether the vault is reachable and how many items it
holds; it deliberately never resolves, because `vault list` and `doctor` call it routinely
and a resolve would hit 1Password every time and could prompt for re-auth. That is a good
reason to skip it — and no reason at all to then call the result "healthy".

A reference can point at an item that does not exist while the vault holding it is
perfectly reachable. Nothing tested that until something failed at runtime, and the green
line sent people looking everywhere except at the reference. The reporter lost roughly 40
minutes to it during an incident.

Third instance of this class in this codebase: `commands_secrets` already carries two
comments saying "`doctor` reported 'all healthy', because from the vault's point of view it
was" — for a plaintext vault on a git-tracked path, and for an empty value stored by
accident.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets, doctor
from charter.secrets import base
from tests._isolation import PersonaIso


class _Ref:
    """A reference vault whose resolution outcome the test decides."""

    def __init__(self, name="v", refs=None, fails=()):
        self.name = name
        self._refs = refs or {"GOOD": "op://vault/good/password"}
        self._fails = set(fails)

    def keys(self):
        return sorted(self._refs)

    def reference_for(self, key):
        return self._refs[key]

    def health(self):
        return True, f"{len(self._refs)} reference(s)"

    def env_overlay(self):
        # `doctor` calls this before health() — a vault declaring an identity whose
        # variable is unset is broken in a way health() cannot see.
        return {}

    def get(self, key):
        if key in self._fails:
            raise base.VaultError(f"resolving '{key}' via op failed (exit 1)")
        return "s3cret"


class TestVerifyResolvesForReal(PersonaIso):
    def test_a_vault_whose_references_resolve_reports_ok(self):
        rows = commands_secrets.verify_vault(_Ref())
        self.assertEqual([r["ok"] for r in rows], [True])

    def test_a_reference_that_does_not_resolve_is_reported(self):
        """The whole point: reachable vault, dead reference."""
        v = _Ref(refs={"GOOD": "op://v/a/p", "DEAD": "op://v/gone/p"}, fails={"DEAD"})
        rows = commands_secrets.verify_vault(v)
        self.assertEqual({r["key"]: r["ok"] for r in rows}, {"GOOD": True, "DEAD": False})

    def test_the_failure_reason_is_carried_back(self):
        v = _Ref(fails={"GOOD"})
        self.assertIn("exit 1", commands_secrets.verify_vault(v)[0]["error"])

    def test_no_resolved_value_is_ever_returned(self):
        """A verify result travels to a terminal and possibly a log. It reports whether a
        reference resolves, never what it resolved to."""
        rows = commands_secrets.verify_vault(_Ref())
        self.assertNotIn("s3cret", repr(rows))

    def test_a_vault_that_cannot_list_keys_does_not_crash_the_verify(self):
        class Broken(_Ref):
            def keys(self):
                raise base.VaultError("registry unreadable")
        rows = commands_secrets.verify_vault(Broken())
        self.assertEqual([r["ok"] for r in rows], [False])


class TestTheCommand(PersonaIso):
    def _run(self, **kw):
        out, err = io.StringIO(), io.StringIO()
        kw.setdefault("name", None)
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands_secrets.cmd_vault_verify(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def test_a_failing_reference_makes_the_command_fail(self):
        """Exit status is what a CI step or a `&&` chain reads. Reporting a dead
        reference on stdout and exiting 0 is the same lie in a new place."""
        with mock.patch("charter.commands_secrets._vaults_to_verify",
                        return_value=[_Ref(fails={"GOOD"})]):
            rc, _ = self._run()
        self.assertNotEqual(rc, 0)

    def test_all_resolving_succeeds(self):
        with mock.patch("charter.commands_secrets._vaults_to_verify",
                        return_value=[_Ref()]):
            rc, _ = self._run()
        self.assertEqual(rc, 0)

    def test_it_names_the_key_that_failed(self):
        with mock.patch("charter.commands_secrets._vaults_to_verify",
                        return_value=[_Ref(refs={"A": "op://v/a/p", "DEAD": "op://v/x/p"},
                                           fails={"DEAD"})]):
            _, out = self._run()
        self.assertIn("DEAD", out)

    def test_no_vaults_is_not_a_failure(self):
        with mock.patch("charter.commands_secrets._vaults_to_verify", return_value=[]):
            rc, _ = self._run()
        self.assertEqual(rc, 0)


class TestDoctorStopsClaimingUntestedHealth(PersonaIso):
    """The reporter's own fallback suggestion, and the right default: if a resolution
    probe is too expensive for every preflight, say what was actually checked."""

    def _detail(self, vaults):
        with mock.patch("charter.secrets.registry.vaults", return_value=vaults), \
             mock.patch("charter.secrets.registry.provider_for",
                        side_effect=lambda n: _Ref(n)):
            return doctor.check_vaults()

    def test_it_does_not_say_healthy(self):
        r = self._detail({"a": {}, "b": {}})
        self.assertNotIn("healthy", f"{r.detail} {r.hint or ''}".lower())

    def test_it_says_what_it_actually_checked(self):
        r = self._detail({"a": {}})
        self.assertIn("reachable", f"{r.detail}".lower())

    def test_it_points_at_the_command_that_does_resolve(self):
        r = self._detail({"a": {}})
        self.assertIn("vault verify", f"{r.detail} {r.hint or ''}")

    def test_no_vaults_still_says_none_configured(self):
        r = self._detail({})
        self.assertIn("none", r.detail.lower())


if __name__ == "__main__":
    unittest.main()
