"""An MCP server whose launcher does not exist can never start — and says nothing.

This is the failure #197 arrived as. The `edm`->`charter` rename removed `bin/edm` and its
forwarding shim, and every MCP server in `~/.claude.json` registered as
``bin/edm secret exec <vault> ... --exec -- <server>`` began failing with ENOENT. Claude
Code does not surface that: the tools simply are not there. A persona lost its entire
toolset mid-investigation and rerouted through cloudflared + Kibana before anyone worked
out why.

Nothing in charter read `~/.claude.json` before this, so the breakage was invisible on
every machine it happened to, not just the one that reported it.

**Classify the fact, not the cause.** The narrow check — "is this command `bin/edm`?" —
guesses at how the path went missing. The fact is simpler and stronger: *an absolute path
that does not exist cannot be executed*, whatever removed it. That catches the next rename,
a moved umbrella, a wiped `node_modules`. The `bin/edm` shape then earns a **more specific
hint** on top, because charter does know what that one was and what replaced it (ADR 0009).

**Absolute paths only.** A bare `npx` resolves against the PATH of whoever launches the
server, which is not the PATH charter is running under. Asserting a bare name is missing
would be charter guessing about an environment it cannot see — the exact move ADR 0009
forbids, and the way `doctor` cried wolf in 0.31.1 and #177.
"""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path

from charter import doctor
from tests._isolation import PersonaIso


class LauncherCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.home = self.tmp / "home"
        self.home.mkdir(exist_ok=True)
        self._real_home = os.environ.get("HOME")
        os.environ["HOME"] = str(self.home)
        self.addCleanup(self._restore_home)

    def _restore_home(self) -> None:
        if self._real_home is None:
            os.environ.pop("HOME", None)
        else:
            os.environ["HOME"] = self._real_home

    def write_config(self, body: dict) -> Path:
        p = self.home / ".claude.json"
        p.write_text(json.dumps(body, indent=2))
        return p

    def a_real_program(self) -> str:
        """An absolute path that genuinely exists, so "missing" is the only thing under
        test and not an accident of the fixture."""
        p = self.tmp / "launcher.sh"
        p.write_text("#!/bin/sh\nexit 0\n")
        p.chmod(0o755)
        return str(p)

    def check(self):
        return doctor.check_mcp_launchers()


class TestNothingToCheck(LauncherCase):
    def test_no_config_file_is_ok(self):
        """A machine that never registered an MCP server is healthy, not unknown — this
        is a real, fully-supported way to run Claude Code."""
        self.assertEqual(self.check().status, doctor.OK)

    def test_no_servers_registered_is_ok(self):
        self.write_config({"mcpServers": {}})
        self.assertEqual(self.check().status, doctor.OK)

    def test_a_malformed_config_warns_rather_than_passing(self):
        """#171: "not checked" is the absence of information, not evidence of health. A
        green tick over an unreadable file is read as "your launchers are fine"."""
        (self.home / ".claude.json").write_text("{ not json")
        r = self.check()
        self.assertEqual(r.status, doctor.WARN)
        self.assertEqual(r.hint, doctor._NOT_CHECKED_HINT)


class TestAWorkingLauncherPasses(LauncherCase):
    def test_an_existing_absolute_command_is_ok(self):
        self.write_config({"mcpServers": {"good": {"command": self.a_real_program()}}})
        self.assertEqual(self.check().status, doctor.OK)

    def test_a_bare_command_is_never_flagged(self):
        """`npx` resolves against the launcher's PATH, not charter's. Charter cannot see
        that environment, so it does not get to call it broken."""
        self.write_config({"mcpServers": {"viaPath": {"command": "npx"},
                                          "alsoPath": {"command": "definitely-not-installed"}}})
        self.assertEqual(self.check().status, doctor.OK)

    def test_an_entry_with_no_command_is_not_flagged(self):
        """An SSE/HTTP server has a `url`, not a `command`. Absence of a launcher is not a
        missing launcher."""
        self.write_config({"mcpServers": {"remote": {"type": "sse",
                                                     "url": "https://example.invalid/sse"}}})
        self.assertEqual(self.check().status, doctor.OK)


class TestABrokenLauncherFails(LauncherCase):
    def broken(self, command: str, name: str = "elasticsearch") -> None:
        self.write_config({"mcpServers": {name: {"command": command,
                                                 "args": ["secret", "exec", "ops"]}}})

    def test_a_missing_absolute_command_fails(self):
        """FAIL, not WARN, for the reason `check_plugin_skew` chose it: `cmd_doctor` exits
        non-zero only on FAIL, and that exit code is what makes the SessionStart wrapper
        print anything at all. A WARN here would reproduce #197 — a real breakage that
        reached nobody."""
        self.broken(str(self.tmp / "umbrella" / "bin" / "edm"))
        self.assertEqual(self.check().status, doctor.FAIL)

    def test_it_names_the_server(self):
        """"An MCP server is broken" is not actionable; the operator has to know which one
        lost its tools."""
        self.broken(str(self.tmp / "umbrella" / "bin" / "edm"))
        r = self.check()
        self.assertIn("elasticsearch", f"{r.detail} {r.hint}")

    def test_it_names_the_missing_path(self):
        path = str(self.tmp / "umbrella" / "bin" / "edm")
        self.broken(path)
        self.assertIn(path, self.check().hint)

    def test_every_broken_server_is_counted(self):
        """Reporting only the first would let the second stay silent, which is the whole
        defect this check exists for."""
        self.write_config({"mcpServers": {
            "a": {"command": str(self.tmp / "gone-a")},
            "b": {"command": str(self.tmp / "gone-b")},
            "ok": {"command": self.a_real_program()}}})
        r = self.check()
        self.assertEqual(r.status, doctor.FAIL)
        both = f"{r.detail} {r.hint}"
        self.assertIn("a", both)
        self.assertIn("b", both)


class TestTheRenameGetsItsOwnHint(LauncherCase):
    def test_a_removed_edm_shim_is_told_what_replaced_it(self):
        """Charter knows this one: it removed `bin/edm` itself. Naming the replacement
        turns a diagnosis into a one-line fix."""
        self.write_config({"mcpServers": {"es": {"command": str(self.tmp / "u" / "bin" / "edm")}}})
        self.assertIn("charter", self.check().hint)

    def test_a_removed_bin_charter_shim_is_handled_too(self):
        """The umbrella's `bin/charter` shim is the same shape and the same removal."""
        self.write_config({"mcpServers": {"es": {"command": str(self.tmp / "u" / "bin" / "charter")}}})
        self.assertIn("charter", self.check().hint)

    def test_an_unrelated_missing_path_gets_no_rename_claim(self):
        """Charter did not remove someone's `node` build, and must not imply it knows why
        the path is gone — that is the guess ADR 0009 forbids."""
        self.write_config({"mcpServers": {"x": {"command": str(self.tmp / "opt" / "node")}}})
        self.assertNotIn("rename", self.check().hint.lower())


class TestProjectScopedServersAreChecked(LauncherCase):
    def test_a_broken_launcher_under_projects_is_found(self):
        """Claude Code stores project-scoped registrations under `projects.<dir>.mcpServers`.
        Checking only the top level would miss most real registrations."""
        self.write_config({"projects": {"/some/project":
                                        {"mcpServers": {"scoped": {"command": str(self.tmp / "gone")}}}}})
        r = self.check()
        self.assertEqual(r.status, doctor.FAIL)
        self.assertIn("scoped", f"{r.detail} {r.hint}")

    def test_a_project_with_no_servers_is_harmless(self):
        self.write_config({"projects": {"/a": {}, "/b": {"mcpServers": {}}}})
        self.assertEqual(self.check().status, doctor.OK)

    def test_a_malformed_project_entry_does_not_crash_the_check(self):
        """`~/.claude.json` is a large host-owned file charter does not control the shape
        of; one odd value must not take the whole preflight down."""
        self.write_config({"projects": {"/a": "not-a-dict"},
                           "mcpServers": {"ok": {"command": "npx"}}})
        self.assertEqual(self.check().status, doctor.OK)


class TestItIsWiredIn(unittest.TestCase):
    def test_the_check_runs_in_doctor(self):
        """A check nothing calls is the same silence it was written to end."""
        names = [r.name for r in doctor.run_all()]
        self.assertIn("mcp", names)


if __name__ == "__main__":
    unittest.main()
