"""Registering a vault over a name that already exists (issue #22).

`charter vault add <name>` used to overwrite the registration in place — different
provider, no prompt, exit 0. The registration is the ONLY pointer to a plain-file vault's
secrets, so replacing it migrates nothing: it strands the file on disk with nothing
referring to it, and `charter secret get` then reports the key as *missing* rather than as
unreachable. Observed during a real migration, where three vaults were re-registered onto
1Password and every one was accepted silently.

It also broke the rule the rest of charter states and follows — additive: never delete or
rename a user's thing to make room; name the blocker and refuse. `init`, `reinit` and
`_create_baseline_dirs` all work that way.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr
from types import SimpleNamespace

from charter import commands_secrets
from charter.secrets import base, registry
from tests._isolation import PersonaIso


def _args(name: str, provider: str = "plain-file", **kw) -> SimpleNamespace:
    return SimpleNamespace(name=name, provider=provider, file=kw.pop("file", None),
                           op_vault=kw.pop("op_vault", None), account=None,
                           persona=kw.pop("persona", None), force=kw.pop("force", False))


class RegisteringOverAnExistingName(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        registry.add_vault("devops", "plain-file", {"file": str(self.tmp / "devops.json")})

    def test_a_collision_is_refused(self):
        with self.assertRaises(base.VaultError):
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})

    def test_the_original_registration_survives_the_refusal(self):
        """The point of refusing: what was there must still be there afterwards."""
        with self.assertRaises(base.VaultError):
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})
        got = registry.vaults()["devops"]
        self.assertEqual(got["provider"], "plain-file")
        self.assertEqual(got["config"]["file"], str(self.tmp / "devops.json"))

    def test_the_refusal_names_what_is_in_the_way_and_how_to_proceed(self):
        """A blocker the user cannot act on is only half a refusal."""
        with self.assertRaises(base.VaultError) as e:
            registry.add_vault("devops", "1password", {"op-vault": "Eng"})
        msg = str(e.exception)
        self.assertIn("plain-file", msg)              # what is registered
        self.assertIn("devops.json", msg)             # and where its secrets are
        self.assertIn("--force", msg)                 # how to override

    def test_force_replaces(self):
        registry.add_vault("devops", "1password", {"op-vault": "Eng"}, force=True)
        self.assertEqual(registry.vaults()["devops"]["provider"], "1password")

    def test_force_does_not_migrate_and_says_so(self):
        """`--force` is an override, not a migration. Moving secrets between providers is
        its own operation and must be typed on purpose — burying it inside `add` is how
        the original bug orphaned things."""
        err = io.StringIO()
        with redirect_stderr(err):
            commands_secrets.cmd_vault_add(
                _args("devops", "1password", op_vault="Eng", force=True))
        out = err.getvalue()
        self.assertIn("NOT migrated", out)
        self.assertIn(str(self.tmp / "devops.json"), out)   # where they still are

    def test_a_different_name_is_unaffected(self):
        registry.add_vault("qa", "plain-file", {"file": str(self.tmp / "qa.json")})
        self.assertEqual(sorted(registry.vaults()), ["devops", "qa"])

    def test_the_command_exits_non_zero_on_a_refusal(self):
        """Scripted callers must be able to tell from the exit code alone that the vault
        they asked for is not the vault that is registered."""
        with redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args("devops", "1password", op_vault="Eng"))
        self.assertEqual(rc, 1)

    def test_a_first_registration_still_just_works(self):
        with redirect_stderr(io.StringIO()):
            rc = commands_secrets.cmd_vault_add(_args("fresh"))
        self.assertEqual(rc, 0)
        self.assertIn("fresh", registry.vaults())


if __name__ == "__main__":
    unittest.main()
