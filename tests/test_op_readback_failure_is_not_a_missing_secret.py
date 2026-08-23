"""A read-back charter could not MAKE is never reported as a missing secret (#399).

`OnePasswordProvider.set()` writes the field and then reads it back, because a concurrent
writer can drop it and silently losing a credential somebody believes they stored is worse
than failing. That verification has two failure modes and used to have one message::

    self._write(fields, ids=self._ids_of(doc), creating=doc is None)
    if self.get(key) != value:
        raise VaultError("wrote '<key>' … but reading it back did not return what was
                          written. …")

Only reachable when the read-back **comes back and disagrees**. When the read-back
*fails*, `self.get(key)` raises first — `SecretNotFound`, the right answer on a read path —
and the operator is told the opposite of what happened, about the very key charter stored
a moment ago::

    $ charter secret set devops NEW_KEY …
    no secret 'NEW_KEY' in vault 'devops'
        (field 'NEW_KEY' of 1Password item 'charter-devops' in 'Eng')

That is #322's species — a failure arriving as a benign absence — on the write path, where
the absence is also demonstrably false. `_fields` refuses to call an unreadable vault
empty for the same reason; this is the same rule applied to the write path's own read.

**The failing read-back is not hypothetical.** #355 made the write in the create/create
race unconditionally `op item create`, trading a silent replacement of another writer's
secret for a loud duplicate item — the right trade, and it means two items can now share a
title, after which `op read op://Eng/charter-devops/NEW_KEY` is ambiguous and exits
non-zero. `ADuplicateTitleRace` below drives exactly that race, with every `op` call
before the read-back exiting 0. `AReadBackThatFailsForAnyOtherReason` drives a rate limit,
which is the same shape without the duplicate, so nothing here is keyed to duplicates.

`test_an_item_proved_absent_is_never_edited` in `test_op_reads_the_item_once` reaches this
same race and deliberately swallows the raise — documented there, because that file is
about which `op` calls charter makes. The text the operator ends up reading is this file.

The fake is that module's `FakeOp`, which already models both halves: `op` answering
`item get` differently under `--reveal`, and a writer landing between two of charter's
calls. Fixture values are inert strings, never credentials.
"""

from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_secrets
from charter.secrets import base
from charter.secrets.onepassword import OnePasswordProvider
from tests.test_op_reads_the_item_once import ITEM, FakeOp, OpCase

#: What `op` writes when a service account has been rate-limited — quoted from #322, an
#: operator's own `op` output. It exits 1, the same code as "no such item", which is the
#: whole reason the exit status cannot classify what happened. Nothing asserts on this
#: text; charter must not be matching on `op`'s English here any more than in `_fields`.
RATE_LIMITED = "[ERROR] 2026/08/20 11:02:31 Too many requests. Your client has been rate-limited."

KEY = "NEW_KEY"
VALUE = "v-fixture"


class ReadBackCase(OpCase):
    """The write reaches its read-back, and the read-back cannot be made.

    Every subclass asserts its own preconditions, because both ways this file could pass
    while proving nothing are silent: a fake that never got as far as writing, and a
    read-back that quietly succeeded.
    """

    def failing_set(self, op, p):
        """Run the `set` that must fail, and return the error it raised."""
        with self.assertRaises(base.VaultError) as caught:
            p.set(KEY, VALUE)
        self.assertIn("read", op.subs(), "the read-back never happened")
        return caught.exception

    def with_op(self, cls, **kw):
        """`OpCase.make`, with a subclassed `op` behind it — the three lines are spelled
        out rather than routed through `make`, which would build a fake only to discard it
        and leave two objects that look interchangeable and are not."""
        kw.setdefault("raw_fields", [dict(f) for f in self.ADOPTED])
        op = cls(**kw)
        # No `env` binding, for the reason `OpCase.make` records: `env_overlay` would then
        # read this process's environment, and the suite runs inside a charter frame.
        p = OnePasswordProvider("devops", {"op-vault": "Eng"})
        p.runner = op
        return op, p


class ADuplicateTitleRace(ReadBackCase):
    """Another writer creates the item the moment after charter proved it ABSENT.

    charter creates too — #355's deliberate choice — so the vault now holds two items
    titled `charter-devops` and reading a field by that title is ambiguous. Every `op`
    call up to the read-back exits 0.
    """

    def race(self):
        """The hook fires on the LISTING, which is what proves absence: landing it any
        earlier would make charter see the item and report a failed read instead, which is
        a different case. `self.landed` records that it really fired — a hook wired to a
        subcommand nobody calls would leave every assertion below true for the boring
        reason that there was no race at all."""
        self.landed: list[int] = []

        def concurrent_create(op, sub, i):
            if sub == "item list" and i == 0:
                op.titles.append(ITEM)
                op.raw_fields = [{"id": "OTHER_SECRET", "label": "OTHER_SECRET",
                                  "type": "CONCEALED", "value": "other-fixture"}]
                self.landed.append(i)

        return self.make(raw_fields=[], titles=[], after=concurrent_create)

    def test_the_precondition_the_race_really_produced_the_ambiguity(self):
        """Otherwise this class is about nothing: the other writer must have landed, the
        write must have been a `create` against a title now taken, and no `op` call before
        the read-back may have failed — a failure earlier would raise for its own reasons
        and never reach the read-back at all."""
        op, p = self.race()
        self.failing_set(op, p)
        self.assertEqual(self.landed, [0], "the other writer never landed")
        self.assertEqual(op.duplicates, 1,
                         "the vault does not hold two items with the same title, so the "
                         "read-back had nothing to be ambiguous about")
        self.assertEqual(op.subs(), ["item get", "item list", "item create", "read"])

    def test_the_operator_is_not_told_the_secret_is_missing(self):
        """The bug, stated as the damage. charter has just written the credential."""
        op, p = self.race()
        e = self.failing_set(op, p)
        self.assertNotIn(f"no secret '{KEY}'", str(e),
                         "charter reported the key it had just written as missing")
        self.assertNotIsInstance(
            e, base.SecretNotFound,
            "a caller classifying by exception type is told this vault has no such "
            "secret, moments after charter wrote it")

    def test_the_message_says_what_actually_happened(self):
        op, p = self.race()
        e = self.failing_set(op, p)
        self.assertIn(f"wrote '{KEY}' to 1Password item '{ITEM}'", str(e))
        self.assertIn("could not read it back", str(e))

    def test_the_message_names_the_duplicate_and_how_to_see_it(self):
        """The likeliest cause, with the command that confirms it — not asserted as fact,
        because a rate limit and an expired session reach here identically."""
        op, p = self.race()
        e = self.failing_set(op, p)
        self.assertIn(f"TWO items titled '{ITEM}'", str(e))
        self.assertIn("op item list --vault Eng", str(e))

    def test_the_failure_does_not_echo_ops_output(self):
        """`op`'s stderr can echo the assignment it was given, and on a read path its
        stdout IS the secret. The provider withholds both everywhere else; a new message
        is a new place to leak them."""
        op, p = self.race()
        e = self.failing_set(op, p)
        self.assertNotIn(VALUE, str(e))
        self.assertNotIn("other-fixture", str(e))


class AReadBackThatFailsForAnyOtherReason(ReadBackCase):
    """Same shape, no duplicate item: `op read` simply fails.

    Without this, the guard could be keyed to the duplicate race and still pass — and the
    rate limit reported in #322 and #354 arrives exactly here, on the call that decides
    whether an operator believes their credential was stored.
    """

    class RateLimitsTheReadBack(FakeOp):
        def _answer(self, sub, reveal, input, argv):
            if sub == "read":
                return SimpleNamespace(returncode=1, stdout="", stderr=RATE_LIMITED)
            return super()._answer(sub, reveal, input, argv)

    def rate_limited(self):
        return self.with_op(self.RateLimitsTheReadBack)

    def test_the_precondition_the_write_really_landed(self):
        """The sharpest statement of the bug: the item HOLDS the value, and only the
        read-back failed. There is no duplicate here, so nothing else can explain it."""
        op, p = self.rate_limited()
        self.failing_set(op, p)
        self.assertEqual(op.value_of(KEY), VALUE,
                         "the field never landed, so this is not a read-back failure")
        self.assertEqual(op.duplicates, 0, "a duplicate would be a different case")

    def test_it_is_still_not_reported_as_a_missing_secret(self):
        op, p = self.rate_limited()
        e = self.failing_set(op, p)
        self.assertNotIn(f"no secret '{KEY}'", str(e))
        self.assertNotIsInstance(e, base.SecretNotFound)
        self.assertIn("could not read it back", str(e))

    def test_the_message_does_not_claim_op_said_the_field_is_absent(self):
        """`op read` exiting non-zero means "not there" OR "could not read". charter has
        no way to tell, and says so rather than picking one."""
        op, p = self.rate_limited()
        e = self.failing_set(op, p)
        self.assertIn("cannot tell which happened", str(e))


class ADisagreeingReadBackStillSaysSo(ReadBackCase):
    """The other failure mode, kept distinct.

    The pre-existing message is the right one when the read-back COMES BACK and returns
    something else — a concurrent writer replaced the value. A fix that routed both modes
    through one sentence would satisfy every assertion above and lose the distinction this
    issue is about.
    """

    class AnswersWithSomethingElse(FakeOp):
        def _answer(self, sub, reveal, input, argv):
            proc = super()._answer(sub, reveal, input, argv)
            if sub == "read" and proc.returncode == 0:
                return SimpleNamespace(returncode=0, stdout="somebody-elses-value",
                                       stderr="")
            return proc

    def disagreeing(self):
        return self.with_op(self.AnswersWithSomethingElse)

    def test_the_precondition_the_read_back_really_succeeded(self):
        """It has to COME BACK. A fake whose read merely failed differently would send
        this class through the branch above and assert nothing about disagreement."""
        op, p = self.disagreeing()
        self.failing_set(op, p)
        self.assertEqual(op.value_of(KEY), VALUE, "the write never landed")
        self.assertEqual(p.get(KEY), "somebody-elses-value",
                         "the read-back did not succeed, so nothing disagreed")

    def test_it_reports_disagreement_not_an_unreadable_item(self):
        op, p = self.disagreeing()
        e = self.failing_set(op, p)
        self.assertIn("did not return what was written", str(e))
        self.assertNotIn("could not read it back", str(e))


class TheOrdinaryWriteIsUnchanged(OpCase):
    """The control. A guard that failed every `set` would satisfy the whole file above."""

    def test_a_set_still_succeeds_and_says_nothing(self):
        op, p = self.make()
        p.set(KEY, VALUE)
        self.assertEqual(op.value_of(KEY), VALUE)
        self.assertIn("read", op.subs(), "the read-back was skipped")


class TheOperatorSeesIt(ReadBackCase):
    """What `charter secret set` prints, end to end. `_provider` is stubbed rather than the
    registry populated, so this is about `cmd_secret_set`'s contract with a provider."""

    def _set(self, p) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        args = SimpleNamespace(vault="devops", key=KEY, value=VALUE, from_file=None,
                               stdin=False, allow_empty=False)
        with mock.patch.object(commands_secrets, "_provider", lambda _n: p), \
             redirect_stdout(out), redirect_stderr(err):
            code = commands_secrets.cmd_secret_set(args)
        return code, out.getvalue() + err.getvalue()

    def _race(self):
        def concurrent_create(op, sub, i):
            if sub == "item list" and i == 0:
                op.titles.append(ITEM)
        return self.make(raw_fields=[], titles=[], after=concurrent_create)

    def test_the_command_does_not_tell_the_operator_the_secret_is_missing(self):
        op, p = self._race()
        code, text = self._set(p)
        self.assertNotEqual(code, 0, "a scripted caller would treat the write as done")
        self.assertNotIn(f"no secret '{KEY}'", text)
        self.assertIn("could not read it back", text)
        self.assertEqual(op.duplicates, 1, "the race never produced the ambiguity")

    def test_a_successful_set_still_reports_success(self):
        _, p = self.make()
        code, text = self._set(p)
        self.assertEqual(code, 0)
        self.assertIn("Set 'NEW_KEY'", text)
        self.assertNotIn(VALUE, text, "the command echoed the secret")


if __name__ == "__main__":
    unittest.main()
