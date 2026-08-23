"""`set()` and `delete()` ask 1Password for the item ONCE (#355).

`OnePasswordProvider.set()` used to make **three** `op item get` calls, traced on `main`
with a recording runner::

    0: item get --reveal      <- _fields(reveal=True)   what are the values?
    1: item get               <- _item_present()        does the item exist?
    2: item get               <- _existing_ids()        what are the field ids?
    3: item edit
    4: read                   <- set()'s read-back

`delete()` made two (0 and 2). All of them fetch the SAME document, and call 0 already
fetches strictly more than the other two need — it is the same `op item get` plus
`--reveal`.

**Three questions, three answers, and they can disagree.** The template `_write` pipes back
pairs *this* fetch's values with *that* fetch's ids, so a concurrent edit landing between
call 0 and call 2 produces an item description that never existed at any instant. One fetch
cannot disagree with itself. It is also a third of the exposure to the rate limiting that
#322 and #354 were both reported from, on the write path, where being rate-limited is most
expensive.

**Collapsing them is only safe if the surviving answer is the RIGHT one.** Three things the
split reads were doing that a single read has to keep doing, each pinned below:

1. **`--reveal` belongs to the write path and nowhere else.** The values must come from the
   revealed fetch; `op item get --format json` conceals them, and round-tripping a concealed
   item replaces every sibling secret with a mask. Collapsing onto the *unrevealed* document
   would make the three answers agree by caching the one that destroys the vault — worse
   than the bug. `TheWrittenValuesAreTheRealOnes` fails if that happens, against a fake that
   models the concealment.
2. **`keys()`/`health()` must still NOT reveal.** `vault list` and `doctor` call them
   routinely; a shared read that always revealed would pull every secret in the vault into
   memory on a listing and could prompt for re-auth each time.
3. **"absent" and "present with no fields" are different answers.** `_item_present` existed
   to draw exactly that distinction, which `_fields`' return value cannot express — `{}`
   means both. The single read has to keep expressing it, because it chooses between
   `op item create` and `op item edit`, and getting it wrong either creates a duplicate
   item (leaving the vault ambiguous to `op item get` until a human deletes one) or edits
   an item that is not there.

Fixture values here are inert strings, never credentials, and nothing below asserts on a
value that reached an error message.
"""

from __future__ import annotations

import json
import unittest
from types import SimpleNamespace

from charter.secrets import base
from charter.secrets.onepassword import OnePasswordProvider

ITEM = "charter-devops"

#: What `op` writes for an item that is not there. charter deliberately does not match on
#: this text — absence is proven by a successful listing — so nothing depends on its wording.
NO_SUCH_ITEM = '[ERROR] 2026/08/20 11:02:31 "charter-devops" isn\'t an item.'

#: A field charter did NOT create: 1Password generated the id, the human chose the label.
#: Renumbering it is the silent mutation #354 was filed for, so the id must be one no key
#: name could coincidentally equal.
ADOPTED_ID = "27r3gphb4fnsonx5ikcaw3cxwq"
ADOPTED_LABEL = "PROD_KUBECONFIG"
ADOPTED_VALUE = "kubeconfig-body-fixture"

#: Stands in for whatever `op item get --format json` puts in a concealed field's `value`
#: when `--reveal` was not asked for. A MODEL of the behaviour this module's own docstring
#: records — "conceals values, so a naive round-trip would write masks back over every
#: sibling secret" — not a transcript of a real `op`; there is no 1Password vault on this
#: machine to record one from. Nothing asserts on its text, only that it differs from the
#: real value, so a wrong guess at op's exact mask cannot make these tests pass or fail.
MASK = "<concealed by op>"


class FakeOp:
    """`op`, with concealment and a hook for another writer landing mid-operation.

    Distinct from the three existing op fakes, which fail by substring
    (`test_onepassword_single_item`), by subcommand (`test_op_unreadable_vault_is_not_empty`)
    or by `item get` index (`test_op_unreadable_item_is_not_renumbered`). None of them
    models the two things this file is about: that `op` answers `item get` *differently*
    depending on `--reveal`, and that the document can CHANGE between two of charter's
    reads without any call failing.
    """

    def __init__(self, *, raw_fields=None, titles=(ITEM,), conceal=False, after=None,
                 get_body=None):
        self.calls: list[dict] = []
        #: Fields of the item as 1Password holds them, ids and all.
        self.raw_fields = [dict(f) for f in (raw_fields or [])]
        #: Item titles this identity can see in the vault.
        self.titles = list(titles)
        #: Whether an unrevealed `item get` masks CONCEALED values, as op does.
        self.conceal = conceal
        #: Verbatim stdout for a *successful* `item get`, for the bodies charter must
        #: refuse. `op` exiting 0 with something that is not an object is the one way a
        #: successful read could otherwise reach the "proven absent" sentinel.
        self.get_body = get_body
        #: ``(fake, sub, index) -> None``, run AFTER each call is answered. Models another
        #: writer landing between two of charter's reads — the window the three-read
        #: version of `set()` left open, and the whole point of collapsing them.
        self.after = after
        self.n = {}
        #: The template piped to `op item create`/`item edit`, if a write was reached.
        self.template = None
        #: `item create` calls made against a title the vault ALREADY holds. 1Password
        #: permits duplicate titles; the item that was there is left untouched.
        self.duplicates = 0

    @staticmethod
    def _sub(argv) -> str:
        words = [w for w in argv[1:] if not w.startswith("-")]
        return " ".join(words[:2]) if words[:1] == ["item"] else (words[0] if words else "")

    def _body(self, reveal: bool) -> str:
        fields = []
        for f in self.raw_fields:
            g = dict(f)
            if self.conceal and not reveal and g.get("type") == "CONCEALED" and "value" in g:
                g["value"] = MASK
            fields.append(g)
        return json.dumps({"id": "itm1", "title": ITEM, "category": "PASSWORD",
                           "fields": fields})

    def __call__(self, argv, input=None, check=False, env=None, **kw):
        sub = self._sub(argv)
        reveal = "--reveal" in argv
        i = self.n.get(sub, 0)
        self.n[sub] = i + 1
        rec = {"sub": sub, "reveal": reveal, "argv": list(argv), "input": input}
        self.calls.append(rec)
        proc = self._answer(sub, reveal, input, argv)
        if sub == "item get" and proc.returncode == 0:
            # NAMES only. What a read saw is the whole question here, and a fixture's
            # values have no business being kept around to answer it.
            body = json.loads(proc.stdout)
            rec["labels"] = [f.get("label") or f.get("id")
                             for f in (body.get("fields", [])
                                       if isinstance(body, dict) else [])]
        if self.after:
            self.after(self, sub, i)
        return proc

    def _answer(self, sub, reveal, input, argv):
        if sub == "item get":
            if ITEM not in self.titles:
                return SimpleNamespace(returncode=1, stdout="", stderr=NO_SUCH_ITEM)
            if self.get_body is not None:
                return SimpleNamespace(returncode=0, stdout=self.get_body, stderr="")
            return SimpleNamespace(returncode=0, stdout=self._body(reveal), stderr="")
        if sub == "item list":
            return SimpleNamespace(
                returncode=0,
                stdout=json.dumps([{"title": t} for t in self.titles]), stderr="")
        if sub in ("item create", "item edit"):
            self.template = json.loads(input or "{}")
            if sub == "item create" and ITEM in self.titles:
                # A second item with the same title. The one already there is untouched,
                # and `op item get <title>` can no longer say which is meant.
                self.duplicates += 1
            else:
                self.raw_fields = [dict(f) for f in self.template.get("fields", [])]
                if ITEM not in self.titles:
                    self.titles.append(ITEM)
            return SimpleNamespace(returncode=0, stdout=self._body(True), stderr="")
        if sub == "read":
            if self.duplicates:
                # Ambiguous: two items carry this title.
                return SimpleNamespace(returncode=1, stdout="", stderr=NO_SUCH_ITEM)
            key = [w for w in argv if w.startswith("op://")][0].rsplit("/", 1)[1]
            vals = {f.get("label") or f.get("id"): f.get("value", "")
                    for f in self.raw_fields}
            if key not in vals:
                return SimpleNamespace(returncode=1, stdout="", stderr=NO_SUCH_ITEM)
            return SimpleNamespace(returncode=0, stdout=vals[key], stderr="")
        return SimpleNamespace(returncode=0, stdout="", stderr="")

    # --- what a test looks at ---------------------------------------------- #
    def subs(self) -> list[str]:
        return [c["sub"] for c in self.calls]

    def gets(self) -> list[dict]:
        return [c for c in self.calls if c["sub"] == "item get"]

    def written(self) -> list[dict]:
        return (self.template or {}).get("fields", [])

    def written_ids(self) -> dict:
        return {f.get("label"): f.get("id") for f in self.written()}

    def value_of(self, label: str):
        """The value the item HOLDS now, after whatever write was made."""
        return {f.get("label") or f.get("id"): f.get("value")
                for f in self.raw_fields}.get(label)


class OpCase(unittest.TestCase):
    """`op` is not on CI's PATH and the provider checks before running, so PATH is pinned
    rather than inherited — the same reason the three sibling op test modules do it."""

    ADOPTED = [{"id": ADOPTED_ID, "label": ADOPTED_LABEL,
                "type": "CONCEALED", "value": ADOPTED_VALUE}]

    def setUp(self) -> None:
        import charter.secrets.onepassword as mod
        real = mod.shutil.which
        mod.shutil.which = lambda n: "/usr/local/bin/op" if n == "op" else None
        self.addCleanup(lambda: setattr(mod.shutil, "which", real))

    def make(self, **kw):
        kw.setdefault("raw_fields", [dict(f) for f in self.ADOPTED])
        op = FakeOp(**kw)
        # No `env` binding: `env_overlay` would then read this process's environment, and
        # the suite runs inside a charter frame where such a variable can already be set.
        p = OnePasswordProvider("devops", {"op-vault": "Eng"})
        p.runner = op
        return op, p

    def attempt(self, fn, *a):
        """Run a write and swallow a vault failure, returning it.

        The assertions that follow are about the `op` calls charter made, not about
        whether it raised. Wrapping them in `assertRaises` instead would make the test go
        red on the raise before it ever reached the property under test — and the raise is
        a *consequence* of the fix, not the thing being fixed.
        """
        try:
            return fn(*a)
        except base.VaultError as e:
            return e


class TheFixtureReallyWrites(OpCase):
    """The control. Every assertion below is about HOW the write path reads; if the write
    path did not work at all they would hold vacuously."""

    def test_a_set_lands_the_field(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(op.value_of("GITHUB_TOKEN"), "ghp-fixture")

    def test_a_set_keeps_the_sibling(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(op.value_of(ADOPTED_LABEL), ADOPTED_VALUE)

    def test_a_delete_removes_the_field(self):
        op, p = self.make(raw_fields=self.ADOPTED + [
            {"id": "GITHUB_TOKEN", "label": "GITHUB_TOKEN",
             "type": "CONCEALED", "value": "ghp-fixture"}])
        p.delete("GITHUB_TOKEN")
        self.assertIsNone(op.value_of("GITHUB_TOKEN"))


class TheWritePathReadsTheItemOnce(OpCase):
    """The redundancy itself. Three code paths answering the same question is the failure
    mode most of this repo's bugs have turned out to be, and here it is also three chances
    to be rate-limited on the one path where that is most expensive."""

    def test_set_makes_exactly_one_item_get(self):
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(len(op.gets()), 1, op.subs())

    def test_delete_makes_exactly_one_item_get(self):
        op, p = self.make()
        p.delete(ADOPTED_LABEL)
        self.assertEqual(len(op.gets()), 1, op.subs())

    def test_the_write_path_never_reads_the_item_unrevealed(self):
        """The values must come from a revealed read. A second, unrevealed fetch of the
        same document is the thing that has no reason to exist — and the thing a careless
        collapse would keep, throwing away the revealed one."""
        op, p = self.make()
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual([c["reveal"] for c in op.gets()], [True], op.subs())

    def test_the_read_path_still_never_reveals(self):
        """`keys()` and `health()` run from `vault list` and `doctor`. Sharing one read
        with the write path must not drag `--reveal` onto them: that would pull every
        secret in the vault into memory on a listing, and can prompt for re-auth."""
        op, p = self.make()
        p.keys()
        p.health()
        self.assertEqual(len(op.gets()), 2, op.subs())
        self.assertEqual([c["reveal"] for c in op.gets()], [False, False])


class OneDocumentCannotDisagreeWithItself(OpCase):
    """The reason the redundancy is worth removing rather than merely tidying.

    Nothing here fails. Every `op` call exits 0, and charter still writes back an item
    description that describes no instant that ever existed — because the ids came from a
    different fetch than the values they are paired with.
    """

    def race(self):
        """Another writer creates the item, with a secret in it, the moment after charter
        has proved the item ABSENT.

        The hook fires on the listing rather than on the first `op item get`, because the
        listing is what proves absence: landing it any earlier would make charter see the
        item in the listing and report a failed read instead, which is a different case.

        `self.landed` records that it really fired. Without that, a hook wired to a
        subcommand nobody calls would leave `assertNotIn("item edit", …)` true for the
        boring reason that there was no race at all.
        """
        self.landed: list[int] = []

        def concurrent_create(op, sub, i):
            if sub == "item list" and i == 0:
                op.titles.append(ITEM)
                op.raw_fields = [{"id": "OTHER_SECRET", "label": "OTHER_SECRET",
                                  "type": "CONCEALED", "value": "other-fixture"}]
                self.landed.append(i)

        return self.make(raw_fields=[], titles=[], after=concurrent_create)

    def test_a_rename_between_the_two_reads_does_not_renumber_an_adopted_field(self):
        """A human renames the field in the 1Password UI while `set()` is running.

        Fetch 0 sees ``{id: <adopted>, label: PROD_KUBECONFIG}``; the later fetch sees the
        same id under the NEW label. `_existing_ids` is then keyed by the new label, the
        values are keyed by the old one, so charter finds no id for the field it is
        writing and mints a fresh one — silently renumbering a field on an item it does
        not own, with no failure anywhere. That is exactly the #354 mutation, reached
        without a rate limit and without a failed call.
        """
        def rename(op, sub, i):
            if sub == "item get" and i == 0:
                op.raw_fields = [{**f, "label": "PROD_KUBECONFIG_OLD"}
                                 for f in op.raw_fields]

        op, p = self.make(after=rename)
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(op.written_ids().get(ADOPTED_LABEL), ADOPTED_ID,
                         "the id written for this field came from a different fetch than "
                         "its value did")

    def test_the_precondition_the_rename_really_lands_mid_operation(self):
        """Otherwise the test above asserts nothing: if the hook never fired, or fired
        before the first read, the two fetches would agree and there would be no
        disagreement to be robust against."""
        seen = []

        def rename(op, sub, i):
            if sub == "item get" and i == 0:
                op.raw_fields = [{**f, "label": "PROD_KUBECONFIG_OLD"}
                                 for f in op.raw_fields]
                seen.append(i)

        op, p = self.make(after=rename)
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(seen, [0], "the concurrent rename never happened")
        self.assertEqual(op.gets()[0]["labels"], [ADOPTED_LABEL],
                         "fetch 0 was expected to see the ORIGINAL label")

    def test_an_item_proved_absent_is_never_edited(self):
        """The presence answer, disagreeing with the values answer.

        The item is genuinely absent when `set()` reads it, and absence is PROVEN — the
        listing this identity can still make does not contain it. Another writer creates
        it a moment later. With a second read to ask "does it exist?", charter now gets
        *yes*, flips to `op item edit`, and an edit REPLACES: the template it pipes holds
        only the key being written, so the other writer's secret is gone and `set()`
        returns success.

        With one read there is no second answer to contradict the first. charter creates,
        which against a title now taken leaves two same-titled items — bad, loud, and
        recoverable (`set()`'s read-back can no longer resolve the field, so it raises).
        That window is not new and is not widened here: the same race exists between any
        presence check and the `op item create` that follows it. What the single read
        removes is charter *destroying another writer's secret while reporting success*.
        """
        op, p = self.race()
        self.attempt(p.set, "NEW_KEY", "v-fixture")
        self.assertNotIn("item edit", op.subs(),
                         "charter edited — which replaces — an item it had just proved "
                         "absent, on the strength of a second read that disagreed")
        self.assertEqual(self.landed, [0], "the other writer never landed")

    def test_the_other_writers_secret_is_not_replaced_away(self):
        """The consequence of the assertion above, stated as the damage."""
        op, p = self.race()
        self.attempt(p.set, "NEW_KEY", "v-fixture")
        self.assertEqual(op.value_of("OTHER_SECRET"), "other-fixture",
                         "the other writer's secret was replaced away, and on `main` "
                         "`set()` returned success while doing it")

    def test_the_precondition_the_item_really_was_absent_and_proved_so(self):
        """Not "op answered with an empty item": `op item get` exited NON-ZERO, and the
        listing is what proved the item absent before the other writer landed."""
        op, p = self.race()
        self.attempt(p.set, "NEW_KEY", "v-fixture")
        self.assertIsNone(op.gets()[0].get("labels"),
                          "`op item get` was expected to refuse — the item was absent")
        self.assertIn("item list", op.subs(), "absence was assumed, not proven")
        self.assertEqual(self.landed, [0], "the other writer never landed")


class AbsentAndEmptyStayDifferentAnswers(OpCase):
    """The distinction `_item_present` existed to draw, and the one a collapse is most
    likely to lose. `_fields` returns `{}` for BOTH an item that is not there and an item
    that is there with no fields, so the surviving read cannot answer presence by asking
    whether the fields are empty.

    Getting it wrong in one direction creates a second item with a title the vault already
    holds — after which `op item get <title>` is ambiguous and the vault is unreadable
    until a human deletes one by hand. In the other it edits an item that is not there.
    """

    def test_an_item_that_exists_with_no_fields_is_edited(self):
        op, p = self.make(raw_fields=[], titles=[ITEM])
        # The subcommand is the subject, so it is asserted before the outcome: answering
        # presence by `not fields` sends this down `item create`, and the read-back then
        # fails against the ambiguous title. Wrapped in `assertRaises` the test would go
        # red on that read-back and never name what actually went wrong.
        outcome = self.attempt(p.set, "FIRST", "v-fixture")
        self.assertIn("item edit", op.subs(),
                      "charter created a SECOND item with a title the vault already "
                      "holds: it read 'this document has no fields' as 'there is no "
                      "document', which `_fields`' `{}` cannot distinguish")
        self.assertNotIn("item create", op.subs())
        self.assertIsNone(outcome, "the write itself did not land")

    def test_the_precondition_that_item_really_reads_as_empty_and_present(self):
        """If the fixture's item had fields, "present" and "non-empty" would coincide and
        the test above could not tell a presence check from a truthiness check. And if the
        item were absent rather than empty, it would be testing the other branch."""
        op, p = self.make(raw_fields=[], titles=[ITEM])
        self.assertEqual(p.keys(), [], "the fixture item is not empty")
        self.assertEqual(op.gets()[0]["labels"], [])
        self.assertNotIn("item list", op.subs(),
                         "`op item get` failed, so this fixture's item is ABSENT rather "
                         "than present-and-empty and the distinction is not under test")

    def test_an_item_that_is_not_there_is_created(self):
        op, p = self.make(raw_fields=[], titles=[])
        p.set("FIRST", "v-fixture")
        self.assertIn("item create", op.subs(), op.subs())
        self.assertNotIn("item edit", op.subs())


class ASuccessfulReadCanNeverMeanAbSENT(OpCase):
    """The one hazard the collapse INTRODUCED, rather than removed.

    "Proven absent" is now a `None` returned from the read, so a successful `op item get`
    whose body happens to parse to `null` would hand the write path *there is no item* on
    the strength of a read that succeeded — and `set` would `op item create` against a
    title the vault already holds, which is the duplicate-item outcome `_item_present` was
    hardened against in #354. Before the collapse `None` had no meaning here and a body
    like that would have raised an `AttributeError`: ugly, but loud.

    A sentinel reachable from a success is not a sentinel, so this is checked in
    `_document` rather than reasoned about. `op` is not expected to do this; a truncated
    body, a proxy's error page or a `null` from a future format change are, and the cost
    of being wrong is a vault a human has to repair by hand.
    """

    def test_a_null_body_is_refused_rather_than_read_as_absence(self):
        op, p = self.make(get_body="null")
        with self.assertRaises(base.VaultError):
            p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertNotIn("item create", op.subs(),
                         "a successful read was treated as proof the item is absent")
        self.assertNotIn("item edit", op.subs())

    def test_the_precondition_that_read_really_succeeded(self):
        """Not a failed read, which is #352's case and already covered. `op` exited 0."""
        op, p = self.make(get_body="null")
        self.attempt(p.set, "GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(op.gets()[0]["labels"], [],
                         "`op item get` was expected to EXIT 0 here")
        self.assertNotIn("item list", op.subs(),
                         "the absence proof ran, so the read did not succeed")

    def test_a_list_body_is_refused_too(self):
        op, p = self.make(get_body="[]")
        with self.assertRaises(base.VaultError):
            p.keys()

    def test_the_failure_does_not_echo_ops_output(self):
        """`op`'s stdout on this path IS the item. A body charter could not read is still
        a body that may hold values, so the error names the item and nothing else — the
        same rule `_fail` follows for stderr, applied to the one path where charter has
        the bytes in hand and could so easily quote them."""
        op, p = self.make(get_body='"s3cret-value"')
        with self.assertRaises(base.VaultError) as raised:
            p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertNotIn("s3cret-value", str(raised.exception))
        self.assertIn("charter-devops", str(raised.exception))


class TheWrittenValuesAreTheRealOnes(OpCase):
    """The failure that would be WORSE than the bug: collapsing onto the unrevealed
    document. All three answers would then agree, and every sibling secret in the vault
    would be replaced by a mask on the next write — the exact failure the earlier
    one-item-per-key design cited when it rejected this schema.

    The fake models the concealment for these tests only. The three existing op fakes
    return real values whether or not `--reveal` was asked for, so `--reveal` could be
    dropped from the write path entirely and every one of their assertions would still
    hold. That is a fixture coincidence, and this class exists to close it.
    """

    def test_the_precondition_the_fake_really_conceals_without_reveal(self):
        op, _ = self.make(conceal=True)
        argv = ["op", "item", "get", ITEM, "--vault", "Eng", "--format", "json"]
        plain = json.loads(op(argv).stdout)["fields"][0]["value"]
        revealed = json.loads(op(argv + ["--reveal"]).stdout)["fields"][0]["value"]
        self.assertEqual(revealed, ADOPTED_VALUE)
        self.assertNotEqual(plain, revealed,
                            "the fixture does not conceal, so nothing below is tested")

    def test_a_sibling_keeps_its_real_value_across_a_write(self):
        op, p = self.make(conceal=True)
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertEqual(op.value_of(ADOPTED_LABEL), ADOPTED_VALUE)

    def test_no_mask_is_ever_piped_back_to_op(self):
        op, p = self.make(conceal=True)
        p.set("GITHUB_TOKEN", "ghp-fixture")
        self.assertNotIn(MASK, json.dumps(op.written()))

    def test_a_delete_does_not_mask_the_survivors_either(self):
        op, p = self.make(conceal=True, raw_fields=self.ADOPTED + [
            {"id": "GITHUB_TOKEN", "label": "GITHUB_TOKEN",
             "type": "CONCEALED", "value": "ghp-fixture"}])
        p.delete("GITHUB_TOKEN")
        self.assertEqual(op.value_of(ADOPTED_LABEL), ADOPTED_VALUE)


if __name__ == "__main__":
    unittest.main()
