"""A reference vault's refusal names the provider that *does* store values (issue #70).

`ReferenceProvider.set` refuses a bare value on purpose — accepting one would turn a
reference vault into a plaintext vault without saying so. That refusal was correct and a
dead end: it offered only "use a plain-file vault", never mentioning that charter already
has a provider which creates the item AND owns it — `1password`, whose `set()` writes the
value to a new or existing item with the value on stdin.

So the reporter, forbidden by policy from calling `op` directly, concluded there was no
charter-mediated route and put a freshly generated credential in a 0600 file under /tmp,
with the irreversible half of the operation already deployed. There was a route. Nothing
pointed at it.

The distinction the message has to teach, because it is the whole reason there are two
providers:

* a **reference** vault POINTS AT items somebody else owns — it stores `op://` URIs and is
  safe to commit;
* a **1password** vault OWNS its items — charter creates, edits and deletes them.

Choosing `reference` is a statement that charter does not own these items. That is why it
declines to create one, and why the fix is a signpost rather than a write path.
"""
from __future__ import annotations

import unittest

from charter.secrets import base, reference
from tests._isolation import PersonaIso


class RefusalCase(PersonaIso):
    def provider(self):
        return reference.ReferenceProvider("refs", {"file": str(self.tmp / "refs.json")})

    def refusal(self) -> str:
        with self.assertRaises(base.VaultError) as e:
            self.provider().set("DEPLOY_KEY", "an-actual-secret-value")
        return str(e.exception)


class TestTheRefusalIsNotADeadEnd(RefusalCase):
    def test_it_names_the_provider_that_stores_values(self):
        self.assertIn("1password", self.refusal())

    def test_it_gives_the_command_to_create_such_a_vault(self):
        """Naming the provider without the invocation leaves the reader to guess at
        `--op-vault`, which is required and has no default."""
        r = self.refusal()
        self.assertIn("charter vault add", r)
        self.assertIn("--op-vault", r)

    def test_it_still_offers_the_plain_file_route(self):
        self.assertIn("plain-file", self.refusal())

    def test_it_still_says_what_a_reference_vault_stores(self):
        """The refusal has to keep teaching the model, not just list escape routes."""
        self.assertIn("URI", self.refusal())

    def test_it_names_the_key_that_was_refused(self):
        self.assertIn("DEPLOY_KEY", self.refusal())

    def test_the_value_is_never_echoed_back(self):
        """The refusal is printed. Echoing the rejected value would put a live secret in
        a terminal and any log scraping it — the failure this whole area exists to avoid.
        """
        self.assertNotIn("an-actual-secret-value", self.refusal())


class TestAValidReferenceIsStillAccepted(RefusalCase):
    """The signpost must not narrow what the provider takes."""

    def test_an_op_uri_is_stored(self):
        p = self.provider()
        p.set("K", "op://Private/item/password")
        self.assertEqual(p.reference_for("K"), "op://Private/item/password")

    def test_a_vault_uri_is_stored(self):
        p = self.provider()
        p.set("K", "vault://secret/data/app#FIELD")
        self.assertEqual(p.reference_for("K"), "vault://secret/data/app#FIELD")

    def test_a_malformed_uri_of_a_known_scheme_is_still_refused(self):
        with self.assertRaises(base.VaultError):
            self.provider().set("K", "op://only-one-part")


if __name__ == "__main__":
    unittest.main()
