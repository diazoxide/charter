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

**The window these tests were written against is gone (#355), and this file was rewritten
when it closed.** It used to say:

    The window is the whole point, and it is what makes these tests non-vacuous. `set()`
    issues *three* separate `op item get` calls — `_fields`, then `_item_present`, then
    `_existing_ids` — all fetching the same document (#355).

and every case below failed a *later* `op item get` after the first had succeeded, with
`assertFirstReadSucceeded` guarding that the window had really been entered. It also
carried the note that "if this ever drops to one (#355), the `fail_get_from` indices below
stop meaning what their comments say and must be revisited". #355 dropped it to one, so
they are revisited here rather than deleted.

What the collapse changes, and what it does not:

* **(a)** `_existing_ids` returning `{}` on a non-zero exit is now *unreachable*, not
  merely handled — there is no second fetch to fail. The property is preserved by
  construction rather than by a check, and `ThereIsOnlyOneReadToSwallow` pins the
  construction so it cannot quietly come back.
* **(b)** the parse swallow is still a live risk, because there is still a parse. Pinned
  below at the single read.
* **(c)** the presence swallow is still a live risk, because presence is still an answer
  that a failed read must not be allowed to give. Pinned below at the single read.

So (b) and (c) keep their tests, moved to `item get` index **0** — which is now the only
index there is. (a) keeps a structural test instead of a behavioural one, which is the
honest replacement: there is no longer a failure to observe, and asserting that a call
charter never makes did not misbehave would be a test that cannot fail.
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
    matches) and `test_op_unreadable_vault_is_not_empty` (fails a whole subcommand).

    The per-index knob was what this file existed for: `item get` was the subcommand of
    all three reads, so failing "item get" wholesale made `_fields` raise and the window
    under test was never entered — what mattered was *which* of the three failed. #355
    collapsed them, so every index used below is now 0. The knob is kept rather than
    simplified away: if a second `op item get` ever reappears on the write path, this is
    the fake that can say which one broke, and `ThereIsOnlyOneReadToSwallow` is the test
    that will notice it did.
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

    def assertTheOnlyReadFailed(self, op):
        """The precondition every failure case in this file rests on, after #355.

        Its predecessor `assertFirstReadSucceeded` demanded a second `op item get` and
        that the first had succeeded — the window. With one read there is no such window,
        and what has to be established instead is that the read charter *does* make really
        ran and really failed, rather than the write path having bailed out earlier (a
        missing `op-vault`, an unset identity) and the assertion passing for a reason that
        has nothing to do with a swallowed read.
        """
        gets = op.gets()
        self.assertEqual(len(gets), 1,
                         "precondition: the write path makes exactly one `op item get` "
                         "(#355); a second one means the swallow window is back and this "
                         "file's premise needs revisiting")
        self.assertFalse(gets[0]["ok"],
                         "precondition: that read must have FAILED — otherwise nothing "
                         "was swallowed and the assertion holds vacuously")

    def assertTheOnlyReadSucceeded(self, op):
        """The parse cases: `op` exited 0 and charter could not read what it said."""
        gets = op.gets()
        self.assertEqual(len(gets), 1, "precondition: exactly one `op item get` (#355)")
        self.assertTrue(gets[0]["ok"],
                        "precondition: the read EXITS 0 here — the failure under test is "
                        "the parse, not the process")


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


class ThereIsOnlyOneReadToSwallow(OpCase):
    """(a), structurally. `_existing_ids`' swallow is unreachable rather than handled.

    This class replaces `test_the_write_path_really_makes_three_item_get_calls`, whose
    docstring read: *The window exists. If this ever drops to one (#355), the
    `fail_get_from` indices below stop meaning what their comments say and must be
    revisited.* It dropped to one. The assertion is inverted rather than dropped, because
    the count is what the rest of this file's premise now rests on: a second `op item get`
    would reopen the window that swallowed a failed id read, and nothing else here would
    notice.
    """

    def test_set_makes_exactly_one_item_get(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(len(op.gets()), 1, op.subs())

    def test_delete_makes_exactly_one_item_get(self):
        op, p = self.make()
        p.delete("PROD_KUBECONFIG")
        self.assertEqual(len(op.gets()), 1, op.subs())

    def test_the_ids_written_come_from_that_one_read(self):
        """What `_existing_ids` was for, now answered by the document already in hand.
        The control above proves the id survives; this proves no `op` call fetched it."""
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp_new")
        self.assertEqual(op.written_ids()["PROD_KUBECONFIG"], ADOPTED_ID)
        self.assertEqual(op.subs().count("item get"), 1, op.subs())


class AFailedIdReadIsReportedRatherThanRenumbering(OpCase):
    """`_existing_ids` — the defect #354 was filed for.

    Its fetch is gone; the read that now supplies both the ids and the values is `item
    get` **0**. So the failure that used to be swallowed into "no ids to keep" is the
    failure of that read, and it must still stop the write rather than renumber. The
    indices moved from 2 (and 1 for `delete`) to 0, which is where they were always going
    to end up once the three reads became one.
    """

    def test_set_raises_rather_than_returning_successfully(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertTheOnlyReadFailed(op)

    def test_no_write_reaches_op_with_renumbered_ids(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIsNone(op.template,
                          "a write was piped to op despite the id read having failed")
        self.assertNotIn("item edit", op.subs())

    def test_the_failure_names_the_rate_limit_rather_than_guessing_at_tokens(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError) as raised:
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIn("rate-limited", str(raised.exception))

    def test_delete_raises_too(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError) as raised:
            p.delete("PROD_KUBECONFIG")
        # `SecretNotFound` IS a `VaultError`, and `delete` raises it for a key that is not
        # among the fields. A swallowed read hands `delete` an empty field set, so the
        # bare `assertRaises(VaultError)` this used to be would have been satisfied by
        # exactly the swallow it exists to forbid — "no such secret" reported about a
        # vault charter could not read. The read failure has to be what surfaces.
        self.assertNotIsInstance(raised.exception, base.SecretNotFound,
                                 "the failed read was reported as a missing secret")
        self.assertTheOnlyReadFailed(op)
        self.assertIsNone(op.template)


class AnUnparseableItemIsNotReadAsNoIds(OpCase):
    """`op` exited 0 and charter could not parse what it said.

    `_fields` raised `VaultError` for exactly this condition eight lines up. `_existing_ids`
    swallowed it, so the same bytes got two different answers. That charter cannot READ the
    answer is not evidence about the item's contents either.

    Still a live risk after #355 — the second *parse* went with the second fetch, but there
    is still a parse, and it must still refuse rather than report "no ids".
    """

    def test_set_raises_rather_than_renumbering(self):
        op, p = self.make(bad_json_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertTheOnlyReadSucceeded(op)
        self.assertIsNone(op.template)

    def test_delete_raises_rather_than_renumbering(self):
        op, p = self.make(bad_json_from=0)
        with self.assertRaises(base.VaultError) as raised:
            p.delete("PROD_KUBECONFIG")
        # See `test_delete_raises_too`: a swallowed parse leaves `delete` with no fields,
        # and the `SecretNotFound` that follows would satisfy a bare `assertRaises`.
        self.assertNotIsInstance(raised.exception, base.SecretNotFound,
                                 "the unreadable document was reported as a missing "
                                 "secret rather than as a document charter cannot read")
        self.assertIn("could not parse", str(raised.exception))
        self.assertTheOnlyReadSucceeded(op)
        self.assertIsNone(op.template)

    def test_keys_raises_too_so_the_same_bytes_get_one_answer(self):
        """The read path and the write path now share a parse. The point of #354 was that
        two parses of the same document must not disagree; one parse is how that stops
        being something to remember."""
        op, p = self.make(bad_json_from=0)
        with self.assertRaises(base.VaultError):
            p.keys()


class AFailedPresenceCheckIsNotAnAbsentItem(OpCase):
    """`_item_present` — the fourth site, and the worst consequence of the set.

    A failed read reading as *absent* flips `_write` to `item create` against a title that
    already exists, leaving two same-titled items in the vault.

    Still a live risk after #355. The presence question did not go away with its fetch:
    the single read still has to answer it, and a failed read must still not be allowed
    to answer *absent*. The item is in the listing here, so absence is DISPROVEN and the
    read failure is the only honest answer.
    """

    def test_set_raises_rather_than_creating_a_duplicate_item(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertTheOnlyReadFailed(op)

    def test_no_second_item_is_created(self):
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertNotIn("item create", op.subs(),
                         "charter created a second item titled the same as the one that "
                         "is already there — the vault is now ambiguous to `op item get`")
        self.assertEqual(op.titles, [ITEM])

    def test_the_precondition_the_item_is_visible_to_this_identity(self):
        """Absence is DISPROVEN here, which is what makes the raise required rather than
        merely one of two defensible answers. With the item missing from the listing this
        would be the legitimate creation path two classes down."""
        op, p = self.make(fail_get_from=0)
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertIn("item list", op.subs(), "absence was assumed, not disproven")
        self.assertIn(ITEM, op.titles)


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

    A rate limit not being per-subcommand is also why collapsing the three reads was worth
    doing on its own terms: they were three chances to be limited, on the one path where
    being limited costs the most.
    """

    def test_set_raises_when_neither_the_read_nor_the_listing_answers(self):
        op, p = self.make(fail_get_from=0, fail_subs={"item list"})
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertTheOnlyReadFailed(op)
        self.assertIn("item list", op.subs(),
                      "precondition: absence was assumed rather than the proof attempted")
        self.assertIsNone(op.template)

    def test_no_item_is_created_on_an_unprovable_absence(self):
        op, p = self.make(fail_get_from=0, fail_subs={"item list"})
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp_new")
        self.assertNotIn("item create", op.subs())


if __name__ == "__main__":
    unittest.main()
