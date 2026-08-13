"""A 1Password vault is ONE item whose custom fields are the secrets.

charter vault `devops`, key `AWS_ACCESS_KEY_ID`
    → 1Password item `charter-devops` in the configured op-vault
      field `AWS_ACCESS_KEY_ID`, concealed

Replaces one-item-per-key. That schema forced a URI indirection file on anyone whose
1Password layout it could not describe: two keys sharing an item, or a value living in
`notesPlain` rather than `password`. Custom fields express both directly.

The module docstring used to record that one-item-per-vault was "wrong here", because
`op item get --format json` **conceals** values and round-tripping them would overwrite
every sibling secret with a mask. That objection was solvable — `--reveal` exists — so the
write path fetches revealed, merges one field, and pipes the whole item back.

Two consequences of that round-trip, designed around rather than wished away:

* every write pulls all sibling values through memory. Same trust boundary charter already
  operates in, but real, and the reason writes stay rare-path rather than routine;
* two agents writing different keys at the same moment can drop one another's field. 1Password
  keeps item history so it is recoverable, and `set` verifies after writing so it becomes a
  loud error rather than silent loss.

No secret reaches argv anywhere: reads are `op read op://vault/item/field`, writes pipe a
JSON template on stdin.
"""
from __future__ import annotations

import json
import unittest
from types import SimpleNamespace

from charter.secrets import base
from charter.secrets.onepassword import OnePasswordProvider
from tests._isolation import PersonaIso


class FakeOp:
    """Stands in for `op`, recording every argv and stdin."""

    def __init__(self, fields=None, item_exists=True, legacy=(), fail_on=None,
                 raw_fields=None, fail_stderr="denied"):
        self.calls: list[dict] = []
        #: What `op` writes to stderr when `fail_on` trips. Tests that care which
        #: failure this is set it to real `op` output — charter classifies on this
        #: text, so a generic default would make every failure look unrecognised.
        self.fail_stderr = fail_stderr
        self.fields = dict(fields or {})
        #: Full field dicts, for items charter did not create — 1Password assigns a
        #: random `id` and keeps the human text as `label`.
        self.raw_fields = raw_fields
        self.item_exists = item_exists
        self.legacy = list(legacy)
        self.fail_on = fail_on

    def _item_json(self):
        return json.dumps({
            "id": "itm1", "title": "charter-devops",
            "category": "PASSWORD", "vault": {"name": "Eng"},
            "fields": self.raw_fields if self.raw_fields is not None else
                      [{"id": k, "label": k, "type": "CONCEALED", "value": v}
                       for k, v in self.fields.items()],
        })

    def __call__(self, argv, input=None, check=False, env=None, **kw):
        self.calls.append({"argv": list(argv), "input": input})
        a = [x for x in argv if not x.startswith("--")]
        if self.fail_on and self.fail_on in " ".join(argv):
            return SimpleNamespace(returncode=1, stdout="", stderr=self.fail_stderr)
        if a[:3] == ["op", "item", "get"]:
            if not self.item_exists:
                return SimpleNamespace(returncode=1, stdout="", stderr="not found")
            return SimpleNamespace(returncode=0, stdout=self._item_json(), stderr="")
        if a[:3] == ["op", "item", "list"]:
            titles = ([{"title": "charter-devops"}] if self.item_exists else []) + \
                     [{"title": t} for t in self.legacy]
            return SimpleNamespace(returncode=0, stdout=json.dumps(titles), stderr="")
        if a[:2] == ["op", "read"]:
            uri = a[2]
            key = uri.rsplit("/", 1)[1]
            if key not in self.fields:
                return SimpleNamespace(returncode=1, stdout="", stderr="not found")
            return SimpleNamespace(returncode=0, stdout=self.fields[key], stderr="")
        if a[:3] in (["op", "item", "edit"], ["op", "item", "create"]):
            tpl = json.loads(input or "{}")
            self.raw_fields = tpl.get("fields", [])
            self.fields = {f.get("label") or f["id"]: f.get("value", "")
                           for f in tpl.get("fields", [])}
            self.item_exists = True
            return SimpleNamespace(returncode=0, stdout=self._item_json(), stderr="")
        return SimpleNamespace(returncode=0, stdout="", stderr="")

    def call_with(self, *needles):
        return [c for c in self.calls if all(n in " ".join(c["argv"]) for n in needles)]


class OpCase(PersonaIso):
    """`op` is not installed on CI, and the provider checks PATH before running.

    Stubbing that keeps these tests about charter's contract with `op` rather than about
    whether this particular machine happens to have it — the suite must give the same
    answer on a laptop with 1Password installed and on a bare runner. Carried over from
    the harness this module replaced; dropping it made CI fail where local passed.
    """

    def setUp(self) -> None:
        super().setUp()
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))

    def make(self, cfg=None, **kw):
        op = FakeOp(**kw)
        p = OnePasswordProvider("devops", {"op-vault": "Eng", **(cfg or {})})
        p.runner = op
        return op, p


class TestTheItemIsTheVault(OpCase):
    def test_the_item_name_defaults_to_charter_and_the_vault_name(self):
        _, p = self.make()
        self.assertEqual(p.op_item, "charter-devops")

    def test_an_explicit_item_is_used(self):
        _, p = self.make(cfg={"op-item": "VolatiCloud Devops Secrets"})
        self.assertEqual(p.op_item, "VolatiCloud Devops Secrets")

    def test_keys_are_the_items_fields(self):
        _, p = self.make(fields={"AWS_ACCESS_KEY_ID": "a", "GITHUB_TOKEN": "b"})
        self.assertEqual(p.keys(), ["AWS_ACCESS_KEY_ID", "GITHUB_TOKEN"])

    def test_an_absent_item_has_no_keys(self):
        _, p = self.make(item_exists=False)
        self.assertEqual(p.keys(), [])


class TestReading(OpCase):
    def test_a_value_is_read_by_reference(self):
        op, p = self.make(fields={"GITHUB_TOKEN": "ghp_xyz"})
        self.assertEqual(p.get("GITHUB_TOKEN"), "ghp_xyz")

    def test_the_reference_addresses_vault_item_field(self):
        op, p = self.make(fields={"GITHUB_TOKEN": "t"})
        p.get("GITHUB_TOKEN")
        self.assertIn("op://Eng/charter-devops/GITHUB_TOKEN",
                      " ".join(op.call_with("read")[0]["argv"]))

    def test_a_missing_key_raises_not_found(self):
        _, p = self.make(fields={"A": "1"})
        with self.assertRaises(base.SecretNotFound):
            p.get("NOPE")

    def test_reading_does_not_fetch_every_sibling(self):
        """`op read` returns one field. Reading via the whole-item round-trip would pull
        every other secret into memory for no reason — writes pay that cost, reads must
        not."""
        op, p = self.make(fields={"A": "1", "B": "2"})
        p.get("A")
        self.assertEqual(op.call_with("item", "get"), [])


class TestWriting(OpCase):
    def test_a_new_key_becomes_a_field(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "2")
        self.assertEqual(sorted(op.fields), ["A", "B"])

    def test_siblings_keep_their_real_values(self):
        """The failure the old docstring predicted: a concealed round-trip writes masks
        back over every other secret."""
        op, p = self.make(fields={"A": "keep-me", "B": "also-me"})
        p.set("C", "new")
        self.assertEqual(op.fields["A"], "keep-me")
        self.assertEqual(op.fields["B"], "also-me")

    def test_the_read_back_is_revealed(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "2")
        self.assertIn("--reveal", " ".join(op.call_with("item", "get")[0]["argv"]))

    def test_an_existing_key_is_overwritten(self):
        op, p = self.make(fields={"A": "old"})
        p.set("A", "new")
        self.assertEqual(op.fields["A"], "new")

    def test_the_value_never_reaches_argv(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "s3cret-value")
        for c in op.calls:
            self.assertNotIn("s3cret-value", " ".join(c["argv"]))

    def test_the_value_travels_on_stdin(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "s3cret-value")
        self.assertTrue(any("s3cret-value" in (c["input"] or "") for c in op.calls))

    def test_fields_are_written_concealed(self):
        op, p = self.make(item_exists=False)
        p.set("B", "v")
        tpl = json.loads(next(c for c in op.calls if c["input"])["input"])
        self.assertEqual(tpl["fields"][0]["type"], "CONCEALED")

    def test_the_item_is_created_when_absent(self):
        op, p = self.make(item_exists=False)
        p.set("A", "1")
        self.assertTrue(op.call_with("item", "create"))

    def test_an_existing_item_is_edited_not_recreated(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "2")
        self.assertTrue(op.call_with("item", "edit"))
        self.assertEqual(op.call_with("item", "create"), [])


class TestWriteVerification(OpCase):
    """The round-trip means a concurrent writer can drop a field. Verifying after the
    write turns silent loss into a loud error."""

    def test_a_write_that_did_not_land_raises(self):
        op, p = self.make(fields={"A": "1"})
        original = op.__call__

        def swallow(argv, input=None, **kw):
            if [x for x in argv if not x.startswith("--")][:3] == ["op", "item", "edit"]:
                op.calls.append({"argv": list(argv), "input": input})
                return SimpleNamespace(returncode=0, stdout="", stderr="")  # nothing applied
            return original(argv, input=input, **kw)

        p.runner = swallow
        with self.assertRaises(base.VaultError):
            p.set("B", "2")

    def test_the_error_does_not_echo_the_value(self):
        op, p = self.make(fields={"A": "1"})
        original = op.__call__

        def swallow(argv, input=None, **kw):
            if [x for x in argv if not x.startswith("--")][:3] == ["op", "item", "edit"]:
                op.calls.append({"argv": list(argv), "input": input})
                return SimpleNamespace(returncode=0, stdout="", stderr="")
            return original(argv, input=input, **kw)

        p.runner = swallow
        with self.assertRaises(base.VaultError) as e:
            p.set("B", "s3cret-value")
        self.assertNotIn("s3cret-value", str(e.exception))


class TestDeleting(OpCase):
    def test_a_field_is_removed(self):
        op, p = self.make(fields={"A": "1", "B": "2"})
        p.delete("A")
        self.assertEqual(sorted(op.fields), ["B"])

    def test_the_item_itself_survives(self):
        op, p = self.make(fields={"A": "1", "B": "2"})
        p.delete("A")
        self.assertEqual(op.call_with("item", "delete"), [])

    def test_a_missing_key_raises_not_found(self):
        _, p = self.make(fields={"A": "1"})
        with self.assertRaises(base.SecretNotFound):
            p.delete("NOPE")


class TestLegacyItemsAreNotSilentlyOrphaned(OpCase):
    """One-item-per-key wrote `charter-<vault>-<key>` items. Those credentials still
    exist; charter simply stops finding them. Reporting an empty vault as healthy is the
    one outcome that must not happen — it is the #55 failure in a new place."""

    def test_health_names_orphaned_legacy_items(self):
        _, p = self.make(item_exists=False,
                         legacy=["charter-devops-AWS_KEY", "charter-devops-GH_TOKEN"])
        ok, detail = p.health()
        self.assertFalse(ok)
        self.assertIn("2", detail)

    def test_a_clean_vault_is_healthy(self):
        _, p = self.make(fields={"A": "1"})
        ok, _ = p.health()
        self.assertTrue(ok)

    def test_no_legacy_and_no_item_is_merely_empty(self):
        _, p = self.make(item_exists=False)
        ok, detail = p.health()
        self.assertTrue(ok)
        self.assertIn("no", detail.lower())


# --------------------------------------------------------------------------- #
# Migrated from tests/test_secret_onepassword.py when the schema changed.       #
# These assert properties that have nothing to do with one-item-per-key vs      #
# one-item-per-vault — argv safety, withheld output, permission hints, a        #
# missing CLI, account pinning — so they moved here rather than being deleted   #
# with the schema-specific cases. Losing them would have gutted the coverage of #
# charter's most security-sensitive module for an unrelated refactor.           #
# --------------------------------------------------------------------------- #
class TestConfiguration(OpCase):
    def test_a_missing_op_vault_is_a_clear_error(self):
        p = OnePasswordProvider("devops", {})
        p.runner = FakeOp()
        with self.assertRaises(base.VaultError) as e:
            p.keys()
        self.assertIn("op-vault", str(e.exception))

    def test_an_account_is_pinned_when_configured(self):
        """A machine signed into several accounts resolves an unqualified vault name
        against whichever is default — a quiet way to write into the wrong company."""
        op, p = self.make(cfg={"account": "acme.1password.com"}, fields={"A": "1"})
        p.get("A")
        self.assertIn("--account", " ".join(op.calls[0]["argv"]))

    def test_no_account_flag_when_unconfigured(self):
        op, p = self.make(fields={"A": "1"})
        p.get("A")
        self.assertNotIn("--account", " ".join(op.calls[0]["argv"]))


class TestErrorsWithholdOutput(OpCase):
    """`op`'s stderr can echo what it was given, and on a read path its stdout IS the
    secret. Errors report the exit status and the attempt, never the output."""

    def test_a_failed_write_does_not_echo_the_value(self):
        op, p = self.make(fields={"A": "1"}, fail_on="item edit")
        with self.assertRaises(base.VaultError) as e:
            p.set("B", "s3cret-value")
        self.assertNotIn("s3cret-value", str(e.exception))

    def test_a_failed_read_does_not_echo_the_provider_output(self):
        op, p = self.make(fields={})
        with self.assertRaises(base.SecretNotFound) as e:
            p.get("NOPE")
        self.assertNotIn("denied", str(e.exception))


class TestWriteFailuresNamePermissions(OpCase):
    """Against a real account the first failure was `(101) You do not have permission` —
    a service-account token that could read the vault but not write to it. The original
    message suggested checking sign-in and vault existence; both were true.

    This class used to fail a write with the fake's generic stderr and assert that the
    message named WRITE access. It passed, because the message named WRITE access on
    every write failure whatever `op` had said — so the test asserted the diagnosis
    without ever supplying the evidence for it, and could not have distinguished a
    message that had learnt something from one that always says the same thing. That is
    what #78 walked into. The assertion is kept and now earns itself: the fake emits the
    real 101, and its opposite is pinned alongside.
    """

    def test_a_write_failure_against_a_real_101_names_permissions(self):
        op, p = self.make(fields={"A": "1"}, fail_on="item edit")
        op.fail_stderr = "(101) You do not have permission to perform this action"
        with self.assertRaises(base.VaultError) as e:
            p.set("B", "2")
        self.assertIn("write access", str(e.exception).lower())

    def test_a_write_failure_op_did_not_explain_does_not_assert_permissions(self):
        """The other half, and the actual regression: an unrecognised failure may list
        a read-only token among its candidates, never announce it as the cause."""
        op, p = self.make(fields={"A": "1"}, fail_on="item edit")
        op.fail_stderr = "ERROR: unexpected end of JSON input"
        with self.assertRaises(base.VaultError) as e:
            p.set("B", "2")
        m = str(e.exception).lower()
        self.assertIn("did not recognise", m)
        self.assertNotIn("write access", m)

    def test_a_read_failure_does_not_blame_permissions(self):
        op, p = self.make(fields={})
        with self.assertRaises(base.SecretNotFound) as e:
            p.get("NOPE")
        self.assertNotIn("write access", str(e.exception).lower())


class TestMissingCli(OpCase):
    def test_a_missing_op_binary_says_so_and_how_to_fix_it(self):
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))
        _, p = self.make()
        with self.assertRaises(base.VaultError) as e:
            p.get("A")
        self.assertIn("op", str(e.exception))

    def test_health_reports_a_missing_cli_rather_than_raising(self):
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))
        _, p = self.make()
        ok, detail = p.health()
        self.assertFalse(ok)
        self.assertIn("PATH", detail)


class TestHealthNeverReadsAValue(OpCase):
    def test_health_does_not_reveal(self):
        """`vault list` and `doctor` call health routinely; revealing there would hit
        1Password on every listing and could prompt for re-auth."""
        op, p = self.make(fields={"A": "1"})
        p.health()
        self.assertNotIn("--reveal", " ".join(" ".join(c["argv"]) for c in op.calls))


class TestAdoptedItemsUseLabelsAsKeys(OpCase):
    """Issue #75. `--op-item` documents itself as "point it at an item you already
    curate" — and adopting one produced 1Password's random field IDs as key names, so
    every consumer naming a key broke.

    Invisible on charter-created items, because `_write` sets id == label == key. It
    appears only on adopted items, which is precisely the case the flag exists for.
    """

    ADOPTED = [
        {"id": "27r3gphb4fnsonx5ikcaw3cxwq", "label": "PROD_KUBECONFIG",
         "type": "CONCEALED", "value": "kubeconfig-body"},
        {"id": "6kqf7x6wchnoeucvbwwogt6bai", "label": "GITHUB_TOKEN",
         "type": "CONCEALED", "value": "ghp_xyz"},
    ]

    def test_keys_are_the_labels_a_human_typed(self):
        _, p = self.make(raw_fields=self.ADOPTED)
        self.assertEqual(p.keys(), ["GITHUB_TOKEN", "PROD_KUBECONFIG"])

    def test_the_generated_ids_are_not_keys(self):
        _, p = self.make(raw_fields=self.ADOPTED)
        self.assertNotIn("27r3gphb4fnsonx5ikcaw3cxwq", p.keys())

    def test_a_field_with_no_label_still_resolves_by_id(self):
        """1Password does not require a label. Falling back keeps such a field
        addressable rather than dropping it silently."""
        _, p = self.make(raw_fields=[{"id": "abc123", "type": "CONCEALED", "value": "v"}])
        self.assertEqual(p.keys(), ["abc123"])

    def test_charter_created_items_are_unaffected(self):
        _, p = self.make(fields={"A": "1", "B": "2"})
        self.assertEqual(p.keys(), ["A", "B"])


class TestDuplicateLabels(OpCase):
    """1Password permits two fields with the same label; ids disambiguate. Letting one
    win silently would make a secret unreachable while everything reported healthy —
    which is the #55 shape, and worse here because the loss is invisible."""

    DUPES = [
        {"id": "aaa", "label": "TOKEN", "type": "CONCEALED", "value": "s3cret-alpha"},
        {"id": "bbb", "label": "TOKEN", "type": "CONCEALED", "value": "s3cret-beta"},
    ]

    def test_it_refuses_rather_than_picking_one(self):
        _, p = self.make(raw_fields=self.DUPES)
        with self.assertRaises(base.VaultError):
            p.keys()

    def test_the_error_names_the_item_and_the_label(self):
        _, p = self.make(raw_fields=self.DUPES)
        with self.assertRaises(base.VaultError) as e:
            p.keys()
        self.assertIn("TOKEN", str(e.exception))
        self.assertIn("charter-devops", str(e.exception))

    def test_the_error_does_not_echo_a_value(self):
        """Values are distinctive on purpose: a short fixture like "one" collides with
        ordinary prose in the message ("Rename one in 1Password") and the assertion then
        fails on the explanation rather than on a leak."""
        _, p = self.make(raw_fields=self.DUPES)
        with self.assertRaises(base.VaultError) as e:
            p.keys()
        self.assertNotIn("s3cret", str(e.exception))

    def test_health_reports_it_rather_than_raising(self):
        _, p = self.make(raw_fields=self.DUPES)
        ok, _ = p.health()
        self.assertFalse(ok)


class TestAdoptionIsNonDestructive(OpCase):
    """A round-trip used to rewrite `id` to the key name, mutating identifiers the user
    never chose on an item charter does not own."""

    ADOPTED = [
        {"id": "27r3gphb4fnsonx5ikcaw3cxwq", "label": "PROD_KUBECONFIG",
         "type": "CONCEALED", "value": "body"},
    ]

    def test_an_existing_fields_id_is_preserved(self):
        op, p = self.make(raw_fields=list(self.ADOPTED))
        p.set("GITHUB_TOKEN", "ghp_new")
        written = {f["label"]: f["id"] for f in op.raw_fields}
        self.assertEqual(written["PROD_KUBECONFIG"], "27r3gphb4fnsonx5ikcaw3cxwq")

    def test_a_new_field_takes_the_key_as_its_id(self):
        op, p = self.make(raw_fields=list(self.ADOPTED))
        p.set("GITHUB_TOKEN", "ghp_new")
        written = {f["label"]: f["id"] for f in op.raw_fields}
        self.assertEqual(written["GITHUB_TOKEN"], "GITHUB_TOKEN")

    def test_the_adopted_value_survives_the_write(self):
        op, p = self.make(raw_fields=list(self.ADOPTED))
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(op.fields["PROD_KUBECONFIG"], "body")


if __name__ == "__main__":
    unittest.main()
