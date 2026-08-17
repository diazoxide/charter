"""A plane's version pin is measured against the PLUGIN, which is per-project.

#127 reported that a pin cannot be honoured: `charter` is one machine-global install, so
two planes with different pins thrash the same binary and `version sync` breaks whichever
plane you are not standing in. It was parked as impossible.

Its stated premise — "there is no per-version store, and no shim that resolves the pinned
version from the plane you are standing in" — is false. Claude Code already keeps one::

    ~/.claude/plugins/cache/charter/charter/{0.1.0 … 0.38.1}   19 full source trees

and `installed_plugins.json` resolves it **per project**::

    {"scope": "project", "projectPath": "…/easydmarc-umbrella",
     "installPath": "…/cache/charter/charter/0.38.0"}

Two projects on this machine were observed running two different charter versions at once,
which is exactly what #127 said could not happen.

So the pin stops being measured against the machine-global binary — which no plane can own
— and is measured against the thing that is genuinely per-plane. The fix that follows is
`claude plugin update charter@charter`, which touches THIS project only; the old advice,
`charter version sync`, conforms a binary every plane shares and is what put the other
plane into drift.

**The plugin's own root is read from ``$CLAUDE_PLUGIN_ROOT``, which is documented.** The
cache layout and `installed_plugins.json` are Claude Code internals: fine to read as a
best-effort diagnosis, never something to build dispatch on. That was the bet `bin/edm`
made, and it broke silently (#197).
"""

from __future__ import annotations

import io
import json
import os
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace

from charter import commands, doctor, update
from tests._isolation import PersonaIso


class PinCase(PersonaIso):
    def pin(self, version: str) -> None:
        (self.tmp / "charter.toml").write_text(
            f'schema = 1\n\n[charter]\nversion = "{version}"\n')

    def as_plugin(self, version: str) -> None:
        """Run as though under the Claude Code plugin serving this project."""
        root = self.tmp / "plugin"
        (root / ".claude-plugin").mkdir(parents=True, exist_ok=True)
        (root / ".claude-plugin" / "plugin.json").write_text(
            json.dumps({"name": "charter", "version": version}))
        os.environ["CLAUDE_PLUGIN_ROOT"] = str(root)
        self.addCleanup(os.environ.pop, "CLAUDE_PLUGIN_ROOT", None)

    def no_plugin(self) -> None:
        self._had = os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        if self._had is not None:
            self.addCleanup(os.environ.__setitem__, "CLAUDE_PLUGIN_ROOT", self._had)


class TestThePluginVersionIsReadable(PinCase):
    def test_it_is_none_outside_the_plugin(self):
        """A bare `charter` in a terminal has no plugin to ask, and must say so rather than
        guess — the same shape `check_plugin_skew` already keeps."""
        self.no_plugin()
        self.assertIsNone(update.plugin_version_here())

    def test_it_reads_the_manifest_of_the_plugin_serving_this_project(self):
        self.as_plugin("0.27.0")
        self.assertEqual(update.plugin_version_here(), "0.27.0")

    def test_an_unreadable_manifest_is_none_not_an_exception(self):
        self.as_plugin("0.27.0")
        (self.tmp / "plugin" / ".claude-plugin" / "plugin.json").write_text("{ not json")
        self.assertIsNone(update.plugin_version_here())


class TestTheLockIsMeasuredAgainstThePlugin(PinCase):
    def test_a_matching_plugin_is_in_sync(self):
        self.pin("0.27.0")
        self.as_plugin("0.27.0")
        r = doctor.check_version_lock()
        self.assertEqual(r.status, doctor.OK)

    def test_a_drifting_plugin_warns(self):
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        self.assertEqual(doctor.check_version_lock().status, doctor.WARN)

    def test_the_fix_is_scoped_to_this_project(self):
        """The whole point of #127. `charter version sync` conforms a machine-global
        binary and puts every other plane into drift; the plugin update touches one
        project."""
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        hint = doctor.check_version_lock().hint
        self.assertIn("claude plugin update", hint)

    def test_the_fix_is_no_longer_the_machine_global_install(self):
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        hint = doctor.check_version_lock().hint
        self.assertNotIn("uv tool install", hint)

    def test_it_names_both_versions(self):
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        said = doctor.check_version_lock()
        self.assertIn("0.27.0", said.detail)
        self.assertIn("0.38.1", said.detail)

    def test_pinning_nothing_is_still_a_normal_state(self):
        """Opt-in, and a plane that pins nothing is not a nag."""
        self.as_plugin("0.38.1")
        self.assertEqual(doctor.check_version_lock().status, doctor.OK)


class TestOutsideThePluginItStaysHonest(PinCase):
    def test_it_does_not_claim_the_plugin_agrees(self):
        """From a plain terminal charter cannot see which plugin serves this project. The
        old check compared the pin to the machine-global CLI and called that the plane's
        version, which is the conflation #127 is about."""
        self.pin("0.27.0")
        self.no_plugin()
        r = doctor.check_version_lock()
        self.assertNotIn("plugin in sync", f"{r.detail} {r.hint}")

    def test_it_still_says_the_binary_is_shared(self):
        self.pin("0.27.0")
        self.no_plugin()
        r = doctor.check_version_lock()
        if r.status != doctor.OK:
            self.assertIn("machine-global", f"{r.detail} {r.hint}")

    def test_it_never_raises_on_a_broken_config(self):
        (self.tmp / "charter.toml").write_text("[charter\nversion =")
        self.assertIn(doctor.check_version_lock().status, (doctor.OK, doctor.WARN))


class TestSyncTargetsTheProject(PinCase):
    def run_sync(self, **kw) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = commands.cmd_version_sync(SimpleNamespace(**{"cli": False, **kw}))
        return rc, out.getvalue() + err.getvalue()

    def test_it_names_the_per_project_plugin_command_by_default(self):
        """#127: "`charter version sync` does not resolve this, it just moves the problem"
        — it conformed the shared binary, so two planes thrashed it."""
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        _, said = self.run_sync()
        self.assertIn("claude plugin update", said)

    def test_it_does_not_touch_the_global_install_by_default(self):
        """The behaviour that broke the other plane. It must now be asked for."""
        self.pin("0.27.0")
        self.as_plugin("0.38.1")
        called = []
        real = commands.sync_to
        commands.sync_to = lambda v: (called.append(v), (True, v))[1]
        self.addCleanup(setattr, commands, "sync_to", real)
        self.run_sync()
        self.assertEqual(called, [])

    def test_the_old_behaviour_is_still_reachable_explicitly(self):
        """Not removed — a machine with no plugin at all still needs it, and taking away
        the escape hatch would be its own defect."""
        self.pin("0.27.0")
        self.no_plugin()
        called = []
        real = commands.sync_to
        commands.sync_to = lambda v: (called.append(v), (True, v))[1]
        self.addCleanup(setattr, commands, "sync_to", real)
        self.run_sync(cli=True)
        self.assertEqual(called, ["0.27.0"])

    def test_pinning_nothing_syncs_nothing(self):
        _, said = self.run_sync()
        self.assertIn("pins no version", said)


if __name__ == "__main__":
    unittest.main()
