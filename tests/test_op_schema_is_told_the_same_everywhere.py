"""A 1Password vault is ONE item whose concealed fields are the secrets — everywhere.

`tests/test_onepassword_single_item.py` pins the provider's behaviour. This module pins
the two places that *describe* it to an operator, because both went on describing the
schema it replaced (#400).

`docs/secrets.md` carried a "Schema — **one 1Password item per secret**" table mapping
vault `devops` / key `KUBECONFIG` onto an item `charter-devops-KUBECONFIG`, and then
argued at length that one item per vault "is wrong here". Neither half survived the
code: `OnePasswordProvider` writes one item per vault by exactly the read-modify-write
that paragraph called wrong, and `health()` carries a `_legacy_items()` path whose whole
job is to find leftovers of the layout that table presented as current. An operator
following it looks for an item charter has not created for several releases.

`charter vault add --provider 1password` said the same thing in its own output — "charter
creates one 1Password item per secret" — at the one moment an operator is told where
their credentials will live.

Wrong documentation of a credential store is not a cosmetic failure: it sends someone
looking for a secret in a place nothing put one, and the natural next move is to write it
again somewhere else.
"""
from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from types import SimpleNamespace

from charter import commands_secrets
from charter.secrets.onepassword import OnePasswordProvider
from tests._isolation import PersonaIso

ROOT = Path(__file__).resolve().parents[1]
SECRETS_DOC = (ROOT / "docs" / "secrets.md").read_text()


class TestTheDocDescribesTheSchemaTheCodeImplements(unittest.TestCase):
    """Prose, pinned only where drifting would send someone to the wrong item."""

    def test_it_does_not_claim_one_item_per_secret(self):
        """The removed schema, stated as current. This is the sentence #400 was filed
        about, and the one an operator acts on."""
        self.assertNotIn("item per secret", SECRETS_DOC)

    def test_it_does_not_advertise_the_per_key_item_title(self):
        """`charter-<vault>-<KEY>` is what `_legacy_items` hunts for, so the doc may name
        it as history — never as the item to go and look in. The old table's worked
        example was `charter-devops-KUBECONFIG`."""
        self.assertNotIn("charter-devops-KUBECONFIG", SECRETS_DOC)

    def test_it_names_the_item_a_vault_actually_lives_in(self):
        """`charter-<vault>`, the `--op-item` override, and what the fields are.

        The three negatives above can all be satisfied by deleting text, so this pins the
        replacement: an operator must still be able to read off which item to open and
        what they will find in it. Green before the fix as well as after — the worked
        example it pins predates #400; what was wrong was the table underneath it.

        `(?!-)` is load-bearing. `charter-devops-KUBECONFIG` *contains* `charter-devops`,
        so a plain substring check reads the stale per-key title as the right answer and
        the assertion stops being able to fail.
        """
        self.assertRegex(SECRETS_DOC, r"charter-devops(?!-)")
        self.assertIn("--op-item", SECRETS_DOC)
        self.assertIn("custom fields are the secrets", SECRETS_DOC)

    def test_it_does_not_argue_against_the_schema_in_force(self):
        """The doc did not merely lag; it made the case *against* one item per vault. The
        concealment hazard that argument rests on is real and is handled — `set()` and
        `delete()` fetch with `--reveal` — so it survives as the reason that flag exists,
        not as a reason the schema is wrong."""
        self.assertNotIn("One item per *vault* is the tidier-looking design", SECRETS_DOC)
        self.assertIn("--reveal", SECRETS_DOC)


class _FakeOp:
    """Stands in for the `op` CLI. `health()` shells out, and `vault add` calls it.

    The suite must answer the same on a laptop with 1Password installed and on a bare
    runner — hence this, and the `shutil.which` stub below.
    """

    def __init__(self, title: str) -> None:
        self.title = title

    def __call__(self, argv, input=None, **kw):
        bare = [a for a in argv if not a.startswith("--")]
        if bare[:3] == ["op", "item", "get"]:
            return SimpleNamespace(returncode=0, stderr="", stdout=json.dumps({
                "id": "itm1", "title": self.title, "category": "PASSWORD",
                "fields": [{"id": "KUBECONFIG", "label": "KUBECONFIG",
                            "type": "CONCEALED", "value": "x"}]}))
        if bare[:3] == ["op", "item", "list"]:
            return SimpleNamespace(returncode=0, stderr="",
                                   stdout=json.dumps([{"title": self.title}]))
        return SimpleNamespace(returncode=0, stdout="", stderr="")


def _args(name: str, **kw) -> SimpleNamespace:
    return SimpleNamespace(name=name, provider="1password", file=None,
                           op_vault=kw.pop("op_vault", "Engineering"),
                           op_item=kw.pop("op_item", None), account=None,
                           persona=None, force=False, share=False)


class TestRegistrationSaysWhereTheSecretsWillLive(PersonaIso):
    """`charter vault add --provider 1password` is where an operator learns the layout."""

    def setUp(self) -> None:
        super().setUp()
        import charter.secrets.onepassword as mod
        real_which = mod.shutil.which
        mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real_which))

    def _add(self, name: str, item_title: str, **kw) -> str:
        real_runner = OnePasswordProvider.__dict__["runner"]
        OnePasswordProvider.runner = _FakeOp(item_title)
        self.addCleanup(lambda: setattr(OnePasswordProvider, "runner", real_runner))
        err = io.StringIO()
        with redirect_stderr(err):
            rc = commands_secrets.cmd_vault_add(_args(name, **kw))
        self.assertEqual(rc, 0, err.getvalue())
        return err.getvalue()

    def _schema_line(self, out: str) -> str:
        """The line that describes the LAYOUT, isolated from the rest of the output.

        Asserting over the whole of `out` would be unfailable: `health()` runs on the same
        command and its detail already names the item ("no secrets yet in item
        'charter-devops'"), so every item-naming assertion below passed against the old
        one-item-per-secret text too. Verified by mutation, which is the only reason this
        helper exists.
        """
        lines = [ln for ln in out.splitlines() if "concealed field" in ln]
        self.assertEqual(len(lines), 1, f"expected one layout line, got:\n{out}")
        return lines[0]

    def test_it_names_the_single_item_the_vault_lives_in(self):
        """The name to type into the 1Password search box. Naming only the op-vault
        leaves the operator to guess it, and the guess the old text invited was
        `charter-devops-KUBECONFIG`."""
        line = self._schema_line(self._add("devops", "charter-devops"))
        self.assertIn("'charter-devops'", line)

    def test_it_does_not_claim_one_item_per_secret(self):
        out = self._add("devops", "charter-devops")
        self.assertNotIn("item per secret", out)

    def test_an_adopted_item_is_named_as_itself(self):
        """`--op-item` points charter at an item somebody already curates. Printing the
        `charter-<vault>` default there would name an item that does not exist and will
        never be created."""
        line = self._schema_line(self._add("devops", "team-kube", op_item="team-kube"))
        self.assertIn("'team-kube'", line)
        self.assertNotIn("charter-devops", line)


if __name__ == "__main__":
    unittest.main()
