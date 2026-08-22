"""`charter guard list` shows both buckets, from both files (#368).

`charter guard` writes two buckets — `cmd_guard_ask` into `permissions.ask`,
`cmd_guard_allow` into `permissions.allow` — through one `add_permission_rule`, into one
file. The listing read one of them, and bare `charter guard` defaults to the listing, so it
is the command an operator reaches for to answer "what has charter put in my permissions".

The half it hid is the half with the wider blast radius. `cmd_guard_allow` shouts
`COMMITTED — this stops the prompt for everyone on this repo, not just you` precisely
because an allow rule widens what happens without a human, and its own comment records the
asymmetry: an ask rule narrows, so sharing it is conservative; an allow rule widens, and
sharing extends one person's trust decision to everyone who clones the repo. The listing
showed the conservative half and hid the widening one.

`--local` rules were invisible for the same reason — the reader only opened the committed
file — so no command answered "what is currently not prompting me, and who decided that".

**The file a rule lives in IS its blast radius**, so the output groups by file rather than
labelling rows: the committed file, then the machine-local one, each named for what it
means. `permissions.deny` is deliberately not shown; charter never writes it, and it
answers neither operator question.

A malformed file is named and does not suppress the other. Reporting nothing because one of
two files is unparseable would hide rules that are in force — the same silent direction the
defect itself pointed in.
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


class GuardListCase(PersonaIso):
    def settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def local_settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.local.json"

    def write(self, path: Path, body: dict) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(body, indent=2))

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def listing(self) -> tuple[int, str]:
        return self.invoke(commands.cmd_guard_list)


class TestBothBucketsAreShown(GuardListCase):
    def test_an_allow_rule_appears(self):
        """The reported case: written by `charter guard allow`, absent from the listing."""
        self.write(self.settings(), {"permissions": {"allow": ["Bash(gh pr merge *)"]}})
        _, said = self.listing()
        self.assertIn("gh pr merge", said)

    def test_an_ask_rule_still_appears(self):
        self.write(self.settings(), {"permissions": {"ask": ["Bash(terraform apply *)"]}})
        _, said = self.listing()
        self.assertIn("terraform apply", said)

    def test_both_appear_together_and_are_told_apart(self):
        self.write(self.settings(), {"permissions": {
            "ask": ["Bash(terraform apply *)"], "allow": ["Bash(gh pr merge *)"]}})
        _, said = self.listing()
        self.assertIn("terraform apply", said)
        self.assertIn("gh pr merge", said)
        self.assertIn("ask", said)
        self.assertIn("allow", said)

    def test_the_deny_bucket_is_not_shown(self):
        """charter never writes it, and it answers neither operator question."""
        self.write(self.settings(), {"permissions": {
            "ask": ["Bash(terraform apply *)"], "deny": ["Bash(rm -rf /)"]}})
        _, said = self.listing()
        self.assertNotIn("rm -rf", said)


class TestBlastRadiusIsOnScreen(GuardListCase):
    def test_a_machine_local_rule_is_listed(self):
        """Written by `guard allow --local`, and invisible to every reader until now."""
        self.write(self.local_settings(), {"permissions": {"allow": ["Bash(git status *)"]}})
        _, said = self.listing()
        self.assertIn("git status", said)

    def test_the_local_file_is_named_as_this_machine_only(self):
        self.write(self.local_settings(), {"permissions": {"allow": ["Bash(git status *)"]}})
        _, said = self.listing()
        self.assertIn("settings.local.json", said)
        self.assertIn("machine", said.lower())

    def test_the_committed_file_is_named_as_everyone(self):
        self.write(self.settings(), {"permissions": {"ask": ["Bash(terraform apply *)"]}})
        _, said = self.listing()
        self.assertIn("committed", said.lower())

    def test_rules_from_both_files_appear_in_one_listing(self):
        self.write(self.settings(), {"permissions": {"ask": ["Bash(terraform apply *)"]}})
        self.write(self.local_settings(), {"permissions": {"allow": ["Bash(git status *)"]}})
        _, said = self.listing()
        self.assertIn("terraform apply", said)
        self.assertIn("git status", said)

    def test_each_rule_is_listed_under_the_file_that_holds_it(self):
        """Grouping IS the blast radius. A flat list would need the reader to trust a
        label; this makes the file the heading the rule sits under."""
        self.write(self.settings(), {"permissions": {"ask": ["Bash(committed-rule *)"]}})
        self.write(self.local_settings(), {"permissions": {"ask": ["Bash(local-rule *)"]}})
        _, said = self.listing()
        lines = [ln for ln in said.splitlines() if ln.strip()]
        committed_at = next(i for i, ln in enumerate(lines) if "settings.json" in ln
                            and "settings.local.json" not in ln)
        local_at = next(i for i, ln in enumerate(lines) if "settings.local.json" in ln)
        committed_rule = next(i for i, ln in enumerate(lines) if "committed-rule" in ln)
        local_rule = next(i for i, ln in enumerate(lines) if "local-rule" in ln)
        self.assertTrue(committed_at < committed_rule < local_at,
                        "the committed rule belongs under the committed file")
        self.assertTrue(local_at < local_rule, "the local rule belongs under the local file")


class TestTheEmptyPlane(GuardListCase):
    def test_no_rules_anywhere_says_so(self):
        rc, said = self.listing()
        self.assertEqual(rc, 0)
        self.assertTrue(said.strip())

    def test_an_empty_bucket_is_not_a_rule(self):
        self.write(self.settings(), {"permissions": {"ask": [], "allow": []}})
        rc, said = self.listing()
        self.assertEqual(rc, 0)
        self.assertIn("No", said)


class TestAMalformedFile(GuardListCase):
    def test_a_malformed_committed_file_is_named(self):
        self.settings().parent.mkdir(parents=True, exist_ok=True)
        self.settings().write_text("{not json")
        rc, said = self.listing()
        self.assertEqual(rc, 1)
        self.assertIn("settings.json", said)

    def test_a_malformed_local_file_does_not_hide_the_committed_rules(self):
        """Reporting nothing because one file is unparseable would hide rules that ARE in
        force — the same silent direction this issue is about."""
        self.write(self.settings(), {"permissions": {"ask": ["Bash(terraform apply *)"]}})
        self.local_settings().write_text("{not json")
        rc, said = self.listing()
        self.assertEqual(rc, 1)
        self.assertIn("terraform apply", said)
        self.assertIn("settings.local.json", said)

    def test_a_malformed_committed_file_does_not_hide_the_local_rules(self):
        self.settings().parent.mkdir(parents=True, exist_ok=True)
        self.settings().write_text("{not json")
        self.write(self.local_settings(), {"permissions": {"allow": ["Bash(git status *)"]}})
        rc, said = self.listing()
        self.assertEqual(rc, 1)
        self.assertIn("git status", said)


class TestOddShapes(GuardListCase):
    """Somebody's deliberate structure, or somebody's mistake — never a crash."""

    def test_a_permissions_block_of_the_wrong_type_does_not_raise(self):
        self.write(self.settings(), {"permissions": "nope"})
        rc, _ = self.listing()
        self.assertEqual(rc, 0)

    def test_a_bucket_of_the_wrong_type_does_not_raise(self):
        self.write(self.settings(), {"permissions": {"ask": "nope"}})
        rc, _ = self.listing()
        self.assertEqual(rc, 0)


if __name__ == "__main__":
    unittest.main()
