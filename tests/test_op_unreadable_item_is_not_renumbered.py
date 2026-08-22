"""A read that failed on the WRITE path is not "the item has no ids" (#354).

#352 fixed `OnePasswordProvider._fields`, where `if proc.returncode != 0: return {}` turned
every kind of read failure into an empty vault. It deliberately did not widen into the two
sibling helpers carrying the same shape, and this file is that follow-up.

The write path is worse in kind. `_fields` failing produced a false *diagnosis* — "has no
secrets" about a populated vault — which is visible the moment somebody looks. These fail
as a silent *mutation*, and `set()` still returns successfully:

* `_existing_ids` exists to keep an adopted item's field ids stable across a write. `{}`
  means charter believes there are no existing ids and assigns fresh ones, so a transient
  failure renumbers the fields of an item charter did not create.
* `_item_present` answers "does the item exist" with `return proc.returncode == 0`, and
  `op item get` exits non-zero both for *no such item* and for every way a read can fail.
  A failure therefore reads as *absent*, which flips `_write` from `item edit` to `item
  create` against a title that is already there — leaving two same-titled items, after
  which `op item get <title>` is ambiguous and the vault is unreadable until a human
  deletes one by hand. That is the worst outcome of the set, and it is not a `return {}`.

None of this needs an attacker or a race with another writer. A rate limit is enough, and
rate limiting is what #322 was actually reported from.

**The window is the whole point, and it is what makes these tests non-vacuous.** `set()`
issues *three* separate `op item get` calls — `_fields`, then `_item_present`, then
`_existing_ids` — all fetching the same document (#355). So the failure under test is not
"op is broken", which #352 already covers and which would make `_fields` raise before any
of this is reached. It is the second or third call failing after the first SUCCEEDED. Every
test below therefore asserts that the first `op item get` returned 0 before the one that
failed; a fixture where op fails from the start would prove nothing here, and neither would
an item that genuinely had no fields to renumber.
"""

from __future__ import annotations

import json
import unittest
from types import SimpleNamespace
from unittest import mock

from charter.secrets import base
from charter.secrets.onepassword import OnePasswordProvider

#: What `op` writes when a service account has been rate-limited. Quoted from #322 — the
#: operator's own output, from an agent invoking the path charter invokes. The process
#: exits 1, the same code as "no such item", which is why the exit status cannot classify
#: it and why a rate limit is enough to trigger everything below.
RATE_LIMITED = "[ERROR] 2026/08/20 11:02:31 Too many requests. Your client has been rate-limited."

#: What `op` writes for an item that is not there. charter deliberately does not match on
#: this text — absence is proven by a successful listing — so nothing depends on its wording.
NO_SUCH_ITEM = '[ERROR] 2026/08/20 11:02:31 "charter-devops" isn\'t an item.'

ITEM = "charter-devops"

#: A field charter did NOT create: 1Password generated the id, and the human chose only the
#: label. Renumbering this to "PROD_KUBECONFIG" is the silent mutation under test, so the
#: id has to be one no key name could coincidentally equal.
ADOPTED_ID = "27r3gphb4fnsonx5ikcaw3cxwq"


class FakeOp:
    """`op`, with failure injectable **per `item get` call index**.

    Distinct from the fakes in `test_onepassword_single_item` (fails whatever a substring
    matches) and `test_op_unreadable_vault_is_not_empty` (fails a whole subcommand). Neither
    can express the distinction this file exists for: `item get` is the subcommand of all
    three reads, so failing "item get" wholesale makes `_fields` raise and the window is
    never entered. What matters here is *which* of the three failed.
    """

    def __init__(self, *, raw_fields=None, titles=(ITEM,), fail_get_from=None,
                 bad_json_from=None, fail_subs=(), stderr=RATE_LIMITED):
        self.calls: list[dict] = []
        #: Fields as 1Password returns them, ids and all.
        self.raw_fields = list(raw_fields or [])
        self.titles = list(titles)
        #: `item get` calls at this index and beyond exit non-zero. A rate limit does not
        #: clear itself between two calls a moment apart, so "from" models it better than
        #: a single-call blip — and it still leaves call 0 green, which is the precondition.
        self.fail_get_from = fail_get_from
        #: `item get` calls at this index and beyond exit 0 with output charter cannot parse.
        self.bad_json_from = bad_json_from
        self.fail_subs = set(fail_subs)
        self.stderr = stderr
        self.n_get = 0
        #: The template piped to `op item create`/`item edit`, if a write was reached.
        self.template = None

    @staticmethod
    def _sub(argv) -> str:
        words = [w for w in argv[1:] if not w.startswith("-")]
        if words[:1] == ["item"]:
            return " ".join(words[:2])
        return words[0] if words else ""

    def _item_json(self) -> str:
        return json.dumps({"id": "itm1", "title": ITEM, "category": "PASSWORD",
                           "fields": self.raw_fields})

    def _record(self, sub, proc, input=None):
        self.calls.append({"sub": sub, "ok": proc.returncode == 0, "input": input})
        return proc

    def __call__(self, argv, input=None, check=False, env=None, **kw):
        sub = self._sub(argv)
        if sub in self.fail_subs:
            return self._record(sub, SimpleNamespace(
                returncode=1, stdout="", stderr=self.stderr))
        if sub == "item get":
            i, self.n_get = self.n_get, self.n_get + 1
            if self.fail_get_from is not None and i >= self.fail_get_from:
                return self._record(sub, SimpleNamespace(
                    returncode=1, stdout="", stderr=self.stderr))
            if ITEM not in self.titles:
                return self._record(sub, SimpleNamespace(
                    returncode=1, stdout="", stderr=NO_SUCH_ITEM))
            if self.bad_json_from is not None and i >= self.bad_json_from:
                # op exited 0 and produced something charter cannot read: a truncated
                # body, a proxy's error page. charter's inability to READ the answer is
                # still not evidence about the item's contents.
                return self._record(sub, SimpleNamespace(
                    returncode=0, stdout="{not json at all", stderr=""))
            return self._record(sub, SimpleNamespace(
                returncode=0, stdout=self._item_json(), stderr=""))
        if sub == "item list":
            return self._record(sub, SimpleNamespace(
                returncode=0,
                stdout=json.dumps([{"title": t} for t in self.titles]), stderr=""))
        if sub in ("item create", "item edit"):
            self.template = json.loads(input or "{}")
            self.raw_fields = list(self.template.get("fields", []))
            if ITEM not in self.titles:
                self.titles.append(ITEM)
            return self._record(sub, SimpleNamespace(
                returncode=0, stdout=self._item_json(), stderr=""), input=input)
        if sub == "read":
            key = [w for w in argv if w.startswith("op://")][0].rsplit("/", 1)[1]
            vals = {f.get("label") or f.get("id"): f.get("value", "")
                    for f in self.raw_fields}
            if key not in vals:
                return self._record(sub, SimpleNamespace(
                    returncode=1, stdout="", stderr=NO_SUCH_ITEM))
            return self._record(sub, SimpleNamespace(
                returncode=0, stdout=vals[key], stderr=""))
        return self._record(sub, SimpleNamespace(returncode=0, stdout="", stderr=""))

    def subs(self) -> list[str]:
        return [c["sub"] for c in self.calls]

    def gets(self) -> list[dict]:
        return [c for c in self.calls if c["sub"] == "item get"]

    def written_ids(self) -> dict:
        """`{label: id}` from the template op was actually piped."""
        return {f.get("label"): f.get("id") for f in (self.template or {}).get("fields", [])}


class OpCase(unittest.TestCase):
    """`op` is not on CI's PATH and the provider checks before running, so PATH is pinned
    rather than inherited — same reason as the two sibling op test modules."""

    ADOPTED = [{"id": ADOPTED_ID, "label": "PROD_KUBECONFIG",
                "type": "CONCEALED", "value": "body"}]

    def setUp(self) -> None:
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))
        patch = mock.patch.dict("os.environ", {"OP_ENG_DEVOPS_TOKEN": "ops-not-a-real-token"})
        patch.start()
        self.addCleanup(patch.stop)

    def make(self, **kw):
        kw.setdefault("raw_fields", [dict(f) for f in self.ADOPTED])
        op = FakeOp(**kw)
        p = OnePasswordProvider("devops", {
            "op-vault": "Eng",
            "env": {"OP_SERVICE_ACCOUNT_TOKEN": "OP_ENG_DEVOPS_TOKEN"},
        })
        p.runner = op
        return op, p

    def assertFirstReadSucceeded(self, op):
        """The precondition every assertion in this file rests on.

        Without it a test could pass because `op` failed from the very first call — which
        is #352's case, already fixed, and would make these assertions vacuous.
        """
        gets = op.gets()
        self.assertGreaterEqual(len(gets), 2,
                                "precondition: the write path must reach a SECOND "
                                "`op item get`; only one was made")
        self.assertTrue(gets[0]["ok"],
                        "precondition: the FIRST `op item get` must succeed, otherwise "
                        "_fields raises and the window under test is never entered")
        self.assertFalse(gets[-1]["ok"],
                         "precondition: a LATER `op item get` must fail")


class TheFixtureReallyHasAnIdToLose(OpCase):
    """The control. Same construction as every failing case below, minus the failure.

    If the adopted id were not preserved here, or the item had no fields, then "the ids
    were not renumbered" would be true of an empty item and every assertion below would
    be vacuous.
    """

    def test_the_adopted_id_survives_a_write_when_op_answers(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(op.written_ids()["PROD_KUBECONFIG"], ADOPTED_ID)

    def test_a_new_field_still_takes_the_key_as_its_id(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(op.written_ids()["GITHUB_TOKEN"], "GITHUB_TOKEN")

    def test_the_write_path_really_makes_three_item_get_calls(self):
        """The window exists. If this ever drops to one (#355), the `fail_get_from`
        indices below stop meaning what their comments say and must be revisited."""
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(len(op.gets()), 3, op.subs())


class AFailedIdReadIsReportedRatherThanRenumbering(OpCase):
    """`_existing_ids` — the defect #354 was filed for."""

    def test_set_raises_rather_than_returning_successfully(self):
        # Calls 0 (_fields) and 1 (_item_present) succeed; call 2 (_existing_ids) fails.
        op, p = self.make(fail_get_from=2)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertFirstReadSucceeded(op)

    def test_no_write_reaches_op_with_renumbered_ids(self):
        op, p = self.make(fail_get_from=2)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIsNone(op.template,
                          "a write was piped to op despite the id read having failed")
        self.assertNotIn("item edit", op.subs())

    def test_the_failure_names_the_rate_limit_rather_than_guessing_at_tokens(self):
        op, p = self.make(fail_get_from=2)
        with self.assertRaises(base.VaultError) as raised:
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIn("rate-limited", str(raised.exception))

    def test_delete_raises_too(self):
        # delete() has no presence check, so _existing_ids is `item get` call 1 there.
        op, p = self.make(fail_get_from=1)
        with self.assertRaises(base.VaultError):
            p.delete("PROD_KUBECONFIG")
        self.assertFirstReadSucceeded(op)
        self.assertIsNone(op.template)


class AnUnparseableItemIsNotReadAsNoIds(OpCase):
    """`op` exited 0 and charter could not parse what it said.

    `_fields` raises `VaultError` for exactly this condition eight lines up. `_existing_ids`
    swallowed it, so the same bytes got two different answers. That charter cannot READ the
    answer is not evidence about the item's contents either.
    """

    def test_set_raises_rather_than_renumbering(self):
        op, p = self.make(bad_json_from=2)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        gets = op.gets()
        self.assertTrue(gets[0]["ok"] and gets[-1]["ok"],
                        "precondition: every `op item get` here EXITS 0 — the failure "
                        "under test is the parse, not the process")
        self.assertIsNone(op.template)


class AFailedPresenceCheckIsNotAnAbsentItem(OpCase):
    """`_item_present` — the fourth site, and the worst consequence of the set.

    A failed read reading as *absent* flips `_write` to `item create` against a title that
    already exists, leaving two same-titled items in the vault.
    """

    def test_set_raises_rather_than_creating_a_duplicate_item(self):
        # Call 0 (_fields) succeeds; call 1 (_item_present) fails.
        op, p = self.make(fail_get_from=1)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertFirstReadSucceeded(op)

    def test_no_second_item_is_created(self):
        op, p = self.make(fail_get_from=1)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertNotIn("item create", op.subs(),
                         "charter created a second item titled the same as the one that "
                         "is already there — the vault is now ambiguous to `op item get`")
        self.assertEqual(op.titles, [ITEM])


class AProvenAbsentItemIsStillCreated(OpCase):
    """The legitimate absence path, which the fix must not break.

    On a vault's first write there is genuinely no item: `op item get` exits non-zero and
    that is the truth. Absence is proven the way #352 proves it — this vault's own identity
    lists the vault, and the item is not in the listing — rather than assumed from an exit
    code that cannot tell absence from failure.
    """

    def test_a_first_write_creates_the_item(self):
        op, p = self.make(raw_fields=[], titles=[])
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIn("item create", op.subs(), op.subs())
        self.assertEqual(op.written_ids(), {"GITHUB_TOKEN": "GITHUB_TOKEN"})

    def test_the_precondition_op_really_refused_the_read(self):
        """Not "op answered with an empty item" — `op item get` exited NON-ZERO, and the
        listing is what proved the item absent."""
        op, p = self.make(raw_fields=[], titles=[])
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertFalse(op.gets()[0]["ok"], "`op item get` was expected to refuse")
        self.assertIn("item list", op.subs(), "absence was assumed, not proven")


class AVaultThatCannotBeListedIsNotAnAbsentItem(OpCase):
    """Absence must be *proven*, and a listing that fails is not proof.

    This is #352's rule applied to the presence check: under a real rate limit the listing
    that would prove absence is limited too, which is exactly what makes absence unprovable
    and the failure the only honest answer.
    """

    def test_set_raises_when_neither_the_read_nor_the_listing_answers(self):
        op, p = self.make(fail_get_from=1, fail_subs={"item list"})
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertTrue(op.gets()[0]["ok"],
                        "precondition: the first read succeeded, so this is not #352")
        self.assertIsNone(op.template)


if __name__ == "__main__":
    unittest.main()
