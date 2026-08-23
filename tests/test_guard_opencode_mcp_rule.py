"""opencode calls an MCP tool `<server>_<tool>`, so charter writes THAT name (#374).

`charter guard ask 'mcp__slack__send'` wrote the right rule for Claude Code (#365) and a
wrong one for opencode. `OpenCodeHarness.ask_rule` walks `TOOL_NAMES` looking for a
`Tool(pattern)` prefix and falls through to `return "bash", p` for everything else, so an
MCP pattern landed as `{"permission": {"bash": {"mcp__slack__send": "ask"}}}` — a rule over
a *bash command* literally named `mcp__slack__send`. The operator was told the guard was in
force and it could never fire, under any opencode configuration. That is #365's failure one
harness over, and it survived #365 because the fix was made in `commands._as_rule`, which
opencode does not go through.

`Harness.apply_ask_rule` documents the other honest answer — a harness that cannot express
a pattern returns `unsupported` and says why, because "the difference between naming that
limit and staying quiet is the difference between a limit and a lie". **That answer is not
available here, because opencode can express this.** Verified against opencode 1.18.21, the
binary on this machine, not inferred from the shape of the key:

* `permission` takes keys beyond the five it documents. `Permission.fromConfig` turns
  ``{"<key>": {"*": "ask"}}`` into ``{permission: "<key>", pattern: "*", action: "ask"}``,
  which `opencode debug agent build` prints back in the resolved rule list.
* Every MCP tool is registered under ``McpCatalog.toolName(server, tool)``, which is
  ``sanitize(server) + "_" + sanitize(tool)`` for ``sanitize = s =>
  s.replace(/[^a-zA-Z0-9_-]/g, "_")``, and the wrapper around it asks under exactly that
  id: ``ask({permission: <tool id>, patterns: ["*"]})``.
* `Permission.evaluate` glob-matches the permission NAME as well as the pattern
  (`Wildcard.match`) — which is how opencode's own ``{permission: "*"}`` default works — so
  a whole-server rule is ``<server>_*``.

The translation is therefore a rename, not an invented syntax: `mcp__slack__send` becomes
`slack_send` and `mcp__slack` becomes `slack_*`. `commands._MCP_RULE_RE` already confines
the pattern to ``[A-Za-z0-9_-]``, which is exactly the set opencode's `sanitize` leaves
alone, so no character in a rule charter accepts ever needs rewriting — and reusing that
regex rather than a second one keeps the two harnesses from disagreeing about what an MCP
rule even is.

What charter still cannot check is that the operator named the server the same way in
opencode's own `mcp` block. That is the same contract Claude Code's rule has — the name
comes from the operator, not from a guess — and the difference from the defect is total: a
mistyped server makes the rule inert for that server, while `bash` made it inert for every
possible one.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config
from charter.harness import registry
from tests._isolation import PersonaIso


class TestTheMcpNameOpencodeActuallyUses(unittest.TestCase):
    """The translation itself: charter's `mcp__` name → opencode's tool id."""

    def setUp(self) -> None:
        self.h = registry.get("opencode")

    def test_one_mcp_tool_becomes_that_tools_opencode_id(self):
        """The reported case. `bash` here matches a command nothing can run."""
        self.assertEqual(self.h.ask_rule("mcp__slack__send"), ("slack_send", "*"))

    def test_a_whole_server_becomes_a_glob_over_its_tools(self):
        """opencode glob-matches the permission name, so one key covers the server."""
        self.assertEqual(self.h.ask_rule("mcp__slack"), ("slack_*", "*"))

    def test_a_server_name_carrying_an_underscore_survives(self):
        """Only the FIRST `__` separates server from tool; `_` is legal in both halves and
        is a character opencode's own `sanitize` leaves alone."""
        self.assertEqual(self.h.ask_rule("mcp__my_server__do_thing"),
                         ("my_server_do_thing", "*"))

    def test_a_dashed_server_name_survives(self):
        """`-` is in opencode's keep-set too, so charter must not rewrite it."""
        self.assertEqual(self.h.ask_rule("mcp__my-server__do"), ("my-server_do", "*"))

    def test_allow_translates_the_same_way(self):
        """`allow_rule` defaults to `ask_rule` — the decision differs, the name must not."""
        self.assertEqual(self.h.allow_rule("mcp__slack__send"), ("slack_send", "*"))


class TestWhatIsStillABashCommand(unittest.TestCase):
    """The mirror-image failure a prefix test would introduce, which #365 already paid for
    once: a binary whose name merely begins with `mcp` is a command, not an MCP rule."""

    def test_a_bare_command_is_untouched(self):
        self.assertEqual(registry.get("opencode").ask_rule("git push *"),
                         ("bash", "git push *"))

    def test_a_binary_called_mcp_something_is_a_command(self):
        self.assertEqual(registry.get("opencode").ask_rule("mcp-inspector --list"),
                         ("bash", "mcp-inspector --list"))

    def test_a_tool_rule_still_comes_apart_into_tool_and_glob(self):
        self.assertEqual(registry.get("opencode").ask_rule("Read(./secrets/**)"),
                         ("read", "./secrets/**"))

    def test_a_wildcarded_mcp_pattern_is_not_quietly_given_an_opencode_name(self):
        """`commands` refuses `mcp__slack__send *` before any harness is asked (#369), and
        this is what keeps that refusal meaningful if the order ever changes: the test is on
        the SHAPE `_MCP_RULE_RE` accepts, not on a `mcp__` prefix. A prefix test would coin
        the opencode tool id `slack_send *` — a name with a space in it, which no tool can
        have — which is the same silent failure wearing the other harness's clothes."""
        self.assertEqual(registry.get("opencode").ask_rule("mcp__slack__send *"),
                         ("bash", "mcp__slack__send *"))


class TestWhatLandsInOpencodeJson(PersonaIso):
    """The file, because the translation is only worth anything once it is written."""

    def config_path(self) -> Path:
        return Path(config.ROOT) / "opencode.json"

    def test_the_rule_is_keyed_by_the_mcp_tool_not_by_bash(self):
        status, _ = registry.get("opencode").apply_ask_rule(Path(config.ROOT),
                                                            "mcp__slack__send")
        self.assertEqual(status, "added")
        perms = json.loads(self.config_path().read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "ask"})
        self.assertNotIn("bash", perms)

    def test_allow_writes_the_same_key_with_the_other_verb(self):
        registry.get("opencode").apply_allow_rule(Path(config.ROOT), "mcp__slack__send")
        perms = json.loads(self.config_path().read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "allow"})

    def test_writing_it_twice_is_not_an_edit(self):
        """Idempotence on the TRANSLATED key — it held for the wrong key too, so the
        assertion has to name what the file ends up holding or it proves nothing."""
        h = registry.get("opencode")
        h.apply_ask_rule(Path(config.ROOT), "mcp__slack__send")
        before = self.config_path().read_text()
        self.assertIn('"slack_send"', before)
        self.assertEqual(h.apply_ask_rule(Path(config.ROOT), "mcp__slack__send")[0],
                         "present")
        self.assertEqual(self.config_path().read_text(), before)


class TestThroughTheCommandBothHarnessesGetIt(PersonaIso):
    """One operator sentence, and neither harness is left holding a rule that cannot fire."""

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def test_guard_ask_writes_the_mcp_tool_id_for_opencode(self):
        rc, _ = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send",
                            local=False)
        self.assertEqual(rc, 0)
        perms = json.loads((Path(config.ROOT) / "opencode.json").read_text())["permission"]
        self.assertEqual(perms["slack_send"], {"*": "ask"})
        self.assertNotIn("bash", perms)

    def test_and_claude_code_still_gets_its_own_verbatim_rule(self):
        """The half that already worked (#365) must not move to fix the half that did not."""
        self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send", local=False)
        rules = json.loads(
            (Path(config.ROOT) / ".claude" / "settings.json").read_text()
        )["permissions"]["ask"]
        self.assertIn("mcp__slack__send", rules)

    def test_the_operator_is_shown_the_name_that_was_written(self):
        """`guard` says what it wrote (0.49.0). A translated name is exactly the case where
        the operator needs to read it back and check their server is called that."""
        _rc, out = self.invoke(commands.cmd_guard_ask, pattern="mcp__slack__send",
                               local=False)
        self.assertIn("slack_send", out)


if __name__ == "__main__":
    unittest.main()
