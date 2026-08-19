"""`charter guard allow` — the missing half of `charter guard ask`.

charter could make a command prompt MORE and had no way to make one prompt LESS. The
symptom was concrete: `toolgate.decide` is allow-only and no persona declares `git` (only
`gh`, `glab`), so every `git status` in a control plane fell through to a permission
prompt — and it read as charter's doing, because charter is what put hooks on git. It is
not: charter denies three narrow things and asks on one. The prompts were the host's, and
charter had no vocabulary to tell it otherwise.

Same shape as `guard ask` throughout, because it is the same job with a different verb —
ADR 0014's "charter writes the host's rules and keeps no list of its own" is what makes
both correct, and a second mechanism here would be the `charter.toml` list that ADR
rejected. The two commands therefore share one writer and one translation.
"""

from __future__ import annotations

import io
import json
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import cli, commands, config
from charter.harness import registry
from tests._isolation import PersonaIso


class AllowCase(PersonaIso):
    def settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def read(self) -> dict:
        return json.loads(self.settings().read_text())

    def write(self, body: dict) -> None:
        p = self.settings()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(body, indent=2))

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def allow(self, pattern: str):
        return self.invoke(commands.cmd_guard_allow, pattern=pattern)


class TestItWritesTheHostsRule(AllowCase):
    def test_a_pattern_lands_in_permissions_allow(self):
        self.allow("git status *")
        self.assertIn("Bash(git status *)", self.read()["permissions"]["allow"])

    def test_a_bare_pattern_is_wrapped_as_a_bash_rule(self):
        self.allow("git diff *")
        self.assertIn("Bash(git diff *)", self.read()["permissions"]["allow"])

    def test_an_already_qualified_rule_is_left_alone(self):
        self.allow("Read(./docs/**)")
        self.assertIn("Read(./docs/**)", self.read()["permissions"]["allow"])

    def test_it_is_idempotent(self):
        self.allow("git status *")
        self.allow("git status *")
        self.assertEqual(1, self.read()["permissions"]["allow"].count("Bash(git status *)"))

    def test_it_does_not_touch_the_ask_list(self):
        """The two buckets mean opposite things — a bug that crossed them would turn a
        'stop asking me' into 'always ask me'."""
        self.write({"permissions": {"ask": ["Bash(terraform apply *)"]}})
        self.allow("git status *")
        perms = self.read()["permissions"]
        self.assertEqual(["Bash(terraform apply *)"], perms["ask"])
        self.assertIn("Bash(git status *)", perms["allow"])

    def test_it_preserves_unrelated_settings(self):
        self.write({"statusLine": {"type": "command", "command": "charter statusline"}})
        self.allow("git status *")
        self.assertEqual("charter statusline", self.read()["statusLine"]["command"])


class TestItRefusesRatherThanRepairing(AllowCase):
    """Somebody's deliberate structure is not charter's to rewrite — the restraint
    `add_ask_rule` already keeps."""

    def test_a_permissions_of_the_wrong_type_is_refused(self):
        self.write({"permissions": ["not", "an", "object"]})
        rc, out = self.allow("git status *")
        self.assertIn("permissions", out)
        self.assertEqual(["not", "an", "object"], self.read()["permissions"])

    def test_an_allow_of_the_wrong_type_is_refused(self):
        self.write({"permissions": {"allow": {"not": "a list"}}})
        self.allow("git status *")
        self.assertEqual({"not": "a list"}, self.read()["permissions"]["allow"])

    def test_an_unparseable_file_is_left_alone(self):
        p = self.settings()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ this is not json")
        self.allow("git status *")
        self.assertEqual("{ this is not json", p.read_text())

    def test_an_empty_pattern_is_rejected(self):
        rc, _ = self.allow("   ")
        self.assertEqual(2, rc)


class TestEveryHarnessGetsItsOwnSyntax(AllowCase):
    """ADR 0014 only holds if the translation lives in the harness, not in the command."""

    def test_claude_code_uses_its_rule_string(self):
        h = registry.get(registry.CLAUDE_CODE)
        self.assertEqual("Bash(git status *)", h.allow_rule("git status *"))

    def test_the_allow_translation_matches_the_ask_translation(self):
        """One operator sentence, one encoding — the verb is the only difference."""
        for h in registry.all():
            for pattern in ("git status *", "Read(./docs/**)", "rm -rf *"):
                self.assertEqual(h.ask_rule(pattern), h.allow_rule(pattern),
                                 f"{h.name}: {pattern}")

    def test_opencode_writes_its_own_permission_block(self):
        h = registry.get(registry.OPENCODE)
        status, _ = h.apply_allow_rule(Path(config.ROOT), "git status *")
        self.assertEqual("added", status)
        doc = json.loads((Path(config.ROOT) / "opencode.json").read_text())
        self.assertEqual("allow", doc["permission"]["bash"]["git status *"])

    def test_a_harness_without_command_patterns_says_so(self):
        """Naming the limit is the difference between a limit and a lie."""
        from charter.harness.base import Harness
        status, detail = Harness().apply_allow_rule(Path(config.ROOT), "git status *")
        self.assertEqual("unsupported", status)
        self.assertTrue(detail)


class TestItIsWiredIntoTheCli(unittest.TestCase):
    def test_the_subcommand_parses(self):
        ns = cli.build_parser().parse_args(["guard", "allow", "git status *"])
        self.assertEqual("git status *", ns.pattern)
        self.assertIs(ns.func, commands.cmd_guard_allow)

    def test_it_sits_beside_ask(self):
        ns = cli.build_parser().parse_args(["guard", "ask", "terraform apply *"])
        self.assertIs(ns.func, commands.cmd_guard_ask)


if __name__ == "__main__":
    unittest.main()
