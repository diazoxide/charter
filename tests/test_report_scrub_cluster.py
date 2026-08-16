"""Redaction covers the whole identifier cluster, or none of it (#137, #154).

The reported draft, verbatim:

    charter vault add [redacted] --provider 1password \\
      --op-vault "VolatiCloud Marketing" --op-item charter-[redacted] \\
      --persona [redacted] --token-env OP_VOLATICLOUD_MARKETING_SERVICE_ACCOUNT_TOKEN \\
      --share --force

The persona/vault **alias** was removed while the real 1Password vault name, the item and
the token variable survived on the adjacent lines. No secret VALUE leaked — these are all
names — but the policy was applied to one class of identifier and not to the others it sits
beside, so `[redacted]` was recoverable in one step. It cost readability and bought no
privacy.

Worse, per #154: a visible `[redacted]` creates **false confidence**. An author who sees one
concludes the identifiers were handled and stops checking what else is going out on a public
tracker under their own identity.

Two decisions are pinned here. **Redact everything charter knows**, because a vault name is
frequently a company name — the exact class of identifier this exists to keep off a public
tracker. And **say what was scrubbed**, because ADR 0003 makes the Reporter read the draft,
and a read that has to notice ABSENCES is the hardest kind to do well.
"""

from __future__ import annotations

import unittest

from charter import config, report, workspace
from charter.secrets import registry
from tests._isolation import PersonaIso


class ClusterCase(PersonaIso):
    def register_vault(self, name="marketing", op_vault="VolatiCloud Marketing",
                       op_item="charter-marketing",
                       token_env="OP_VOLATICLOUD_MARKETING_SERVICE_ACCOUNT_TOKEN"):
        registry.add_vault(name, "1password",
                           {"op-vault": op_vault, "op-item": op_item,
                            "token-env": token_env})

    def draft(self) -> str:
        return ("charter vault add marketing --provider 1password "
                '--op-vault "VolatiCloud Marketing" --op-item charter-marketing '
                "--token-env OP_VOLATICLOUD_MARKETING_SERVICE_ACCOUNT_TOKEN --share")


class TestTheWholeClusterGoes(ClusterCase):
    def test_the_real_vault_name_does_not_survive(self):
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertNotIn("VolatiCloud Marketing", out)

    def test_the_token_variable_does_not_survive(self):
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertNotIn("OP_VOLATICLOUD_MARKETING_SERVICE_ACCOUNT_TOKEN", out)

    def test_the_item_name_does_not_survive(self):
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertNotIn("charter-marketing", out)

    def test_the_alias_still_goes_too(self):
        """The half that already worked. Dropping it would be the other incoherent policy."""
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertNotIn("marketing", out)

    def test_nothing_recoverable_is_left_beside_a_redaction(self):
        """The actual complaint, asserted as one statement: after scrubbing, no fragment of
        the cluster remains from which the removed alias could be reconstructed."""
        self.register_vault()
        out, _ = report.scrub(self.draft())
        for leak in ("VolatiCloud", "MARKETING", "marketing"):
            self.assertNotIn(leak, out, leak)


class TestItStaysReadable(ClusterCase):
    def test_the_command_shape_survives(self):
        """A maintainer has to be able to follow the command. That is what the uniform
        `[redacted]` destroyed and what per-category placeholders restore."""
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertIn("charter vault add", out)
        self.assertIn("--provider 1password", out)
        self.assertIn("--share", out)

    def test_categories_are_distinguishable(self):
        self.register_vault()
        out, _ = report.scrub(self.draft())
        self.assertIn("[vault]", out)
        self.assertIn("[token-env]", out)


class TestItSaysWhatItRemoved(ClusterCase):
    def test_the_categories_are_reported(self):
        self.register_vault()
        _, used = report.scrub(self.draft())
        self.assertIn("[vault]", used)
        self.assertIn("[token-env]", used)

    def test_a_clean_report_claims_nothing(self):
        """A summary that fires when nothing was removed is how the line stops being read."""
        self.assertEqual(report.scrub("charter cannot archive a workspace"),
                         ("charter cannot archive a workspace", []))

    def test_the_rendered_draft_states_it(self):
        """ADR 0003 makes the Reporter read the draft before sending. Without this the read
        is a search for things that are NOT there — the hardest kind — which is how a
        half-redaction went out looking handled."""
        self.register_vault()
        rid = report.record_gap(self.draft())
        body = report.render(report.load(rid))
        self.assertIn("Scrubbed before drafting", body)
        self.assertIn("[vault]", body)

    def test_a_clean_draft_carries_no_scrub_line(self):
        rid = report.record_gap("charter cannot archive a workspace")
        self.assertNotIn("Scrubbed before drafting", report.render(report.load(rid)))


class TestItDegradesRatherThanFails(ClusterCase):
    def test_an_unreadable_registry_still_scrubs_the_rest(self):
        """The failure direction that matters is "scrubbed less than it could", never
        "the report never got drafted"."""
        workspace.ensure("acme-migration")
        real = registry.vaults

        def boom(*a, **kw):
            raise OSError("registry unreadable")

        registry.vaults = boom
        self.addCleanup(setattr, registry, "vaults", real)
        out, used = report.scrub("clone fails inside acme-migration")
        self.assertNotIn("acme-migration", out)
        self.assertIn("[workspace]", used)


if __name__ == "__main__":
    unittest.main()
