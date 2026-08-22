"""A 1Password vault charter could not read is not a vault with no secrets (#322).

Reported through `charter report bug` by an operator whose four populated vaults all
printed::

    • Vault 'devops' has no secrets.

The vaults were fine. `op` was rate-limiting the client — an earlier agent on the same
path saw *Too many requests. Your client has been rate-limited.* across ~17 attempts —
and `OnePasswordProvider._fields` turned every non-zero exit of `op item get` into `{}`:

    if proc.returncode != 0:
        return {}

`keys()` sorts that into `[]` and `cmd_secret_list` prints "has no secrets". The failure
arrived as a benign state, which is the same species as `doctor`'s `_NOT_CHECKED_HINT`
(ADR 0013) and #55's "0 secrets, all fine" over a vault whose credentials charter had
simply stopped being able to see.

The two diagnoses demand opposite responses — *go and populate it* versus *check the
token, and the contents are unknown* — so an hour went into re-provisioning secrets that
were already there, and the re-provisioning could not be verified afterwards because the
same masked failure was still masking it. It also inverts the guardrail in charter's own
guidance, "do not reach past charter to raw `op` because a vault is empty": that
instruction assumes *empty* is true, and when empty is a lie the pressure to bypass
charter goes up rather than down.

**Absence is proven here, not assumed.** `op item get` exits non-zero both for "there is
no such item yet" and for every way a read can fail, so the exit code cannot tell them
apart and neither can charter. What can: ask this vault's *own identity* to list the
vault. A listing that succeeds and does not contain the item proves there is no item. A
listing that fails is the answer — the read failed, and it propagates. No matching of
`op`'s English anywhere on this path, so a reworded error cannot quietly turn a failure
back into an empty vault.

Every test below asserts its own precondition, because the one way this file could pass
while proving nothing is a fixture whose vault is genuinely empty.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets
from charter.secrets import base
from charter.secrets.onepassword import OnePasswordProvider

#: What `op` writes when a service account has been rate-limited. Quoted from the session
#: in #322 — the operator's own `op` output, from an agent invoking the path charter
#: invokes. The process still exits 1, the same code as "no such item", which is exactly
#: why the exit status cannot classify this.
RATE_LIMITED = "[ERROR] 2026/08/20 11:02:31 Too many requests. Your client has been rate-limited."

#: What `op` writes for an item that is not there. charter deliberately does **not** match
#: on this text — absence is proven by a successful listing instead — so this string is
#: here for realism and nothing depends on its wording.
NO_SUCH_ITEM = '[ERROR] 2026/08/20 11:02:31 "charter-devops" isn\'t an item.'

ITEM = "charter-devops"


class FakeOp:
    """`op`, with each subcommand's outcome settable on its own.

    Separate from the fake in `test_onepassword_single_item`, whose `fail_on` fails
    whatever a substring matches. The distinction under test is precisely *which* `op`
    call failed, so it has to be set per subcommand.
    """

    def __init__(self, *, fields=None, titles=(ITEM,), fails=(), stderr=RATE_LIMITED):
        self.calls: list[dict] = []
        self.fields = dict(fields or {})
        #: Item titles the vault holds, as this identity can see them.
        self.titles = list(titles)
        #: Subcommands that fail, e.g. ``{"item get", "item list"}``.
        self.fails = set(fails)
        self.stderr = stderr

    @staticmethod
    def _sub(argv) -> str:
        words = [w for w in argv[1:] if not w.startswith("-")]
        if words[:1] == ["item"]:
            return " ".join(words[:2])
        return words[0] if words else ""

    def _item_json(self) -> str:
        return json.dumps({
            "id": "itm1", "title": ITEM, "category": "PASSWORD",
            "fields": [{"id": k, "label": k, "type": "CONCEALED", "value": v}
                       for k, v in self.fields.items()],
        })

    def __call__(self, argv, input=None, check=False, env=None, **kw):
        sub = self._sub(argv)
        self.calls.append({"argv": list(argv), "sub": sub, "input": input})
        if sub in self.fails:
            return SimpleNamespace(returncode=1, stdout="", stderr=self.stderr)
        if sub == "item get":
            if ITEM not in self.titles:
                return SimpleNamespace(returncode=1, stdout="", stderr=NO_SUCH_ITEM)
            return SimpleNamespace(returncode=0, stdout=self._item_json(), stderr="")
        if sub == "item list":
            return SimpleNamespace(
                returncode=0,
                stdout=json.dumps([{"title": t} for t in self.titles]), stderr="")
        if sub in ("item create", "item edit"):
            # A write replaces the item with exactly the template it was piped, which is
            # what makes a dropped sibling observable here rather than only in 1Password.
            self.fields = {f.get("label") or f["id"]: f.get("value", "")
                           for f in json.loads(input or "{}").get("fields", [])}
            if ITEM not in self.titles:
                self.titles.append(ITEM)
            return SimpleNamespace(returncode=0, stdout=self._item_json(), stderr="")
        if sub == "read":
            key = [w for w in argv if w.startswith("op://")][0].rsplit("/", 1)[1]
            if key not in self.fields:
                return SimpleNamespace(returncode=1, stdout="", stderr=NO_SUCH_ITEM)
            return SimpleNamespace(returncode=0, stdout=self.fields[key], stderr="")
        return SimpleNamespace(returncode=0, stdout="", stderr="")

    def subs(self) -> list[str]:
        return [c["sub"] for c in self.calls]


class OpCase(unittest.TestCase):
    """`op` is not on CI's PATH and the provider checks before running, so PATH is pinned
    rather than inherited — the same reason `test_onepassword_single_item` does it."""

    def setUp(self) -> None:
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))

    def make(self, **kw):
        op = FakeOp(**kw)
        p = OnePasswordProvider("devops", {
            "op-vault": "Eng",
            # A per-scope identity, which is the setup #322 was reported from: four vaults,
            # four service accounts. It is what `identity_note` names in the failure.
            "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_ENG_DEVOPS_TOKEN"},
        })
        p.runner = op
        return op, p

    def setenv(self, value: str = "ops-not-a-real-token") -> None:
        """`env_overlay` raises when a declared identity variable is unset, which would
        refuse every call below before `op` was reached."""
        patch = mock.patch.dict("os.environ", {"OP_ENG_DEVOPS_TOKEN": value})
        patch.start()
        self.addCleanup(patch.stop)


class ThePopulatedVaultIsReallyPopulated(OpCase):
    """The precondition the whole file rests on.

    Same construction as every failing case below, minus the failure. If this returned
    `[]` the fixture's vault would be genuinely empty and "has no secrets" would be the
    truth, so every assertion about a *masked failure* would be vacuous.
    """

    def test_the_fixture_vault_holds_secrets_when_op_answers(self):
        self.setenv()
        op, p = self.make(fields={"AWS_ACCESS_KEY_ID": "a", "GITHUB_TOKEN": "b"})
        self.assertEqual(p.keys(), ["AWS_ACCESS_KEY_ID", "GITHUB_TOKEN"])
        self.assertIn("item get", op.subs(), "`op item get` was never reached")


class AFailedReadIsReported(OpCase):
    def setUp(self) -> None:
        super().setUp()
        self.setenv()
        # Rate limiting is not per-subcommand: the client is limited, so the listing that
        # would prove absence fails too. That is what makes absence unprovable and the
        # failure the only honest answer.
        self.op, self.p = self.make(fields={"AWS_ACCESS_KEY_ID": "a"},
                                    fails={"item get", "item list"})

    def test_the_precondition_the_read_actually_failed(self):
        """Not "the vault was empty". `op item get` ran and exited non-zero."""
        with self.assertRaises(base.VaultError):
            self.p.keys()
        self.assertIn("item get", self.op.subs(), "`op item get` was never reached")
        got = [c for c in self.op.calls if c["sub"] == "item get"][0]
        self.assertEqual(self.op._sub(got["argv"]), "item get")

    def test_keys_raises_rather_than_returning_an_empty_list(self):
        with self.assertRaises(base.VaultError):
            self.p.keys()

    def test_the_failure_names_the_rate_limit_rather_than_guessing_at_tokens(self):
        """`_fail`'s unrecognised-failure fallback sends the reader to check the identity
        variable and the sign-in — which is where #322's operator went, and rotated a
        token that was fine. A recognised signature says what actually happened."""
        with self.assertRaises(base.VaultError) as raised:
            self.p.keys()
        self.assertIn("rate-limit", str(raised.exception).lower())

    def test_the_failure_never_carries_ops_own_output(self):
        """`op`'s stderr can echo an assignment, and on a read path its stdout IS the
        secret. The error class is separable from the value and only the class travels."""
        with self.assertRaises(base.VaultError) as raised:
            self.p.keys()
        self.assertNotIn(RATE_LIMITED, str(raised.exception))
        self.assertNotIn("AWS_ACCESS_KEY_ID", str(raised.exception))

    def test_the_failure_names_the_identity_the_read_was_made_under(self):
        """Four vaults, four service accounts: which one failed is the first thing the
        operator needs and the last thing they can guess."""
        with self.assertRaises(base.VaultError) as raised:
            self.p.keys()
        self.assertIn("OP_ENG_DEVOPS_TOKEN", str(raised.exception))

    def test_health_does_not_report_the_vault_as_fine(self):
        """`vault list` and `doctor` read this. A green line over an unreadable vault is
        #55 in a new place."""
        healthy, detail = self.p.health()
        self.assertFalse(healthy)
        self.assertNotIn("no secrets", detail)

    def test_a_write_does_not_replace_the_item_with_what_it_could_not_read(self):
        """The same swallow on the write path, and the expensive half. `set` is a
        read-modify-write; `{}` for the read means the template it pipes back holds only
        the new key, and every sibling secret in the item is dropped."""
        with self.assertRaises(base.VaultError):
            self.p.set("NEW_KEY", "v")
        self.assertNotIn("item create", self.op.subs())
        self.assertNotIn("item edit", self.op.subs())


class AGenuinelyEmptyVaultStillReadsAsEmpty(OpCase):
    """The other direction, and the reason a failed read cannot simply always raise.

    A vault whose item has not been created yet is the ordinary state of a freshly
    registered vault. `op item get` fails for it too, so the fix has to tell it apart —
    and it does, by the listing this identity can still perform.
    """

    def setUp(self) -> None:
        super().setUp()
        self.setenv()
        self.op, self.p = self.make(fields={}, titles=())

    def test_the_precondition_the_read_failed_and_the_vault_is_readable(self):
        self.p.keys()
        self.assertIn("item get", self.op.subs())
        self.assertIn("item list", self.op.subs(),
                      "absence was assumed from the failed get rather than proven")

    def test_keys_are_empty(self):
        self.assertEqual(self.p.keys(), [])

    def test_health_says_there_is_nothing_in_it_yet(self):
        healthy, detail = self.p.health()
        self.assertTrue(healthy)
        self.assertIn("no secrets", detail)

    def test_the_first_write_still_creates_the_item(self):
        self.p.set("FIRST", "v")
        self.assertIn("item create", self.op.subs())


class AnUnreadableItemInAReadableVaultIsReported(OpCase):
    """The item is right there in the listing and `op item get` still failed. Absence is
    disproven, so this cannot be reported as empty either."""

    def setUp(self) -> None:
        super().setUp()
        self.setenv()
        self.op, self.p = self.make(fields={"A": "1"}, fails={"item get"})

    def test_the_precondition_the_item_is_visible_to_this_identity(self):
        self.assertIn(ITEM, self.p._list_items(tagged=False))

    def test_keys_raises(self):
        with self.assertRaises(base.VaultError):
            self.p.keys()


class TheCommandSaysWhichOfTheTwoItIs(OpCase):
    """What the operator in #322 actually saw. `_provider` is stubbed rather than the
    registry populated, so this is about `cmd_secret_list`'s contract with a provider."""

    def _list(self, p) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(commands_secrets, "_provider", lambda _n: p), \
             redirect_stdout(out), redirect_stderr(err):
            code = commands_secrets.cmd_secret_list(SimpleNamespace(vault="devops"))
        return code, out.getvalue() + err.getvalue()

    def test_an_unreadable_vault_is_not_reported_as_empty(self):
        self.setenv()
        op, p = self.make(fields={"AWS_ACCESS_KEY_ID": "a"},
                          fails={"item get", "item list"})
        code, text = self._list(p)
        self.assertNotIn("has no secrets", text)
        self.assertIn("rate-limit", text.lower())
        self.assertNotEqual(code, 0,
                            "a scripted caller would proceed against an apparently-empty "
                            "vault")
        self.assertIn("item get", op.subs(), "the read never happened")

    def test_an_empty_vault_is_still_reported_as_empty(self):
        self.setenv()
        _, p = self.make(fields={}, titles=())
        code, text = self._list(p)
        self.assertIn("has no secrets", text)
        self.assertEqual(code, 0)

    def test_a_populated_vault_still_lists_its_keys(self):
        """The feature, unchanged. A fix that reported every vault as unreadable would
        satisfy every assertion above and tell nobody anything."""
        self.setenv()
        _, p = self.make(fields={"AWS_ACCESS_KEY_ID": "value-of-the-aws-key",
                                 "GITHUB_TOKEN": "value-of-the-github-token"})
        code, text = self._list(p)
        self.assertEqual(code, 0)
        self.assertIn("AWS_ACCESS_KEY_ID", text)
        self.assertIn("GITHUB_TOKEN", text)
        # Names, never values: redacting the value is right, and it is what makes
        # redacting the *failure* separable from it.
        self.assertNotIn("value-of-the-aws-key", text)
        self.assertNotIn("value-of-the-github-token", text)


class AnUnsetIdentityIsNotAnEmptyVaultEither(OpCase):
    """The failure mode `env_overlay` already refuses, pinned from this end: it must not
    come back as an empty listing through the path #322 opened."""

    def test_a_declared_identity_that_is_unset_reports_rather_than_listing_nothing(self):
        _, p = self.make(fields={"AWS_ACCESS_KEY_ID": "a"})
        with mock.patch.dict("os.environ", {}, clear=False):
            import os
            os.environ.pop("OP_ENG_DEVOPS_TOKEN", None)
            with self.assertRaises(base.VaultError):
                p.keys()
