"""A line that names where a credential lives is not a credential (#985).

`hooks._secret_kind` is the one classifier behind four refusals: the `charter handoff` brief
check, the PostToolUse warning on a memory file, `charter persona memory-sync` and
`charter save`. Its "credential assignment" rule, `token: <six or more non-blank
characters>`, refused the remedy the brief refusal gives — "name where the credential lives"
— whenever that name was one word (`api_key = vault:forge/api-token`) or the command that
reads the value (``token: `charter secret get forge token` ``). Measured on main 63d9406. A
refusal that forbids its own remedy teaches the reader to route around it.

The exemption is the classifier's, so every caller gets the same answer, and it is narrow:
the WHOLE value, to the end of its line, must be a reference in a spelling charter uses —
`vault:<vault>/<key>` (ADR 0022), `charter secret get <vault> <key>` (the CLI), or one of the
two URIs a reference vault stores, `op://<vault>/<item>/<field>` and `vault://<path>#<field>`
— with at most a quote or backtick on either side. Its names must also look like names: no
name starts with a known credential prefix, and ALL the names together — every vault, key,
item, field and path segment, without the scheme or the separators — come to at most 32
characters. Two reviews shaped that. The first cut had no rule, and a live token typed into a
reference (`vault:forge/ghp_…`) passed where main refused it. The second capped each name at
32, and a longer secret that holds a `/` — AWS's documented example secret key is one — split
into short names and passed. A cap on the total refuses a secret of more than 32 name
characters however it is split. What stays refused, and is pinned below as refused on main too:

* a bare `word/word`, because a secret can contain a slash;
* a real value beside a reference, appended to it, or in a second assignment on the line
  or the next;
* a token in any slot of any spelling, by its length or by its prefix, and a long secret
  split across several slots;
* every near-miss of the four spellings, so the grammar is anchored at both ends;
* every other rule in the table, which the exemption never reaches: `vault:forge/xAKIA…` is
  an AWS access key whatever it is assigned to.

The rule's own ceiling is pinned as a measured fact rather than tested around: a secret of at
most 32 name characters, not counting the `/`, `#` or single spaces that separate them, that
starts with no listed prefix still reads as names — `vault:<16>/<16>` passes at 33.
"""

from __future__ import annotations

import io
import re
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from charter import commands_handoff, config, hooks
from tests._isolation import PersonaIso
from tests.test_a_handoff_refuses_before_it_changes_anything import _AHandoffFromAlpha

#: The spellings #985 measured refused on main, and their quoted and multi-line forms.
REFERENCES = [
    "api_key = vault:forge/api-token",
    "token: vault:forge/token",
    "password=vault:db/password",
    "SECRET: vault:ops/DEPLOY_TOKEN",
    "token: `charter secret get forge token`",
    "token: charter secret get forge token",
    'token: "vault:forge/token"',
    "token: 'vault:forge/token'",
    "`token: vault:forge/token`",
    "token: vault:forge/token   ",
    "token:vault:forge/token\r",
    "# Deploy notes\n\ntoken: vault:forge/token\nThe runner reads it at start.\n",
    "token: op://Eng/deploy/token",
    "password: 'op://Private/db-prod/password'",
    "token: vault://secret/data/app#TOKEN",
    "api_key = `vault://kv/forge#api-token`",
]

AWS = "AKIA" + "Q7VZ3RJHT2LMNPQR"
GITHUB = "ghp_" + "Zq81LmVw03RkT7yPqa9XcB2nD4sFhJ6uWe0i"

#: Every spelling the exemption reads, with `{}` in turn standing for each of its slots and
#: the other slots filled with ordinary names. A token is dropped into the `{}`.
SLOTS = [
    "vault:{}/token", "vault:forge/{}",
    "charter secret get {} token", "charter secret get forge {}",
    "op://{}/deploy/token", "op://Eng/{}/token", "op://Eng/deploy/{}",
    "vault://{}/data/app#TOKEN", "vault://secret/{}/app#TOKEN", "vault://secret/data/{}#TOKEN",
    "vault://secret/data/app#{}",
]

#: The live-token shapes the review measured refused on main and let through by the first cut
#: of this exemption. Built at runtime, split inside the prefix, so no specimen is a literal a
#: secret scanner would read as a real token.
LIVE_TOKENS = {
    "a GitHub classic token": "gh" + "p_" + "Zq81LmVw03RkT7yPqa9XcB2nD4sFhJ6uWe0i",
    "a GitHub fine-grained token": "github" + "_pat_" + "11AAAAAAA0" + "b1" * 30,
    "a Google API key": "AI" + "za" + "SyD" + "x9" * 16,
    "a Stripe live key": "sk_" + "live_" + "4eC39HqLyjWDarjtT1zdp7dc",
    "an OpenAI project key": "sk-" + "proj-" + "abc" * 16,
    "a Slack bot token": "xo" + "xb-" + "2400000000-1111111111-" + "a" * 24,
    "a PyPI token": "py" + "pi-" + "AgEIcHlwaS5vcmc" + "Q" * 50,
    "a 40-character hex key": "0123456789abcdef" * 2 + "01234567",
}

#: One SHORT specimen per credential prefix — 32 characters or fewer, so length alone never
#: refuses it and only the prefix can. Spelled out here rather than built from the tuple, and
#: the tuple is asked to match this table's keys, so a prefix added without a specimen, or
#: removed from the tuple, fails.
SHORT_PREFIXED = {
    "ghp_": "gh" + "p_" + "a1b2c3d4e5", "gho_": "gh" + "o_" + "a1b2c3d4e5",
    "ghu_": "gh" + "u_" + "a1b2c3d4e5", "ghs_": "gh" + "s_" + "a1b2c3d4e5",
    "ghr_": "gh" + "r_" + "a1b2c3d4e5", "github_pat_": "github" + "_pat_" + "a1b2c3",
    "glpat-": "gl" + "pat-" + "a1b2c3d4e5", "sk_live_": "sk_" + "live_" + "a1b2c3",
    "sk_test_": "sk_" + "test_" + "a1b2c3", "rk_live_": "rk_" + "live_" + "a1b2c3",
    "sk-": "sk" + "-a1b2c3d4e5", "xoxa-": "xo" + "xa-" + "a1b2c3",
    "xoxb-": "xo" + "xb-" + "a1b2c3", "xoxp-": "xo" + "xp-" + "a1b2c3",
    "xoxr-": "xo" + "xr-" + "a1b2c3", "xoxs-": "xo" + "xs-" + "a1b2c3",
    "AIza": "AI" + "za" + "a1b2c3d4", "pypi-": "py" + "pi-" + "a1b2c3",
    "npm_": "np" + "m_" + "a1b2c3d4", "hf_": "h" + "f_" + "a1b2c3d4",
    "AKIA": "AK" + "IA" + "a1b2", "ASIA": "AS" + "IA" + "a1b2",
}

#: Every spelling with EVERY slot open, for the length rule, which is on all the names of a
#: reference together rather than on any one of them.
SPELLINGS = ["vault:{}/{}", "charter secret get {} {}", "op://{}/{}/{}", "vault://{}#{}",
             "vault://{}/{}/{}#{}"]


def _names_totalling(template: str, total: int) -> str:
    """*template* with its slots filled by ordinary names whose lengths add up to *total*,
    spread as evenly as the slots allow."""
    n = template.count("{}")
    sizes = [total // n + (1 if i < total % n else 0) for i in range(n)]
    return template.format(*("deploy_token_name_for_the_ci_run"[:s] for s in sizes))


#: A secret longer than 32 characters split into short names. Each was `credential
#: assignment` on main and passed a per-name cap of 32 (review of 36e084d).
AWS_EXAMPLE_SECRET = "wJalrXUtnFEMI/K7MDENG/" + "bPxRfiCYEXAMPLEKEY"
HEX32 = "0123456789abcdef" * 2
SPLIT_SECRETS = {
    "AWS's example secret key as a vault:// path": f"vault://{AWS_EXAMPLE_SECRET}#x",
    "AWS's example secret key as an op:// reference": f"op://{AWS_EXAMPLE_SECRET}",
    "a random secret split by one slash": "vault:9sXk2LqPz7VbN4mR0tYw/" + "Hc8dJf3Ua6Ge1Ko5Ri",
    "two 32-character hex names": f"vault:{HEX32}/{HEX32}",
    "three 32-character names in a vault:// URI": f"vault://{HEX32}/{HEX32}#{HEX32}",
    "six 30-character path segments": "vault://" + "/".join([HEX32[:30]] * 6) + "#field",
}

#: Ordinary references, which the length rule must leave alone.
ORDINARY = ["op://Eng/deploy/token", "vault://secret/data/deploy#token",
            "vault:forge/api-token", "charter secret get forge api-token"]

#: Still refused, and refused on main too. Each names the clause that refuses it.
REFUSED = {
    "a bare word/word": "token: forge/token",
    "a bare word/word, password": "password: db/password",
    "a real value beside a reference": f"token: vault:forge/x {GITHUB}",
    "a real value appended to a reference": f"token: vault:forge/x{AWS}",
    "a reference then a real assignment": "token=vault:forge/x password=hunter22-not-real",
    "a real assignment then a reference": "password=hunter22-not-real token=vault:forge/x",
    "a real assignment on the next line": "token: vault:forge/token\npassword: hunter2-not-a-real-one",
    "prose after the reference": "token: vault:forge/token hunter2-not-a-real-one",
    "no key": "token: vault:forge",
    "no vault": "token: vault:/token",
    "a third segment": "token: vault:forge/token/extra",
    "another scheme word": "token: vaults:forge/token",
    "a prefix before the scheme": "token: xvault:forge/token",
    "the CLI with no key": "token: charter secret get forge",
    "the CLI with an extra word": "token: charter secret get forge token --reveal",
    "the CLI inside a substitution": "token: $(charter secret get forge token)",
    "op: with one slash": "token: op:/Eng/deploy/token",
    "another scheme ending in op": "token: opx://Eng/deploy/token",
    "op:// with a segment missing": "token: op://Eng/deploy",
    "op:// with an extra segment": "token: op://Eng/deploy/token/extra",
    "op:// with an empty segment": "token: op://Eng//token",
    "vault:// with no field": "token: vault://secret/data/app",
    "vault:// with two fields": "token: vault://secret/data/app#A#B",
    "vault:// with no path": "token: vault://#TOKEN",
    "vault:/ with one slash": "token: vault:/secret/data/app#TOKEN",
    "another scheme ending in vault": "token: xvault://secret/data/app#TOKEN",
}


class TheClassifierReadsAReferenceAsAReference(unittest.TestCase):
    def test_a_reference_is_not_a_credential(self):
        for text in REFERENCES:
            with self.subTest(text=text):
                self.assertIsNone(hooks._secret_kind(text))

    def test_what_is_not_exactly_a_reference_is_still_refused(self):
        for clause, text in REFUSED.items():
            with self.subTest(clause):
                self.assertIsNotNone(hooks._secret_kind(text), text)

    def test_the_exemption_never_reaches_another_rule(self):
        self.assertEqual("AWS access key", hooks._secret_kind(f"token: vault:forge/x{AWS}"))

    def test_every_slot_holds_an_ordinary_name(self):
        """The other half of every row below: the slot template itself is a reference."""
        for slot in SLOTS:
            with self.subTest(slot=slot):
                self.assertIsNone(hooks._secret_kind("token: " + slot.format("deploy")))

    def test_a_live_token_in_any_slot_is_refused(self):
        """The review's regression. Each is `credential assignment` on main, and each was
        let through by a reference that had no rule for its slots."""
        for shape, tok in LIVE_TOKENS.items():
            for slot in SLOTS:
                text = "token: " + slot.format(tok)
                with self.subTest(shape, slot=slot):
                    self.assertEqual("credential assignment", hooks._secret_kind(text))

    def test_a_short_token_with_a_credential_prefix_is_refused_in_any_slot(self):
        """Only the prefix refuses these: the same slot holding an unprefixed name of the same
        length is exempt, so the length rule cannot be what answers."""
        self.assertEqual(set(SHORT_PREFIXED), set(hooks._CREDENTIAL_PREFIXES))
        for prefix, tok in SHORT_PREFIXED.items():
            self.assertLessEqual(len(tok), 32, prefix)
            self.assertTrue(tok.startswith(prefix), prefix)
            for slot in SLOTS:
                text = "token: " + slot.format(tok)
                with self.subTest(prefix=prefix, slot=slot):
                    self.assertIsNone(hooks._secret_kind("token: " + slot.format("n" * len(tok))))
                    self.assertIsNotNone(hooks._secret_kind(text), text)

    def test_a_long_secret_split_into_short_names_is_refused(self):
        for shape, ref in SPLIT_SECRETS.items():
            with self.subTest(shape):
                self.assertEqual("credential assignment", hooks._secret_kind(f"secret: {ref}"))
                self.assertEqual("credential assignment", hooks._secret_kind(f"password: {ref}"))

    def test_ordinary_references_are_within_the_length_rule(self):
        for ref in ORDINARY:
            with self.subTest(ref=ref):
                self.assertIsNone(hooks._secret_kind(f"token: {ref}"))

    def test_the_separators_are_not_counted(self):
        """The documented ceiling, measured: sixteen name characters either side of one `/` is
        a 33-character value, and it passes, because the cap counts names and not the text
        between them. A secret carrying k separators can be 32 + k characters."""
        value = "vault:" + "a" * 16 + "/" + "b" * 16
        self.assertEqual(33, len(value) - len("vault:"))
        self.assertIsNone(hooks._secret_kind(f"secret: {value}"))

    def test_names_totalling_32_are_a_reference_and_33_are_not(self):
        """The boundary, spread across every slot of every spelling."""
        for spelling in SPELLINGS:
            at, over = _names_totalling(spelling, 32), _names_totalling(spelling, 33)
            with self.subTest(spelling=spelling):
                self.assertIsNone(hooks._secret_kind(f"token: {at}"), at)
                self.assertEqual("credential assignment", hooks._secret_kind(f"token: {over}"),
                                 over)

    def test_the_remedy_the_brief_refusal_names_is_accepted(self):
        """The refusal and the exemption have to agree, or the refusal sends its reader
        straight back into itself: #985's whole defect. Every backticked spelling the text
        recommends, its placeholders filled with ordinary names, is read as a reference —
        and the same spelling with a token in a placeholder is not."""
        spellings = re.findall(r"`([^`]*<[a-z]+>[^`]*)`", commands_handoff.SECRET_BRIEF)
        self.assertGreaterEqual(len(spellings), 2, commands_handoff.SECRET_BRIEF)
        for spelling in spellings:
            filled = re.sub(r"<([a-z]+)>", r"\1", spelling)
            tokened = re.sub(r"<[a-z]+>", LIVE_TOKENS["a GitHub classic token"], spelling,
                             count=1)
            with self.subTest(spelling=spelling):
                self.assertIsNone(hooks._secret_kind(f"token: {filled}"))
                self.assertIsNone(hooks._secret_kind(f"token: `{filled}`"))
                self.assertIsNotNone(hooks._secret_kind(f"token: {tokened}"))


class TheMemoryWarningAgrees(PersonaIso):
    """The PostToolUse warning a memory write gets, through the hook's own scan."""

    def _scan(self, body: str) -> str:
        d = Path(config.ROOT) / "personas" / "qa" / "memory"
        d.mkdir(parents=True, exist_ok=True)
        fp = d / "where-the-token-lives.md"
        fp.write_text(body)
        out = io.StringIO()
        with redirect_stdout(out):
            hooks._posttooluse_secret_scan({"content": body}, str(fp), "sess-1")
        return out.getvalue()

    def test_a_memory_naming_where_the_token_lives_is_not_warned_about(self):
        for text in REFERENCES:
            with self.subTest(text=text):
                self.assertEqual("", self._scan(f"# Forge\n\n{text}\n"))

    def test_a_memory_holding_a_value_beside_a_reference_still_warns(self):
        for clause, text in REFUSED.items():
            with self.subTest(clause):
                self.assertIn("SECURITY", self._scan(f"# Forge\n\n{text}\n"))


class TheHandoffBriefAgrees(_AHandoffFromAlpha):
    """`charter handoff`'s own brief check, which names the remedy this is about."""

    def test_a_brief_that_names_where_the_credential_lives_opens(self):
        for line in ("api_key = vault:forge/api-token", "token: `charter secret get forge token`"):
            self.open.reset_mock()
            with self.subTest(line=line):
                rc, _out, err = self._handoff("beta", brief=f"Deploy the widget\n{line}\n")
                self.assertEqual(0, rc, err)
                self.open.assert_called_once()

    def test_a_brief_with_a_value_beside_the_reference_is_refused(self):
        rc, _out, err = self._handoff(
            "beta", brief="Deploy the widget\napi_key = vault:forge/api-token\n"
                          "password: hunter2-not-a-real-one\n")
        self.assertEqual(1, rc)
        self.assertIn("credential assignment", err)
        self.assertNotIn("hunter2", err)
        self._nothing_changed()


if __name__ == "__main__":
    unittest.main()
