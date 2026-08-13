"""A 1Password failure names a cause only when charter recognised one (issue #78).

`_fail` used to staple a fixed sentence to every write failure: *"for a service-account
token, check it has WRITE access; a read-only token fails here with (101)"*. That sentence
was earned exactly once, against a real 101 during a real incident — and then applied
unconditionally, to parse failures, missing vaults and expired sessions alike.

So when `op` rejected a template for an unrelated reason, charter reported a permissions
problem. The reporter went and checked permissions. They had two tokens, one of which
genuinely was read-only, which made the wrong answer look confirmed; they only separated
the two causes by testing each token by hand. The error message cost more time than the
error.

The rule this file pins: **charter may classify a failure it recognises, and must not
assert a cause it has not verified.** An unrecognised failure gets candidates presented AS
candidates, plus the invitation to run `op` directly — because charter withholds output the
user is perfectly entitled to see.

Matching another CLI's English is only safe because of which way it fails. An unmatched
signature costs precision; it can never produce a wrong answer. That is the inverse of the
behaviour it replaces, which was precise and unearned.
"""
from __future__ import annotations

import unittest
from types import SimpleNamespace

from charter.secrets import base
from charter.secrets.onepassword import _diagnose
from tests.test_onepassword_single_item import OpCase

#: Verbatim `op` stderr, each with the provenance that earns its place in `_DIAGNOSES`.
NO_ACCOUNT = "No accounts configured for use with 1Password CLI."          # op 2.34.0, local
FORBIDDEN = "(101) You do not have permission to perform this action"      # real incident
NO_TEMPLATE = "ERROR: provide the item category with '--category' flag"    # #78, op 2.34.0
UNKNOWN = "ERROR: something charter has never seen before"


class TestRecognisedFailures(unittest.TestCase):
    def test_a_missing_account_is_named(self):
        self.assertIn("sign in", (_diagnose(NO_ACCOUNT) or "").lower())

    def test_a_permission_refusal_is_named(self):
        self.assertIn("write", (_diagnose(FORBIDDEN) or "").lower())

    def test_an_unparsed_template_is_named(self):
        d = (_diagnose(NO_TEMPLATE) or "").lower()
        self.assertIn("template", d)

    def test_the_unparsed_template_hint_warns_against_the_obvious_fix(self):
        """`op` asks for `--category`. Supplying it makes op create the item and drop
        every custom field — a credential that looks stored and is not. The message has
        to say so, because the error itself is an invitation to do exactly that."""
        d = (_diagnose(NO_TEMPLATE) or "")
        self.assertIn("--category", d)
        self.assertIn("silently", d.lower())

    def test_matching_ignores_case(self):
        self.assertIsNotNone(_diagnose(FORBIDDEN.upper()))


class TestUnrecognisedFailures(unittest.TestCase):
    def test_an_unknown_failure_is_not_diagnosed(self):
        self.assertIsNone(_diagnose(UNKNOWN))

    def test_empty_stderr_is_not_diagnosed(self):
        self.assertIsNone(_diagnose(""))
        self.assertIsNone(_diagnose(None))


class TestTheDiagnosisCannotCarryTheSecret(unittest.TestCase):
    """`stderr` is matched against, never interpolated. The hint per signature is a
    fixed constant, so there is no code path by which provider output could reach a
    message — the property holds by the shape of the code, not by anyone remembering it.
    """

    def test_no_recognised_hint_echoes_the_stderr_it_matched(self):
        for stderr in (NO_ACCOUNT, FORBIDDEN, NO_TEMPLATE):
            leaky = f"{stderr} while storing s3cret-alpha"
            self.assertNotIn("s3cret-alpha", _diagnose(leaky) or "")

    def test_the_hint_is_the_same_constant_whatever_surrounds_the_signature(self):
        self.assertEqual(_diagnose(FORBIDDEN), _diagnose(f"prefix {FORBIDDEN} suffix"))


class TestTheMessageStopsGuessing(OpCase):
    def message(self, stderr: str, *, write: bool = True) -> str:
        op, p = self.make(fields={"A": "1"}, fail_on="item edit")
        op.fail_stderr = stderr
        with self.assertRaises(base.VaultError) as e:
            p.set("B", "s3cret-beta")
        return str(e.exception)

    def test_a_recognised_permission_failure_still_names_permissions(self):
        """The old message was not wrong about 101 — it was wrong to say it every time.
        Where the signature actually matches, the diagnosis must survive."""
        self.assertIn("write", self.message(FORBIDDEN).lower())

    def test_an_unparsed_template_is_not_reported_as_a_permissions_problem(self):
        """The regression from #78, stated directly."""
        m = self.message(NO_TEMPLATE)
        self.assertIn("template", m.lower())
        self.assertNotIn("read-only", m.lower())

    def test_an_unrecognised_failure_admits_it(self):
        m = self.message(UNKNOWN)
        self.assertIn("did not recognise", m.lower())

    def test_an_unrecognised_failure_offers_candidates_as_candidates(self):
        """Listing causes is honest; naming one is not. The wording has to make the
        difference legible to someone skimming under incident pressure."""
        self.assertIn("roughly in order of how often", self.message(UNKNOWN))

    def test_an_unrecognised_failure_says_how_to_see_the_real_error(self):
        """charter withholds op's output for a good reason. Withholding it without
        saying the user may go and read it themselves is a dead end — which is how #78's
        reporter ended up debugging by hand with no idea what op had actually said."""
        m = self.message(UNKNOWN).lower()
        self.assertIn("yourself", m)

    def test_no_failure_of_any_kind_echoes_the_value_being_written(self):
        for stderr in (NO_ACCOUNT, FORBIDDEN, NO_TEMPLATE, UNKNOWN):
            self.assertNotIn("s3cret-beta", self.message(stderr))

    def test_no_failure_of_any_kind_echoes_op_s_output(self):
        self.assertNotIn("something charter has never seen", self.message(UNKNOWN))


class TestTheUnknownCaseStillDependsOnThePath(OpCase):
    """Where nothing matched, the path is the only information left: a failed write is
    usually a token that can read but not write; a failed read is usually a reference
    pointing at something that moved. That is the one job the `write` flag still does."""

    def failure(self, stderr: str, *, write: bool) -> str:
        """`_fail` is the seam both paths share; the read side of it is reachable only
        through the item-list call that `health()` wraps, so it is exercised here
        directly rather than through a contrived route."""
        _, p = self.make(fields={"A": "1"})
        proc = SimpleNamespace(returncode=1, stdout="", stderr=stderr)
        return str(p._fail("doing something", proc, write=write))

    def test_an_unrecognised_write_lists_write_access_first(self):
        self.assertIn("can not write to it", self.failure(UNKNOWN, write=True))

    def test_an_unrecognised_read_does_not_lead_with_write_access(self):
        self.assertNotIn("can not write to it", self.failure(UNKNOWN, write=False))

    def test_a_recognised_failure_ignores_the_path_entirely(self):
        """Where op told us what happened, the guess the path would have produced is
        irrelevant — the same signature means the same thing reading or writing."""
        self.assertEqual(self.failure(FORBIDDEN, write=True),
                         self.failure(FORBIDDEN, write=False))


class TestTheTemplateFlagsAreNeverPassed(OpCase):
    """`op` responds to an unparsed template by asking for `--category`. Adding it — or
    `--title` — makes op create the item with only `notesPlain`, dropping every custom
    field: a write that reports success and stores nothing.

    charter builds this argv from a literal, so nothing a user types can introduce the
    flags. What this pins is the code edit: the next person to hit that error and try the
    fix `op` itself suggests.
    """

    def test_neither_flag_reaches_a_create(self):
        op, p = self.make(fields={}, item_exists=False)
        p.set("A", "1")
        for c in op.call_with("item", "create"):
            self.assertNotIn("--category", c["argv"])
            self.assertNotIn("--title", c["argv"])

    def test_neither_flag_reaches_an_edit(self):
        op, p = self.make(fields={"A": "1"})
        p.set("B", "2")
        for c in op.call_with("item", "edit"):
            self.assertNotIn("--category", c["argv"])
            self.assertNotIn("--title", c["argv"])


if __name__ == "__main__":
    unittest.main()
