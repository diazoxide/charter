"""A line that names where a credential lives is not a credential (#985).

`hooks._secret_kind` is the one classifier behind four refusals: the `charter handoff` brief
check, the PostToolUse warning on a memory file, `charter persona memory-sync` and
`charter save`. Its "credential assignment" rule, `token: <six or more non-blank
characters>`, refused the remedy the brief refusal gives — "name where the credential lives"
— whenever that name was one word (`api_key = vault:forge/api-token`) or the command that
reads the value (``token: `charter secret get forge token` ``). Measured on main 63d9406. A
refusal that forbids its own remedy teaches the reader to route around it.

The exemption is the classifier's, so every caller gets the same answer, and it is narrow:
the WHOLE value, to the end of its line, must be a reference in one of the two spellings
charter writes — `vault:<vault>/<key>` (ADR 0022) or `charter secret get <vault> <key>` (the
CLI) — with at most a quote or backtick on either side. What stays refused, and is pinned
below as refused on main too:

* a bare `word/word`, because a secret can contain a slash;
* a real value beside a reference, appended to it, or in a second assignment on the line
  or the next;
* every near-miss of the two spellings, so the grammar is anchored at both ends;
* every other rule in the table, which the exemption never reaches: `vault:forge/xAKIA…` is
  an AWS access key whatever it is assigned to.
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
]

AWS = "AKIA" + "Q7VZ3RJHT2LMNPQR"
GITHUB = "ghp_" + "Zq81LmVw03RkT7yPqa9XcB2nD4sFhJ6uWe0i"

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
    "another provider's URI, which charter does not write here": "token: op://Eng/deploy/token",
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

    def test_the_remedy_the_brief_refusal_names_is_accepted(self):
        """The refusal and the exemption have to agree, or the refusal sends its reader
        straight back into itself: #985's whole defect. Every backticked spelling the text
        recommends, filled in, is read as a reference."""
        spellings = re.findall(r"`([^`]*<vault>[^`]*)`", commands_handoff.SECRET_BRIEF)
        self.assertGreaterEqual(len(spellings), 2, commands_handoff.SECRET_BRIEF)
        for spelling in spellings:
            filled = spelling.replace("<vault>", "forge").replace("<key>", "token")
            with self.subTest(spelling=spelling):
                self.assertIsNone(hooks._secret_kind(f"token: {filled}"))
                self.assertIsNone(hooks._secret_kind(f"token: `{filled}`"))


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
