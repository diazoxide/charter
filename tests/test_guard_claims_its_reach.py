"""`charter guard` says how far it reaches, and says when it only got half way (#369).

Two claims, both understating reach, both documentation-shaped rather than behaviour-shaped.

**The help named one file.** It said rules are written as Claude Code `permissions.ask`
rules in `.claude/settings.json`, while `cmd_guard_ask` and `cmd_guard_allow` iterate
`registry.all()` and write every registered harness — so a plane whose settings declare
`CHARTER_HARNESS: claude-code` gains a tracked `opencode.json` from a command whose help
named one file.

The behaviour is right and is not what changed. `registry.all()` has no `detect()` gate on
purpose: `detect()` answers "am I running inside this harness right now", not "does this
team use it", so gating on it would make a rule's reach depend on which harness happened to
type the command — and a teammate on opencode would silently not get it. That is exactly
the drift ADR 0014 exists to remove ("no sync step, and nothing that can drift"). The claim
is what was wrong, so the claim is what moved.

The names are derived from `registry.KINDS` rather than written into the help string.
`registry.py`'s own docstring says why: a hardcoded literal is the thing that goes stale the
day a harness is added, and this help would be one more place to remember.

**A `✗` read as "nothing was written".** The refusal to touch a malformed file is
per-harness, so with `.claude/settings.json` corrupted the rule still landed in
`opencode.json`: one invocation, rule in force under one harness and absent under the other,
and a re-run after the fix makes opencode's copy a duplicate write rather than a first one.
`add_permission_rule`'s docstring said charter "reports it and stops"; it reports and
continues.

Making it all-or-nothing is a two-phase apply across three file formats with no rollback
primitive, which is a design change and not this one. What is fixed here is the claim: the
output now says a rule landed unevenly, at the moment it happens.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import cli, commands, config
from charter.harness import registry
from tests._isolation import PersonaIso


def _guard_help() -> str:
    """The `guard` subcommand's help, as `charter --help` renders it."""
    parser = cli.build_parser()
    for action in parser._actions:
        if getattr(action, "choices", None) and "guard" in (action.choices or {}):
            return action.choices["guard"].format_help() + (
                action._choices_actions and "" or "")
    raise AssertionError("no `guard` subcommand found")


def _guard_summary() -> str:
    """The one-line help shown beside `guard` in the top-level command list."""
    parser = cli.build_parser()
    for action in parser._actions:
        for sub in getattr(action, "_choices_actions", []) or []:
            if sub.dest == "guard":
                return sub.help or ""
    raise AssertionError("no `guard` entry in the command list")


class TestTheHelpNamesEveryHarnessItWrites(unittest.TestCase):
    def test_the_registered_harnesses_are_named(self):
        said = _guard_summary().lower()
        for name in registry.KINDS:
            with self.subTest(harness=name):
                self.assertIn(name.lower(), said)

    def test_more_than_one_harness_is_registered(self):
        """Precondition: naming them all is only a claim worth testing while there are
        several. A one-harness registry would make this test vacuous."""
        self.assertGreater(len(registry.KINDS), 1)

    def test_the_names_are_derived_not_written_down(self):
        """A harness added to the registry must appear without editing the help string —
        the staleness `registry.py`'s docstring warns about."""
        extended = dict(registry.KINDS)
        extended["zzz-fictional"] = registry.KINDS[registry.CLAUDE_CODE]
        with mock.patch.object(registry, "KINDS", extended):
            self.assertIn("zzz-fictional", _guard_summary())

    def test_it_no_longer_claims_a_single_file(self):
        said = _guard_summary()
        self.assertNotIn(".claude/settings.json — charter keeps no list", said)

    def test_adr_0014_is_still_cited(self):
        """The reasoning did not change; only the count of files did."""
        self.assertIn("0014", _guard_summary())


class GuardCase(PersonaIso):
    def settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def opencode(self) -> Path:
        return Path(config.ROOT) / "opencode.json"

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def corrupt_claude_settings(self) -> None:
        self.settings().parent.mkdir(parents=True, exist_ok=True)
        self.settings().write_text("{not json")


class TestAPartialWriteSaysSo(GuardCase):
    def test_the_precondition_that_one_harness_refuses_and_another_writes(self):
        """Asserted on its own, so the message test below cannot pass in a world where
        nothing was written at all."""
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="foo *", local=False)
        self.assertEqual(rc, 0)
        self.corrupt_claude_settings()
        rc, said = self.invoke(commands.cmd_guard_ask, pattern="bar *", local=False)
        self.assertEqual(rc, 1, "the malformed file is still refused")
        self.assertEqual(self.settings().read_text(), "{not json",
                         "and left byte-identical")
        self.assertIn("bar *", json.dumps(json.loads(self.opencode().read_text())),
                      "while another harness took the rule")

    def test_an_uneven_landing_is_named(self):
        self.corrupt_claude_settings()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="bar *", local=False)
        self.assertIn("opencode", said)
        self.assertRegex(said.lower(), r"in force|not in force|unevenly|some harnesses")

    def test_guard_allow_says_it_too(self):
        self.corrupt_claude_settings()
        _, said = self.invoke(commands.cmd_guard_allow, pattern="bar *", local=False)
        self.assertRegex(said.lower(), r"in force|not in force|unevenly|some harnesses")

    def test_a_clean_write_says_nothing_about_uneven_landing(self):
        """The message must be the exception, or it stops carrying information."""
        _, said = self.invoke(commands.cmd_guard_ask, pattern="foo *", local=False)
        self.assertNotRegex(said.lower(), r"unevenly|not in force")

    def test_the_docstring_says_the_refusal_is_per_harness(self):
        """`add_permission_rule` said charter "reports it and stops". It reports and
        continues to the next harness, and the next reader deserves the true sentence.

        Asserted on the true claim rather than the absence of the old phrase, because the
        docstring now quotes that phrase in order to correct it — and a test that greps for
        a string would forbid the correction along with the error.
        """
        doc = commands.add_permission_rule.__doc__ or ""
        self.assertIn("per-harness", doc)


if __name__ == "__main__":
    unittest.main()
