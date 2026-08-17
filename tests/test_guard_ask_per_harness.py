"""One `charter guard ask`, every harness's own syntax and own file.

ADR 0014: charter writes the harness's rules and keeps no list of its own. With three
harnesses that means one operator sentence — `charter guard ask "git push *"` — has to
become `Bash(git push *)` in `.claude/settings.json` and `{"bash": {"git push *": "ask"}}`
in `opencode.json`. The operator learns neither syntax.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from charter.harness import registry


def _tmp(case) -> Path:
    p = Path(tempfile.mkdtemp(prefix="charter-ask-"))
    case.addCleanup(lambda: __import__("shutil").rmtree(p, True))
    return p


class RuleTranslation(unittest.TestCase):
    def test_claude_code_wraps_a_bare_command_as_bash(self):
        self.assertEqual(registry.get("claude-code").ask_rule("git push *"),
                         "Bash(git push *)")

    def test_claude_code_leaves_a_rule_that_already_names_a_tool(self):
        """Wrapping `Read(./secrets/**)` would produce `Bash(Read(...))`, which matches
        nothing and fails in the silent direction."""
        self.assertEqual(registry.get("claude-code").ask_rule("Read(./secrets/**)"),
                         "Read(./secrets/**)")

    def test_opencode_keeps_the_glob_and_names_the_tool_separately(self):
        """Verified shape: opencode's `permission` is `{tool: {pattern: decision}}` with
        `*`/`?` wildcards — not Claude Code's `Tool(pattern)` string."""
        self.assertEqual(registry.get("opencode").ask_rule("git push *"),
                         ("bash", "git push *"))


class WhereTheRuleLands(unittest.TestCase):
    def test_claude_code_writes_the_planes_settings(self):
        root = _tmp(self)
        status, path = registry.get("claude-code").apply_ask_rule(root, "git push *")
        self.assertEqual(status, "added")
        doc = json.loads(Path(path).read_text())
        self.assertIn("Bash(git push *)", doc["permissions"]["ask"])

    def test_opencode_writes_its_own_config(self):
        root = _tmp(self)
        status, path = registry.get("opencode").apply_ask_rule(root, "git push *")
        self.assertEqual(status, "added")
        doc = json.loads(Path(path).read_text())
        self.assertEqual(doc["permission"]["bash"]["git push *"], "ask")

    def test_adding_the_same_rule_twice_is_not_an_edit(self):
        for name in ("claude-code", "opencode"):
            with self.subTest(harness=name):
                root = _tmp(self)
                registry.get(name).apply_ask_rule(root, "git push *")
                before = sorted(p.read_text() for p in root.rglob("*.json"))
                self.assertEqual(registry.get(name).apply_ask_rule(root, "git push *")[0],
                                 "present")
                self.assertEqual(sorted(p.read_text() for p in root.rglob("*.json")),
                                 before)

    def test_codex_says_it_cannot_carry_the_rule_rather_than_pretending(self):
        """Codex's permissions are an approval policy and a sandbox, not command patterns.
        Charter's own hook still guards the command there — what it cannot do is hand the
        rule to the harness, and saying so is the difference between a limit and a lie."""
        status, detail = registry.get("codex").apply_ask_rule(_tmp(self), "git push *")
        self.assertEqual(status, "unsupported")
        self.assertTrue(detail.strip())

    def test_a_malformed_config_is_refused_not_repaired(self):
        root = _tmp(self)
        (root / "opencode.json").write_text("{not json")
        self.assertEqual(registry.get("opencode").apply_ask_rule(root, "x *")[0],
                         "malformed")
        self.assertEqual((root / "opencode.json").read_text(), "{not json")


if __name__ == "__main__":
    unittest.main()
