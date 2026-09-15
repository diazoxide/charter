"""Every reader of Claude Code's install list survives every shape of it.

`<config folder>/plugins/installed_plugins.json` is Claude Code's file, and a chat can write it.
0.62.0 read it in three places that assumed the shape Claude Code writes, and a list in any
other shape raised out of all of them:

* `charter doctor` printed a traceback and no rows — `_checks()` has no per-check guard;
* the SessionStart preflight printed "charter preflight failed" with that traceback at every
  session start;
* `charter init` and `charter reinit` raised in `_ensure_guard_hook`.

The shapes: a top-level array, `plugins` that is not an object, a record anywhere that is not
an object, a version-1 list, records that are a number.

Measured on Claude Code 2.1.272 in throwaway folders: Claude Code checks the whole list before it
loads anything. `version` must be 1 or 2; `plugins` an object of arrays; every record, under
any plugin id, an object whose `scope` is `managed`, `user`, `project` or `local`, whose
`installPath` is a string, and whose `projectPath`, when present, is a string. Any other shape
and it logs "Failed to load installed_plugins.json" and loads no plugin at all. A version-1
list it migrates at start-up, each plugin becoming one `user`-scope install, and loads.

So `doctor` says it could not tell, and `init`/`reinit` write the guard hook as though no
plugin dispatched it. That is the safe direction for wiring: a guard declared twice runs
twice, which is harmless and which `doctor` reports; a guard declared nowhere is the unguarded
plane this family of checks exists for.
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

from charter import commands, config, doctor, guardseen
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN
PID = "charter@charter"

#: Hand-spelled, never imported from the module under test.
_PRETOOLUSE = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
    {"type": "command", "command": "charter hook pretooluse"}]}]}}


class ListShapeCase(PersonaIso):
    """A plane enabling charter's plugin, whose files dispatch the guard, and a session there."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        for var in ("CLAUDE_CONFIG_DIR", "CLAUDE_PLUGIN_ROOT", "CLAUDE_CODE_PLUGIN_CACHE_DIR"):
            os.environ.pop(var, None)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        self.assertTrue(doctor.session_is_the_plane())
        self.plane = Path(config.ROOT).resolve()
        self.plugin = self.tmp / "plugin-cache" / "charter"
        self.write(self.plugin / "hooks" / "hooks.json", _PRETOOLUSE)

    def write(self, path: Path, doc) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))

    @property
    def manifest(self) -> Path:
        return self.home / ".claude" / "plugins" / "installed_plugins.json"

    def enable(self, pid: str = PID) -> None:
        """Reset the plane's settings to enabling *pid* and nothing else."""
        self.write(config.ROOT / ".claude" / "settings.json", {"enabledPlugins": {pid: True}})

    def valid(self, project=None) -> dict:
        return {"scope": "project", "projectPath": str(project or self.plane),
                "installPath": str(self.plugin)}

    def shapes(self):
        """Every shape 0.62.0 raised on or 2.1.272 refuses, a record Claude Code cannot read
        always FIRST — the position the old readers met before any record they could."""
        v = self.valid()
        return (
            ("not JSON", "{not json"),
            ("a top-level array", []),
            ("plugins an array", {"version": 2, "plugins": []}),
            ("plugins a string", {"version": 2, "plugins": "x"}),
            ("no version", {"plugins": {PID: [v]}}),
            # Shaped as version 1 would be, so it is `true`, and not the shape, that refuses it.
            ("version true", {"version": True, "plugins": {PID: {"installPath": str(self.plugin)}}}),
            ("version 3", {"version": 3, "plugins": {PID: [v]}}),
            ("records a number", {"version": 2, "plugins": {PID: 1}}),
            ("records an object", {"version": 2, "plugins": {PID: v}}),
            ("a string record first", {"version": 2, "plugins": {PID: ["junk", v]}}),
            ("a null record first", {"version": 2, "plugins": {PID: [None, v]}}),
            ("a number record first", {"version": 2, "plugins": {PID: [7, v]}}),
            ("a record Claude Code cannot read under another plugin",
             {"version": 2, "plugins": {"other@x": ["junk"], PID: [v]}}),
            ("an unknown scope first",
             {"version": 2, "plugins": {PID: [{**v, "scope": "workspace"}, v]}}),
            ("no installPath first",
             {"version": 2, "plugins": {PID: [{"scope": "project", "projectPath": str(self.plane)}, v]}}),
            ("an installPath that is a number first",
             {"version": 2, "plugins": {PID: [{**v, "installPath": 5}, v]}}),
            ("a null projectPath first",
             {"version": 2, "plugins": {PID: [{**v, "projectPath": None}, v]}}),
            ("a version-1 plugin that is not an object", {"version": 1, "plugins": {PID: "x"}}),
            ("a version-1 plugin with no installPath", {"version": 1, "plugins": {PID: {"version": "1"}}}),
        )


class TestEveryEntryPointSurvivesEveryShape(ListShapeCase):
    def test_the_doctor_rows(self):
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                # From settings, with no settings file declaring it: the one sighting that makes
                # `guard seen` ask the list who declares the guard now.
                guardseen.mark(harness="claude-code", source=guardseen.SETTINGS)
                wired = doctor.check_guard_wired()
                seen = doctor.check_guard_seen()
                self.assertEqual(wired.status, WARN, f"{wired.detail} {wired.hint}")
                self.assertIn("could not tell", wired.detail)
                self.assertIn(seen.status, (OK, WARN))

    def test_the_session_preflight(self):
        """What the SessionStart hook runs: `charter doctor --preflight`, streamed."""
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                out = io.StringIO()
                with redirect_stdout(out):
                    commands.cmd_doctor(SimpleNamespace(fix=False, preflight=True, json=False))
                lines = out.getvalue().splitlines()
                self.assertTrue(any("plane-root guard" in line and "could not tell" in line
                                    for line in lines), out.getvalue())

    def test_inits_guard_check_wires_the_hook(self):
        """`charter init` and `charter reinit` ask this before writing the plane's settings."""
        for label, doc in self.shapes():
            with self.subTest(label):
                self.enable()
                self.write(self.manifest, doc)
                status, _ = commands._ensure_guard_hook(self.plane)
                self.assertEqual(status, "created")
                self.assertIn("charter hook pretooluse",
                              (config.ROOT / ".claude" / "settings.json").read_text())


class TestAListClaudeCodeReadsIsReadTheSameWay(ListShapeCase):
    def test_an_enabled_plugin_installed_nowhere_is_unwired_and_not_could_not_tell(self):
        """No list, or a list with no install of it: nothing is installed, which is knowable."""
        for label, doc in (("no list", None), ("no install of it", {"version": 2, "plugins": {}})):
            with self.subTest(label):
                self.enable()
                if doc is None:
                    self.manifest.unlink(missing_ok=True)
                else:
                    self.write(self.manifest, doc)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
                self.assertIn("pretooluse is not wired", r.detail)
                self.assertNotIn("could not tell", r.detail)

    def test_another_plugin_that_dispatches_the_guard_is_placed_the_same_way(self):
        """The directory question is asked of whichever enabled plugin declares the guard."""
        old = self.plane.parent / f"{self.plane.name}-before-it-moved"
        self.enable("other@x")
        self.write(self.manifest, {"version": 2, "plugins": {"other@x": [self.valid(old)]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("other@x", r.detail)
        self.assertIn(old.name, r.detail)

    def test_an_install_that_dispatches_the_guard_counts_only_if_its_plugin_is_enabled(self):
        """Installed is not enabled (#177), and it stays so when some OTHER plugin is enabled —
        the one case where the list is read at all while charter's own plugin is not enabled."""
        self.enable("other@x")
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})
        self.assertIsNone(doctor._plugin_declaring_guard())
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("pretooluse is not wired", r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")

    def test_an_installPath_with_a_NUL_byte_is_files_gone_and_not_a_crash(self):
        """A string Claude Code accepts, naming no directory anything can read."""
        self.enable()
        self.write(self.manifest, {"version": 2, "plugins": {PID: [
            {**self.valid(), "installPath": "/x\x00y"}]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("files are gone", r.detail)
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "created")


class TestAVersionOneListIsReadAsClaudeCodeMigratesIt(ListShapeCase):
    """Measured: 2.1.272 rewrites a version-1 list at start-up, each plugin becoming one
    `user`-scope install, and loads it in that same session."""

    def test_its_plugin_is_installed_for_every_directory(self):
        self.enable()
        self.write(self.manifest, {"version": 1, "plugins": {PID: {
            "version": "0.62.0", "installedAt": "2026-09-15T13:04:28.952Z",
            "installPath": str(self.plugin)}}})
        self.assertEqual(doctor._plugin_declaring_guard(), PID)
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")
        self.assertEqual(commands._ensure_guard_hook(self.plane)[0], "present")


class TestAListClaudeCodeReadsFromElsewhereIsCouldNotTell(ListShapeCase):
    """`$CLAUDE_CODE_PLUGIN_CACHE_DIR` moves the install list out of the config folder. Measured:
    set to an empty directory, `claude plugin list --json` lists no install; set empty, it lists
    them all. Charter does not follow it, so the list it would read is not the one Claude Code
    reads."""

    def setUp(self) -> None:
        super().setUp()
        self.enable()
        self.write(self.manifest, {"version": 2, "plugins": {PID: [self.valid()]}})
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    def test_set_it_is_could_not_tell_and_init_wires_the_hook(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_PLUGIN_CACHE_DIR": str(self.tmp / "moved")}):
            r = doctor.check_guard_wired()
            status, _ = commands._ensure_guard_hook(self.plane)
        self.assertEqual(r.status, WARN, f"{r.detail} {r.hint}")
        self.assertIn("could not tell", r.detail)
        self.assertIn("CLAUDE_CODE_PLUGIN_CACHE_DIR", r.detail)
        self.assertEqual(status, "created")

    def test_set_empty_claude_code_does_not_follow_it_either(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_PLUGIN_CACHE_DIR": ""}):
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, f"{r.detail} {r.hint}")


if __name__ == "__main__":
    unittest.main()
