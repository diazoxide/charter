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

**A `✗` read as "nothing was written".** The refusal to touch a malformed file was
per-harness, so with `.claude/settings.json` corrupted the rule still landed in
`opencode.json`: one invocation, rule in force under one harness and absent under the other,
and a re-run after the fix made opencode's copy a duplicate write rather than a first one.
`add_permission_rule`'s docstring said charter "reports it and stops"; it reported and
continued.

#369 fixed the claim and left the behaviour, so this file used to assert that half-write
and the warning printed over it. **#376 made the behaviour all-or-nothing and those
assertions had to go with it** — they pinned the thing that was fixed, and keeping them
would have meant keeping the split. What is asserted instead is #369's actual guarantee,
which outlived its implementation: an operator is never misled about how far one `guard`
command reached. The `✗` now means what every reader already assumed it meant, and the
tests below hold the output to that.

Two of them are deliberately negative — the old warning must be *gone*, not merely joined
by a new line — because the old sentence told the operator to go and reconcile two
harnesses, and there is nothing to reconcile now. `test_guard_is_all_or_nothing.py` owns
the transaction itself.
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


class TestARefusalMeansNothingWasWritten(GuardCase):
    def test_the_precondition_that_a_clean_run_writes_both_files(self):
        """Asserted on its own, so every "opencode did not take it" below cannot pass in a
        world where opencode never writes at all."""
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="foo *", local=False)
        self.assertEqual(rc, 0)
        self.assertIn("foo *", json.dumps(json.loads(self.opencode().read_text())))

    def test_the_refusal_stops_the_other_harness_too(self):
        """The half-write itself. `.claude/settings.json` is refused, and #369's residue —
        the rule in force under opencode from the same command — is what #376 removed."""
        self.corrupt_claude_settings()
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="bar *", local=False)
        self.assertEqual(rc, 1, "the malformed file is still refused")
        self.assertEqual(self.settings().read_text(), "{not json",
                         "and left byte-identical")
        self.assertFalse(self.opencode().exists(),
                         "another harness took a rule from a command that refused")

    def test_the_old_uneven_landing_warning_is_not_printed(self):
        """Retired with the behaviour it described. It told the operator to reconcile two
        harnesses that now cannot disagree, which is a search with no object at the end."""
        self.corrupt_claude_settings()
        _, said = self.invoke(commands.cmd_guard_ask, pattern="bar *", local=False)
        self.assertNotRegex(said.lower(), r"landed unevenly")
        self.assertRegex(said.lower(), r"nothing was written")

    def test_guard_allow_says_it_too(self):
        self.corrupt_claude_settings()
        _, said = self.invoke(commands.cmd_guard_allow, pattern="bar *", local=False)
        self.assertNotRegex(said.lower(), r"landed unevenly")
        self.assertRegex(said.lower(), r"nothing was written")

    def test_a_clean_write_says_nothing_about_uneven_landing(self):
        """The message must be the exception, or it stops carrying information.

        Narrowed from `unevenly|not in force` to the warning's own words: a clean run now
        DOES say "Not in force under codex", and that is the honest standing limit #376
        added rather than the half-write warning this guards against.
        """
        _, said = self.invoke(commands.cmd_guard_ask, pattern="foo *", local=False)
        self.assertNotRegex(said.lower(), r"landed unevenly")

    def test_the_docstring_says_the_refusal_is_per_harness(self):
        """`add_permission_rule` said charter "reports it and stops", and it did not.

        Still the true sentence after #376, and for a reason worth keeping a test on: the
        stop is real now, but it is not here. This function still refuses one file and
        knows nothing about any other harness — `_guard_apply` is what turns three
        per-harness refusals into one command that writes nowhere. A reader who found the
        stop written into this docstring would look for it in the wrong layer.

        Asserted on the true claim rather than the absence of the old phrase, because the
        docstring now quotes that phrase in order to correct it — and a test that greps for
        a string would forbid the correction along with the error.
        """
        doc = commands.add_permission_rule.__doc__ or ""
        self.assertIn("per-harness", doc)


if __name__ == "__main__":
    unittest.main()
