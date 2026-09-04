"""`charter guard ask` writes Claude Code's rules; charter keeps no list of its own.

ADR 0014. Policy that can be written as a command pattern belongs to `permissions.ask` in
the plane's committed `.claude/settings.json` — a file charter already maintains for the
status line and the plane-root guard. Charter keeps only the guards that need context the
host cannot express: the working directory, the plane's vault paths, the active persona.

The temptation was a list in `charter.toml`. It would have been a second policy engine for
a job the host does better — it segments compound commands, which charter hand-rolls and
got wrong once — and the two could not have been reconciled, because **a hook cannot relax
a permission rule**: *"a matching ask rule still prompts even when the hook returned `allow`
or `ask`"*.

That last sentence is also the one interaction this creates, and the reason `doctor` grows a
check: a broad ask rule silently shadows `toolgate`'s `allow`, so a persona's declared tools
start prompting and nothing says why. A mechanism that looks wired and is not is the failure
shape this repo keeps paying for.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace

from charter import commands, config, doctor
from tests._isolation import PersonaIso


class GuardCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # Rooted at the plane, which is what these tests are about: `cmd_guard_ask` writes
        # `config.ROOT/.claude/settings.json`, and since #855 `check_ask_rules` reads the
        # settings the SESSION reads — the directory it is standing in, never an ancestor.
        # The two agree here, which is the ordinary case; without the chdir the writer and
        # the reader would be looking at different files and every assertion below would be
        # about the developer's own checkout.
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)

    def settings(self) -> Path:
        return Path(config.ROOT) / ".claude" / "settings.json"

    def write(self, body: dict) -> None:
        p = self.settings()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps(body, indent=2))

    def read(self) -> dict:
        return json.loads(self.settings().read_text())

    def invoke(self, fn, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = fn(SimpleNamespace(**kw))
        return rc, out.getvalue() + err.getvalue()


class TestItWritesTheHostsRule(GuardCase):
    def test_a_pattern_lands_in_permissions_ask(self):
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.assertIn("Bash(terraform apply *)", self.read()["permissions"]["ask"])

    def test_a_bare_pattern_is_wrapped_as_a_bash_rule(self):
        """`Bash(...)` is the host's syntax. Making the operator type it would be charter
        asking them to know an encoding it could apply itself."""
        self.invoke(commands.cmd_guard_ask, pattern="rm -rf *")
        self.assertIn("Bash(rm -rf *)", self.read()["permissions"]["ask"])

    def test_an_already_qualified_rule_is_left_alone(self):
        """A rule naming a non-Bash tool must survive untouched — wrapping it would produce
        `Bash(Read(./secrets))`, which matches nothing and fails silently."""
        self.invoke(commands.cmd_guard_ask, pattern="Read(./secrets/**)")
        self.assertIn("Read(./secrets/**)", self.read()["permissions"]["ask"])

    def test_it_is_idempotent(self):
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.assertEqual(self.read()["permissions"]["ask"].count("Bash(terraform apply *)"), 1)

    def test_existing_rules_survive(self):
        self.write({"permissions": {"ask": ["Bash(kubectl delete *)"], "deny": ["Bash(rm -rf /)"]}})
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        body = self.read()["permissions"]
        self.assertIn("Bash(kubectl delete *)", body["ask"])
        self.assertEqual(body["deny"], ["Bash(rm -rf /)"])

    def test_unrelated_settings_survive(self):
        self.write({"statusLine": {"type": "command", "command": "charter statusline"}})
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.assertEqual(self.read()["statusLine"]["command"], "charter statusline")

    def test_a_malformed_settings_file_is_never_overwritten(self):
        """The same restraint `_ensure_guard_hook` keeps: charter does not rewrite a file it
        could not parse, because the operator's content is in there."""
        p = self.settings()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        rc, said = self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.assertEqual(p.read_text(), "{ not json")
        self.assertNotEqual(rc, 0)


class TestThereIsNoCharterSideList(GuardCase):
    def test_charter_toml_is_not_touched(self):
        """ADR 0014's whole point: one record, so nothing can drift. A second list in
        `charter.toml` would need a sync step, and a sync step is where #127 lived."""
        toml = Path(config.ROOT) / "charter.toml"
        toml.write_text("schema = 1\n")
        before = toml.read_text()
        self.invoke(commands.cmd_guard_ask, pattern="terraform apply *")
        self.assertEqual(toml.read_text(), before)

    def test_listing_reads_the_settings_file(self):
        self.write({"permissions": {"ask": ["Bash(terraform apply *)"]}})
        _, said = self.invoke(commands.cmd_guard_list)
        self.assertIn("terraform apply", said)

    def test_listing_an_empty_plane_says_so_rather_than_printing_nothing(self):
        _, said = self.invoke(commands.cmd_guard_list)
        self.assertTrue(said.strip())


class TestDoctorNamesTheShadowedTools(GuardCase):
    def persona_with(self, *tools: str) -> None:
        self.make_persona("ops", role="Ops", vault="none", tools=", ".join(tools))
        from charter import persona
        persona.set_active("ops")

    def test_no_ask_rules_is_ok(self):
        self.persona_with("kubectl")
        self.assertEqual(doctor.check_ask_rules().status, doctor.OK)

    def test_an_unrelated_ask_rule_is_ok(self):
        self.persona_with("kubectl")
        self.write({"permissions": {"ask": ["Bash(terraform apply *)"]}})
        self.assertEqual(doctor.check_ask_rules().status, doctor.OK)

    def test_an_ask_rule_over_a_declared_tool_warns(self):
        """The interaction ADR 0014 creates. `toolgate` returns `allow` for a declared
        binary; the host prompts anyway, and the persona's tools quietly stop being
        pre-approved with nothing naming the cause."""
        self.persona_with("kubectl", "glab")
        self.write({"permissions": {"ask": ["Bash(kubectl *)"]}})
        r = doctor.check_ask_rules()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("kubectl", f"{r.detail} {r.hint}")

    def test_it_names_the_persona_whose_tool_is_shadowed(self):
        self.persona_with("kubectl")
        self.write({"permissions": {"ask": ["Bash(kubectl *)"]}})
        self.assertIn("ops", f"{doctor.check_ask_rules().detail} {doctor.check_ask_rules().hint}")

    def test_a_blanket_bash_rule_is_reported(self):
        """`Bash` and `Bash(*)` match every command, so they shadow every declared tool."""
        self.persona_with("kubectl")
        self.write({"permissions": {"ask": ["Bash"]}})
        self.assertEqual(doctor.check_ask_rules().status, doctor.WARN)

    def test_it_does_not_tell_you_to_delete_the_rule(self):
        """The rule is the operator's policy and probably deliberate. Charter names the
        consequence and leaves the choice — ADR 0013, and the restraint the MCP launcher
        check keeps."""
        self.persona_with("kubectl")
        self.write({"permissions": {"ask": ["Bash(kubectl *)"]}})
        self.assertNotIn("remove", (doctor.check_ask_rules().hint or "").lower())

    def test_an_unreadable_settings_file_does_not_pass_silently(self):
        """#171: absence of information is not evidence of health."""
        p = self.settings()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        self.persona_with("kubectl")
        self.assertEqual(doctor.check_ask_rules().status, doctor.WARN)

    def test_it_runs_in_doctor(self):
        names = [r.name for r in doctor.run_all()]
        self.assertIn("ask rules", names)


if __name__ == "__main__":
    unittest.main()
