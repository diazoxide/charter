"""`_as_rule` tests the SHAPE of a rule, not the prefix of a string (#365).

`_RULE_TOOLS` lists the tools whose rules charter must write verbatim, and deliberately
includes `mcp__`. The condition that consulted it did not: it also required
`("(" in p or p in ("Bash",))`, and Claude Code's MCP permission syntax is the bare
`mcp__<server>__<tool>` — it never carries parentheses. So `charter guard ask
'mcp__slack__send'` wrote `Bash(mcp__slack__send)`: a rule matching a *bash command*
literally named `mcp__slack__send`, which is to say nothing at all. A tick was printed and
a committed rule was written, and the operator was never prompted.

That is the exact failure `_RULE_TOOLS`' own docstring exists to prevent, in its own words —
"which matches nothing and fails in the silent direction" — landing on the one tool family
that cannot carry the parenthesis the carve-out demanded.

**The parenthesis requirement was load-bearing, though, and dropping it would mirror the
bug.** `str.startswith` is a raw prefix match, so `Globalprotect --connect` starts with
`Glob` and `Taskwarrior add x` starts with `Task`. Under a prefix test with no parenthesis
requirement, both would stop being wrapped and become bare rules matching nothing — the same
silent failure pointing the other way. So the test is on the shape a rule can actually take:
`Tool(pattern)`, a bare tool name, or a bare MCP name. Everything else is a command.

**And one shape is refused rather than written.** `mcp__slack__send *` names Claude Code's
MCP syntax and then asks for a wildcard it does not have. There is no string charter can
write that means it: wrapping produces `Bash(mcp__slack__send *)` and writing it bare
produces a rule that matches nothing, so both directions fail silently. `mcp__` is the one
family where wrapping is provably wrong rather than merely suspicious, so the command
refuses before any harness writes — which also keeps the plane from ending up with the rule
under one harness and not another (#369).
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config
from tests._isolation import PersonaIso


class TestBareToolRulesSurvive(unittest.TestCase):
    """A rule naming a tool is written verbatim whether or not it carries parentheses."""

    def test_a_bare_mcp_tool_rule_is_not_wrapped(self):
        """The reported case. `Bash(mcp__slack__send)` matches nothing."""
        self.assertEqual(commands._as_rule("mcp__slack__send"), "mcp__slack__send")

    def test_a_whole_mcp_server_rule_is_not_wrapped(self):
        self.assertEqual(commands._as_rule("mcp__slack"), "mcp__slack")

    def test_a_bare_tool_name_is_not_wrapped(self):
        """`Read` alone is a whole-tool rule in the host's syntax, like `Bash` already was."""
        self.assertEqual(commands._as_rule("Read"), "Read")

    def test_task_is_not_wrapped(self):
        self.assertEqual(commands._as_rule("Task"), "Task")

    def test_bash_alone_is_still_not_wrapped(self):
        self.assertEqual(commands._as_rule("Bash"), "Bash")


class TestParenthesisedRulesStillSurvive(unittest.TestCase):
    """The half that already worked, kept working."""

    def test_a_read_rule_survives(self):
        self.assertEqual(commands._as_rule("Read(./secrets/**)"), "Read(./secrets/**)")

    def test_a_webfetch_rule_survives(self):
        self.assertEqual(commands._as_rule("WebFetch(domain:x.com)"),
                         "WebFetch(domain:x.com)")

    def test_a_bash_rule_survives(self):
        self.assertEqual(commands._as_rule("Bash(git push *)"), "Bash(git push *)")


class TestCommandsThatMerelySTARTWithAToolName(unittest.TestCase):
    """The mirror-image silent failure, which a prefix test would have introduced.

    These are ordinary binaries. Under `startswith(_RULE_TOOLS)` with the parenthesis
    requirement dropped they would have been written bare, matching nothing.
    """

    def test_globalprotect_is_a_command_not_a_glob_rule(self):
        self.assertEqual(commands._as_rule("Globalprotect --connect"),
                         "Bash(Globalprotect --connect)")

    def test_taskwarrior_is_a_command_not_a_task_rule(self):
        self.assertEqual(commands._as_rule("Taskwarrior add x"), "Bash(Taskwarrior add x)")

    def test_an_ordinary_command_is_wrapped(self):
        self.assertEqual(commands._as_rule("terraform apply *"), "Bash(terraform apply *)")

    def test_a_tool_name_with_arguments_is_a_command(self):
        """`Read the manual` is not a whole-tool rule; the tool rule is `Read` exactly."""
        self.assertEqual(commands._as_rule("Read the manual"), "Bash(Read the manual)")


class TestARuleCharterCannotExpress(unittest.TestCase):
    """`mcp__` plus a wildcard: no string means it, so charter writes neither."""

    def test_a_wildcarded_mcp_rule_is_refused(self):
        with self.assertRaises(commands.UnexpressibleRule):
            commands._as_rule("mcp__slack__send *")

    def test_the_refusal_names_the_pattern_and_the_reason(self):
        try:
            commands._as_rule("mcp__slack__send *")
        except commands.UnexpressibleRule as e:
            self.assertIn("mcp__slack__send *", str(e))
            self.assertIn("wildcard", str(e).lower())
        else:
            self.fail("expected UnexpressibleRule")

    def test_a_valid_mcp_name_with_dashes_is_still_accepted(self):
        self.assertEqual(commands._as_rule("mcp__my-server__do_thing"),
                         "mcp__my-server__do_thing")


class GuardCase(PersonaIso):
    def settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()


class TestEndToEndThroughTheCommand(GuardCase):
    def test_guard_ask_writes_the_mcp_rule_verbatim(self):
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send", local=False)
        self.assertEqual(rc, 0)
        rules = json.loads(self.settings().read_text())["permissions"]["ask"]
        self.assertIn("mcp__slack__send", rules)
        self.assertNotIn("Bash(mcp__slack__send)", rules)

    def test_guard_allow_writes_the_mcp_rule_verbatim(self):
        rc, _ = self.invoke(commands.cmd_guard_allow, pattern="mcp__slack__send", local=False)
        self.assertEqual(rc, 0)
        rules = json.loads(self.settings().read_text())["permissions"]["allow"]
        self.assertIn("mcp__slack__send", rules)

    def test_an_unexpressible_pattern_writes_nothing_anywhere(self):
        """Refused before the harness loop, so no harness gets a half of it (#369)."""
        rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send *",
                              local=False)
        self.assertEqual(rc, 2)
        self.assertFalse(self.settings().exists())
        self.assertFalse((Path(config.ROOT) / "opencode.json").exists())
        self.assertIn("mcp__slack__send *", out)

    def test_guard_allow_refuses_the_same_pattern(self):
        rc, _ = self.invoke(commands.cmd_guard_allow, pattern="mcp__slack__send *",
                            local=False)
        self.assertEqual(rc, 2)
        self.assertFalse(self.settings().exists())


if __name__ == "__main__":
    unittest.main()
