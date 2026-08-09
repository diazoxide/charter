"""Binding the identity a vault is read through (issue #13).

`op` authenticates from the single global `OP_SERVICE_ACCOUNT_TOKEN`, but least-privilege
setups issue one service account per scope, so tokens are named per persona. Without a
binding the mapping lives in every caller's shell —
`OP_SERVICE_ACCOUNT_TOKEN="$OP_ACME_DEVOPS_TOKEN" charter secret exec devops -- …` —
which is the property charter's vault abstraction otherwise removes. Worse, using the
wrong one is *silently* wrong: 1Password answers with "no items" or a permission error,
so it reads as an empty or broken vault rather than as the wrong credential.

Stored as a mapping of NAMES, `{TARGET: SOURCE}` — the variable the CLI reads, and the one
this machine carries it in. Never a value, so the registry stays inert if it leaks and the
binding is reviewable in git.
"""
from __future__ import annotations

import io
import os
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets
from charter.secrets import base, registry
from tests._isolation import PersonaIso

_SRC = "OP_ACME_DEVOPS_TOKEN"
_TARGET = "OP_SERVICE_ACCOUNT_TOKEN"


class _Rec:
    """Stands in for `util.run`, recording argv and the env overlay it was handed."""

    def __init__(self, rc=0, stdout="value"):
        self.rc, self.stdout, self.calls = rc, stdout, []

    def __call__(self, argv, check=True, env=None, input=None, **kw):
        self.calls.append({"argv": list(argv), "env": env})
        return SimpleNamespace(returncode=self.rc, stdout=self.stdout, stderr="")


def _args(name, provider="reference", **kw):
    return SimpleNamespace(name=name, provider=provider, file=kw.pop("file", None),
                           op_vault=kw.pop("op_vault", None), account=None, persona=None,
                           force=kw.pop("force", False), share=kw.pop("share", False),
                           env=kw.pop("env", []), token_env=kw.pop("token_env", None))


class RecordingTheBinding(PersonaIso):
    def _add(self, **kw):
        with redirect_stderr(io.StringIO()):
            return commands_secrets.cmd_vault_add(_args("devops", **kw))

    def test_token_env_is_sugar_for_the_1password_variable(self):
        self.assertEqual(self._add(token_env=_SRC), 0)
        self.assertEqual(registry.vaults()["devops"]["config"]["env"], {_TARGET: _SRC})

    def test_the_general_form_names_both_variables(self):
        """Reference vaults already resolve `op://` AND `vault://`, so a binding that
        could only mean "the 1Password token" would be wrong on arrival for half of what
        charter ships."""
        self.assertEqual(self._add(env=["VAULT_TOKEN=ACME_VAULT_TOKEN"]), 0)
        self.assertEqual(registry.vaults()["devops"]["config"]["env"],
                         {"VAULT_TOKEN": "ACME_VAULT_TOKEN"})

    def test_only_names_are_stored_never_a_value(self):
        with mock.patch.dict(os.environ, {_SRC: "ops_supersecret"}):
            self._add(token_env=_SRC)
        self.assertNotIn("supersecret", str(registry.vaults()["devops"]))

    def test_a_malformed_binding_is_refused(self):
        with redirect_stderr(io.StringIO()):
            self.assertEqual(commands_secrets.cmd_vault_add(_args("d", env=["NOEQUALS"])), 1)
        self.assertNotIn("d", registry.vaults())

    def test_an_invalid_variable_name_is_refused_at_registration(self):
        """The failure it prevents surfaces much later and in disguise: a typo'd source is
        simply unset, and unset is (deliberately) a hard error about a credential."""
        with redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args("d", env=["OK=has-a-dash"]))
        self.assertEqual(rc, 1)

    def test_the_binding_travels_with_the_shared_half(self):
        """A variable NAME is a team convention; only its value is private."""
        self._add(token_env=_SRC, share=True)
        self.assertEqual(
            registry.load_shared()["vaults"]["devops"]["config"]["env"], {_TARGET: _SRC})


class UsingTheBinding(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # `reference` refuses to resolve when the CLI is absent from PATH, so without this
        # these tests pass on a laptop with `op` installed and fail on a bare CI runner —
        # which is exactly what happened. Same stub `test_secret_reference.py` uses.
        from charter.secrets import reference
        self._which = reference.shutil.which
        reference.shutil.which = lambda c: f"/usr/local/bin/{c}"
        self.addCleanup(lambda: setattr(reference.shutil, "which", self._which))

        (self.tmp / "d.json").write_text('{"API": "op://Eng/api/token"}')
        registry.add_vault("devops", "reference",
                           {"file": "d.json", "env": {_TARGET: _SRC}})
        self.prov = registry.provider_for("devops")

    def test_the_credential_reaches_the_child_environment(self):
        rec = _Rec()
        self.prov.runner = rec
        with mock.patch.dict(os.environ, {_SRC: "ops_supersecret"}):
            self.prov.get("API")
        self.assertEqual(rec.calls[0]["env"][_TARGET], "ops_supersecret")

    def test_it_never_reaches_argv(self):
        """The module's standing rule — `ps` and shell history can read argv."""
        rec = _Rec()
        self.prov.runner = rec
        with mock.patch.dict(os.environ, {_SRC: "ops_supersecret"}):
            self.prov.get("API")
        self.assertFalse([a for a in rec.calls[0]["argv"] if "supersecret" in a])

    def test_charter_does_not_set_it_on_itself(self):
        """A mutated `os.environ` would outlive the call and apply to the NEXT vault —
        the identity confusion this feature exists to prevent."""
        rec = _Rec()
        self.prov.runner = rec
        with mock.patch.dict(os.environ, {_SRC: "ops_supersecret"}):
            self.prov.get("API")
            self.assertNotIn(_TARGET, os.environ)

    def test_an_unset_source_is_a_hard_error_not_a_fallback(self):
        """The whole point. Falling back to an ambient token reads the vault under an
        identity the plane never declared, and 1Password reports that as "no items"."""
        with mock.patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(base.VaultError) as e:
                self.prov.env_overlay()
        msg = str(e.exception)
        self.assertIn(_SRC, msg)
        self.assertIn("will not fall back", msg)

    def test_an_empty_source_counts_as_unset(self):
        with mock.patch.dict(os.environ, {_SRC: ""}):
            with self.assertRaises(base.VaultError):
                self.prov.env_overlay()

    def test_a_vault_declaring_nothing_is_untouched(self):
        """Single-account setups see no change — no overlay, no new failure mode."""
        registry.add_vault("plain", "reference", {"file": "d.json"})
        self.assertEqual(registry.provider_for("plain").env_overlay(), {})

    def test_a_failed_read_names_the_identity_in_play(self):
        """Turns a silent wrong answer into a pointed question, without asking `op` who it
        is on every read."""
        self.prov.runner = _Rec(rc=1)
        with mock.patch.dict(os.environ, {_SRC: "tok"}):
            with self.assertRaises(base.VaultError) as e:
                self.prov.get("API")
        self.assertIn(_SRC, str(e.exception))


class DoctorSeparatesTheTwoFailures(PersonaIso):
    def test_a_missing_identity_is_not_reported_as_an_unhealthy_vault(self):
        """The fix is `export`, not anything about the vault — and `health()` cannot see
        it, because `op` reports it as an empty or misconfigured vault."""
        from charter import doctor
        (self.tmp / "d.json").write_text('{"API": "op://Eng/api/token"}')
        registry.add_vault("devops", "reference",
                           {"file": "d.json", "env": {_TARGET: _SRC}})
        with mock.patch.dict(os.environ, {}, clear=True):
            res = doctor.check_vaults()
        self.assertEqual(res.status, doctor.WARN)
        self.assertIn("identity variable unset", res.hint)
        self.assertIn(_SRC, res.hint)


if __name__ == "__main__":
    unittest.main()
