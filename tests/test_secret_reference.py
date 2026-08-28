"""Reference vault provider: entries are URIs into someone else's secret store.

The point is that storage stays in **one** place. For a team already running
HashiCorp Vault or 1Password, a charter vault would otherwise be a third copy of
a credential and a third thing to rotate; here the vault file holds
``op://Eng/deploy/token`` and the value is fetched at read time.

Every test stubs the resolver — nothing shells out to a real `op`/`vault`, so the
suite stays hermetic and runs with neither CLI installed.
"""

from __future__ import annotations

import json
import os
import stat
import subprocess
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from charter.secrets import reference
from charter.secrets.base import SecretNotFound, VaultError
from charter.secrets.reference import ReferenceProvider


def _proc(stdout="", rc=0):
    return subprocess.CompletedProcess(args=[], returncode=rc, stdout=stdout, stderr="")


class _Stub:
    """Records argv and returns a canned result — the resolver never really runs."""

    def __init__(self, stdout="s3cret", rc=0):
        self.stdout, self.rc, self.calls = stdout, rc, []

    def __call__(self, argv, check=True, **kw):
        self.calls.append(list(argv))
        return _proc(self.stdout, self.rc)


class ReferenceVault(unittest.TestCase):
    def setUp(self) -> None:
        self._td = TemporaryDirectory()
        self.f = Path(self._td.name) / "refs.json"
        self.addCleanup(self._td.cleanup)
        self.p = ReferenceProvider("team", {"file": str(self.f)})
        self.stub = _Stub()
        self.p.runner = self.stub
        # every test pretends both CLIs are installed unless it says otherwise
        self._which = reference.shutil.which
        reference.shutil.which = lambda c: f"/usr/local/bin/{c}"
        self.addCleanup(lambda: setattr(reference.shutil, "which", self._which))

    # --- storing references ------------------------------------------------ #
    def test_stores_a_1password_reference(self):
        self.p.set("TOKEN", "op://Eng/deploy/token")
        self.assertEqual(json.loads(self.f.read_text())["TOKEN"], "op://Eng/deploy/token")

    def test_stores_a_vault_reference(self):
        self.p.set("PW", "vault://secret/data/app#PASSWORD")
        self.assertEqual(self.p.reference_for("PW"), "vault://secret/data/app#PASSWORD")

    def test_refuses_a_bare_value(self):
        """The whole point: silently accepting a value would turn a reference vault
        into a plaintext one without saying so."""
        with self.assertRaises(VaultError) as cm:
            self.p.set("TOKEN", "hunter2")
        self.assertIn("stores a URI", str(cm.exception))
        self.assertNotIn("hunter2", str(cm.exception))

    def test_refuses_an_unsupported_scheme(self):
        with self.assertRaises(VaultError):
            self.p.set("TOKEN", "s3://bucket/key")

    def test_rejects_a_malformed_reference_at_write_time(self):
        """Validate on set, so a broken reference fails when you type it — not at
        3am when something tries to read it."""
        for bad in ("op://onlyvault", "vault://path-without-field"):
            with self.subTest(bad=bad):
                with self.assertRaises(VaultError):
                    self.p.set("K", bad)

    def test_reference_file_is_0600(self):
        self.p.set("TOKEN", "op://Eng/deploy/token")
        self.assertEqual(stat.S_IMODE(self.f.stat().st_mode), 0o600)

    # --- resolving --------------------------------------------------------- #
    def test_get_resolves_through_the_cli(self):
        self.p.set("TOKEN", "op://Eng/deploy/token")
        self.assertEqual(self.p.get("TOKEN"), "s3cret")
        self.assertEqual(self.stub.calls,
                         [["op", "read", "--no-newline", "op://Eng/deploy/token"]])

    def test_vault_reference_becomes_a_kv_get(self):
        self.p.set("PW", "vault://secret/data/app#PASSWORD")
        self.p.get("PW")
        self.assertEqual(self.stub.calls,
                         [["vault", "kv", "get", "-field=PASSWORD", "secret/data/app"]])

    def test_a_trailing_newline_from_the_resolver_is_stripped(self):
        """`vault kv get -field=` appends one; a secret with it would break auth."""
        self.p.runner = _Stub(stdout="s3cret\n")
        self.p.set("PW", "vault://secret/data/app#PASSWORD")
        self.assertEqual(self.p.get("PW"), "s3cret")

    def test_a_multiline_secret_keeps_its_internal_newlines(self):
        self.p.runner = _Stub(stdout="-----BEGIN KEY-----\nabc\n-----END KEY-----\n")
        self.p.set("PEM", "op://Eng/tls/key")
        self.assertEqual(self.p.get("PEM"),
                         "-----BEGIN KEY-----\nabc\n-----END KEY-----")

    def test_never_shells_out_as_a_string(self):
        """argv, never a shell string — a reference can never be command injection."""
        self.p.set("K", "op://Eng/item/field; rm -rf /")
        self.p.get("K")
        argv = self.stub.calls[0]
        self.assertIsInstance(argv, list)
        self.assertEqual(argv[-1], "op://Eng/item/field; rm -rf /")  # one opaque element

    # --- failure modes ----------------------------------------------------- #
    def test_missing_cli_is_an_actionable_error(self):
        reference.shutil.which = lambda c: None
        self.p.set("TOKEN", "op://Eng/deploy/token")
        with self.assertRaises(VaultError) as cm:
            self.p.get("TOKEN")
        msg = str(cm.exception)
        self.assertIn("not on PATH", msg)
        self.assertIn("op", msg)          # names the CLI to install
        self.assertIn("authenticate", msg)  # and that installing alone is not enough

    def test_resolver_failure_does_not_leak_its_output(self):
        """A resolver's stderr can echo what it fetched — report status, not output."""
        self.p.runner = _Stub(stdout="LEAKED-SECRET", rc=1)
        self.p.set("TOKEN", "op://Eng/deploy/token")
        with self.assertRaises(VaultError) as cm:
            self.p.get("TOKEN")
        self.assertNotIn("LEAKED-SECRET", str(cm.exception))
        self.assertIn("exit 1", str(cm.exception))

    def test_unknown_key_raises_secret_not_found(self):
        with self.assertRaises(SecretNotFound):
            self.p.get("NOPE")

    # --- listing / health -------------------------------------------------- #
    def test_keys_lists_names_only(self):
        self.p.set("A", "op://v/i/f")
        self.p.set("B", "vault://p#F")
        self.assertEqual(self.p.keys(), ["A", "B"])

    def test_health_never_resolves(self):
        """`vault list` and `doctor` call health() routinely — resolving there would
        hit 1Password on every listing and could prompt for re-auth."""
        self.p.set("A", "op://v/i/f")
        ok, detail = self.p.health()
        self.assertTrue(ok)
        self.assertEqual(self.stub.calls, [], "health() must not invoke a resolver")
        self.assertIn("1 reference", detail)

    def test_health_reports_a_missing_cli_without_failing_hard(self):
        self.p.set("A", "op://v/i/f")
        reference.shutil.which = lambda c: None
        ok, detail = self.p.health()
        self.assertFalse(ok)
        self.assertIn("op", detail)

    def test_health_on_an_empty_vault(self):
        """An empty vault FILE, which is a different fact from no file at all (#491).

        This case used to be asserted with no file on disk, and both states printed
        ``no references yet`` — so a vault registered against a mistyped ``--file`` read
        as one somebody had simply not filled in yet. The distinction is
        `tests/test_reference_vault_reports_its_directory.py`'s subject; what belongs here
        is that a vault that HAS a file and no entries still says so.
        """
        self.f.write_text("{}\n")
        ok, detail = self.p.health()
        self.assertTrue(ok)
        self.assertIn("no references", detail)

    def test_delete_removes_the_reference(self):
        self.p.set("A", "op://v/i/f")
        self.p.delete("A")
        self.assertEqual(self.p.keys(), [])
        with self.assertRaises(SecretNotFound):
            self.p.delete("A")

    # --- registry wiring --------------------------------------------------- #
    def test_provider_is_registered(self):
        from charter.secrets.registry import PROVIDERS
        self.assertIs(PROVIDERS.get("reference"), ReferenceProvider)


if __name__ == "__main__":
    unittest.main()
