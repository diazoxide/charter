"""`--local` — a permission rule for THIS MACHINE, not for the team.

`guard ask` and `guard allow` both write the plane's committed files, and the command says
so as reassurance: *"These files are committed, so the rule applies to everyone on this repo
— no sync step, and nothing that can drift (ADR 0014)."* For `ask` that is a feature. For
`allow` it is often exactly wrong, and there was no way to say so.

The two are not mirrors, and they differ in the direction that matters:

* an **ask** rule NARROWS what happens without a human — sharing it is conservative, and the
  worst case is a colleague sees one more prompt;
* an **allow** rule WIDENS it — sharing it extends one person's trust decision to everyone
  who clones the repo, on machines and under identities they did not choose.

Concretely: reaching for `charter guard allow 'gh pr merge *'` to unblock your own session
committed unprompted PR merges for the whole team, and nothing warned, because from the
command's point of view it had done its job.

ADR 0014 is right about the ENGINE — charter writes the host's rules and keeps no list of its
own. "Therefore always the shared file" is the part that does not follow when the rule widens.
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


class LocalCase(PersonaIso):
    def shared(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def local(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.local.json"

    def read(self, p: Path) -> dict:
        return json.loads(p.read_text())

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()

    def allow(self, pattern, local=False):
        return self.invoke(commands.cmd_guard_allow, pattern=pattern, local=local)

    def ask(self, pattern, local=False):
        return self.invoke(commands.cmd_guard_ask, pattern=pattern, local=local)


class TestLocalWritesTheMachineLocalFile(LocalCase):
    def test_allow_local_lands_in_settings_local(self):
        self.allow("gh pr merge *", local=True)
        self.assertIn("Bash(gh pr merge *)", self.read(self.local())["permissions"]["allow"])

    def test_it_does_not_touch_the_committed_file(self):
        """The whole point: a personal unblock must not become team policy."""
        self.allow("gh pr merge *", local=True)
        self.assertFalse(self.shared().exists(), "wrote the COMMITTED settings file")

    def test_ask_local_works_too(self):
        """Symmetry — a personal extra prompt is as legitimate as a personal allowance."""
        self.ask("terraform apply *", local=True)
        self.assertIn("Bash(terraform apply *)", self.read(self.local())["permissions"]["ask"])

    def test_local_is_idempotent(self):
        self.allow("gh pr merge *", local=True)
        self.allow("gh pr merge *", local=True)
        self.assertEqual(1, self.read(self.local())["permissions"]["allow"]
                         .count("Bash(gh pr merge *)"))

    def test_local_preserves_unrelated_local_settings(self):
        self.local().parent.mkdir(parents=True, exist_ok=True)
        self.local().write_text(json.dumps({"enabledPlugins": {"x@y": True}}))
        self.allow("gh pr merge *", local=True)
        self.assertEqual({"x@y": True}, self.read(self.local())["enabledPlugins"])

    def test_the_two_files_are_independent(self):
        self.allow("git status *")               # shared
        self.allow("gh pr merge *", local=True)  # local
        self.assertEqual(["Bash(git status *)"], self.read(self.shared())["permissions"]["allow"])
        self.assertEqual(["Bash(gh pr merge *)"], self.read(self.local())["permissions"]["allow"])


class TestTheDefaultIsUnchanged(LocalCase):
    """Committed remains the default — this adds a choice, it does not move the door."""

    def test_allow_without_local_still_writes_the_committed_file(self):
        self.allow("git status *")
        self.assertIn("Bash(git status *)", self.read(self.shared())["permissions"]["allow"])
        self.assertFalse(self.local().exists())

    def test_ask_without_local_still_writes_the_committed_file(self):
        self.ask("terraform apply *")
        self.assertIn("Bash(terraform apply *)", self.read(self.shared())["permissions"]["ask"])
        self.assertFalse(self.local().exists())


class TestTheSharedAllowSaysItIsTeamWide(LocalCase):
    """The output used to read as reassurance — 'no sync step, nothing that can drift' —
    which is the wrong register for a rule that widens what runs unprompted."""

    def test_a_shared_allow_names_the_blast_radius_and_the_alternative(self):
        _, out = self.allow("gh pr merge *")
        self.assertIn("everyone", out)
        self.assertIn("--local", out)

    def test_a_local_allow_does_not_claim_to_be_team_wide(self):
        _, out = self.allow("gh pr merge *", local=True)
        self.assertNotIn("everyone", out)


class TestAHarnessWithoutALocalFileSaysSo(LocalCase):
    """Naming the limit is the difference between a limit and a lie — the restraint
    `apply_ask_rule` already keeps for a harness with no command patterns."""

    def test_opencode_reports_unsupported_rather_than_writing_its_global_config(self):
        """opencode's only uncommitted config is `~/.config/opencode`, which applies to
        EVERY project. Swapping team-wide for all-my-projects-wide is not narrower, so
        this declines and says why."""
        h = registry.get(registry.OPENCODE)
        status, detail = h.apply_allow_rule(Path(config.ROOT), "gh pr merge *", local=True)
        self.assertEqual("unsupported", status)
        self.assertTrue(detail)
        self.assertFalse((Path(config.ROOT) / "opencode.json").exists())

    def test_the_command_still_succeeds_when_one_harness_declines(self):
        """Claude Code took it; opencode could not. That is a partial success, not a failure."""
        rc, out = self.allow("gh pr merge *", local=True)
        self.assertEqual(0, rc)
        self.assertIn("Bash(gh pr merge *)", self.read(self.local())["permissions"]["allow"])


class TestItRefusesRatherThanRepairing(LocalCase):
    def test_a_malformed_local_file_is_left_alone(self):
        self.local().parent.mkdir(parents=True, exist_ok=True)
        self.local().write_text("{ not json")
        self.allow("gh pr merge *", local=True)
        self.assertEqual("{ not json", self.local().read_text())


class TestItIsWiredIntoTheCli(unittest.TestCase):
    def test_allow_takes_local(self):
        ns = cli.build_parser().parse_args(["guard", "allow", "--local", "gh pr merge *"])
        self.assertTrue(ns.local)

    def test_ask_takes_local(self):
        ns = cli.build_parser().parse_args(["guard", "ask", "--local", "terraform apply *"])
        self.assertTrue(ns.local)

    def test_local_defaults_to_false(self):
        self.assertFalse(cli.build_parser().parse_args(["guard", "allow", "x"]).local)
        self.assertFalse(cli.build_parser().parse_args(["guard", "ask", "x"]).local)


if __name__ == "__main__":
    unittest.main()
