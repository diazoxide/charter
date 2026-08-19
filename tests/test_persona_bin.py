"""A persona carrying its own executables (#283).

The report asked for `personas/<name>/bin/` "exposed on PATH while that persona is active",
on the grounds that a persona "cannot carry an executable, so a plugin survives purely as a
file carrier". Both halves turned out to be wrong, and checking them is what produced this
much smaller change:

* **It can already carry one.** `personas/<name>/` is committed wholesale, `bin/` is not
  gitignored, and git preserves the executable bit.
* **The tool guard already lets it run.** `toolgate._parse` reduces a command to
  `os.path.basename`, so a persona declaring `tools: site-health.sh` already gets
  `./personas/seo/bin/site-health.sh` auto-approved.
* **PATH cannot be done at all.** A `PreToolUse` hook decides *whether* a Bash call runs; it
  cannot alter the environment it runs in. Wrapping every Bash call to inject one is exactly
  the takeover of a host mechanism ADR 0014 rejects.

So what was actually missing was charter *knowing* the directory exists — and one hole the
report gestured at without noticing: basename matching means declaring a script's name
auto-approves that name **anywhere**, including a file an agent just wrote to /tmp.
"""
from __future__ import annotations

import os
import unittest

from charter import persona, toolgate
from tests._isolation import PersonaIso


class BinCase(PersonaIso):
    def script(self, owner: str, name: str, *, executable: bool = True) -> None:
        d = persona.bin_dir(owner)
        d.mkdir(parents=True, exist_ok=True)
        f = d / name
        f.write_text("#!/bin/sh\necho hi\n")
        if executable:
            f.chmod(0o755)


class TestCharterKnowsTheDirectory(BinCase):
    def test_a_persona_with_no_bin_has_no_scripts(self):
        self.make_persona("seo", role="SEO", vault="none")
        self.assertEqual(persona.bin_scripts("seo"), {})

    def test_an_executable_script_is_found(self):
        self.make_persona("seo", role="SEO", vault="none")
        self.script("seo", "site-health.sh")
        self.assertIn("site-health.sh", persona.bin_scripts("seo"))

    def test_a_non_executable_file_is_not_a_script(self):
        """The trap this exists to catch: git preserves the mode bit, so a file committed
        without +x fails at the moment someone needs it, with a shell error rather than
        anything pointing back here."""
        self.make_persona("seo", role="SEO", vault="none")
        self.script("seo", "lib.sh", executable=False)
        self.assertEqual(persona.bin_scripts("seo"), {})

    def test_lint_names_a_file_that_cannot_run(self):
        self.make_persona("seo", role="SEO", vault="none")
        self.script("seo", "lib.sh", executable=False)
        issues = persona.lint("seo")
        self.assertTrue(any("chmod" in m for _l, m in issues), issues)

    def test_scripts_are_inherited_down_the_extends_chain(self):
        """`mcp_servers` unions along exactly this lineage, and a child silently losing what
        its parent declared would be the surprise the rest of the frontmatter avoids."""
        self.make_persona("base", role="Base", vault="none")
        self.script("base", "doctor.sh")
        self.make_persona("child", role="Child", vault="none", extends="base")
        self.assertIn("doctor.sh", persona.bin_scripts("child"))

    def test_a_child_shadows_its_parents_script_of_the_same_name(self):
        self.make_persona("base", role="Base", vault="none")
        self.script("base", "doctor.sh")
        self.make_persona("child", role="Child", vault="none", extends="base")
        self.script("child", "doctor.sh")
        self.assertEqual(persona.bin_scripts("child")["doctor.sh"].parent,
                         persona.bin_dir("child"))


class TestProvenance(BinCase):
    """Declaring a script's name must not auto-approve that name anywhere.

    For `gh` or `kubectl` basename matching is right — they are system binaries and the
    plane does not own them. For a persona's own script it inverts the guarantee: the
    declaration looks specific and the check is not, so a file an agent wrote to /tmp with
    the same name inherits the persona's auto-approval.
    """

    def setUp(self):
        super().setUp()
        self.make_persona("seo", role="SEO", vault="none", tools="site-health.sh, gh")
        self.script("seo", "site-health.sh")
        persona.set_active("seo")

    def test_the_persona_s_own_script_is_approved(self):
        rel = persona.bin_dir("seo") / "site-health.sh"
        self.assertIsNotNone(toolgate.decide(f"{rel} --full"))

    def test_the_same_name_from_somewhere_else_is_not(self):
        self.assertIsNone(toolgate.decide("/tmp/site-health.sh --full"))

    def test_a_bare_name_is_not_approved_either(self):
        """`site-health.sh` alone resolves through PATH — which is precisely the thing
        charter cannot see and therefore cannot vouch for."""
        self.assertIsNone(toolgate.decide("site-health.sh --full"))

    def test_a_system_binary_is_unaffected(self):
        """The rule tightens only where charter has ground truth. `gh` is declared and is
        not one of this persona's scripts, so nothing about it changes."""
        self.assertIsNotNone(toolgate.decide("gh pr list"))

    def test_a_declared_name_with_no_script_behind_it_is_unchanged(self):
        """No bin/ entry means charter has nothing to check against, and inventing a
        restriction there would break planes that declare an ordinary binary with a
        script-shaped name."""
        self.make_persona("dev", role="Dev", vault="none", tools="mytool.sh")
        persona.set_active("dev")
        self.assertIsNotNone(toolgate.decide("mytool.sh --x"))


class TestTheAgentIsToldTheyExist(BinCase):
    """Carrying a script the dispatched agent never learns about is the same as not
    carrying it. PATH would have made them discoverable by habit; since PATH is off the
    table, the brief has to say so explicitly."""

    def test_the_generated_sub_agent_names_the_scripts_and_their_paths(self):
        from charter import commands_persona

        self.make_persona("seo", role="SEO", vault="none")
        self.script("seo", "site-health.sh")
        d = persona.resolve("seo")
        body = commands_persona._render_agent("seo", d["meta"], d["charter"])
        self.assertIn("site-health.sh", body)
        self.assertIn(f"personas/seo/{persona.BIN_DIR}/", body)

    def test_a_persona_with_no_scripts_gets_no_section(self):
        from charter import commands_persona

        self.make_persona("plain", role="Plain", vault="none")
        d = persona.resolve("plain")
        self.assertNotIn(persona.BIN_DIR + "/", commands_persona._render_agent(
            "plain", d["meta"], d["charter"]))


class TestShowDisclosesThem(BinCase):
    """A LIVE persona is committed and synced, so `bin/` puts someone else's code on a
    teammate's disk, run with their credentials. `mcp.json` is already committed and already
    launches processes — but it names commands that must already exist, while this ships the
    code. Disclosure rather than a gate: anyone who can commit `bin/` can commit an
    `mcp.json` pointing at the same file."""

    def test_show_lists_the_scripts(self):
        import io
        from contextlib import redirect_stdout
        from unittest import mock
        from charter import commands_persona

        self.make_persona("seo", role="SEO", vault="none")
        self.script("seo", "site-health.sh")
        args = mock.Mock()
        args.name = "seo"          # `Mock(name=…)` names the mock, it does not set .name
        out = io.StringIO()
        with redirect_stdout(out):
            commands_persona.cmd_persona_show(args)
        self.assertIn("site-health.sh", out.getvalue())


if __name__ == "__main__":
    unittest.main()
