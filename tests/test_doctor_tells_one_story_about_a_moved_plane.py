"""A plane that has moved gets one story from `doctor`, not two.

`charter init` installs charter's Claude Code plugin at project scope, and Claude Code binds
that install to the directory it ran in: `projectPath`, in `claude plugin list --json` and in
`<config folder>/plugins/installed_plugins.json`. Rename or move the plane and the record keeps
the old path, while the plane's `.claude/settings.json`, which enables the plugin, moves with it.

Measured 2026-09-15 on Claude Code 2.1.272, in a throwaway `CLAUDE_CONFIG_DIR` and a throwaway
plane, installed at project scope and then moved with `mv`:

* `claude plugin list --json` at the new path still lists the install, with the old
  `projectPath` and `enabled: true`;
* a session started at the new path loads no plugin ("Found 0 plugins", "Registered 0 hooks
  from 0 plugins") and runs none of charter's hooks, where the same session before the move
  ran six;
* `charter doctor --fix` from the new path adds a second record, for the new path, and the next
  session there loads the plugin and runs all six again — as does a session in a directory
  inside the plane that enables it (a workspace).

So `plugin install`, which asks whether an install covers this plane, was right. `plane-root
guard` was not. It believed the plane's `enabledPlugins` and the manifest without asking which
directory the install belongs to: it said the guard was "wired for the NEXT session" and told
the reader to restart, which changes nothing, and with a sighting from before the move it went
green, "it has fired here".
"""

from __future__ import annotations

import json
import os
import shutil
import tempfile
import unittest
from contextlib import contextmanager
from pathlib import Path
from unittest import mock

from charter import config, doctor, guardseen, plugincache
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN

#: Hand-spelled, never imported from the module under test.
_PRETOOLUSE = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
    {"type": "command", "command": "charter hook pretooluse"}]}]}}
_FIX = "charter doctor --fix"


class MovedPlaneCase(PersonaIso):
    """A plane whose plugin is enabled in its own settings, and a session rooted at it."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True)
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        os.environ.pop("CLAUDE_CONFIG_DIR", None)
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        self.assertTrue(doctor.session_is_the_plane(),
                        "these tests are about a session rooted at the plane")
        self.plane = Path(config.ROOT).resolve()
        # Where the plane stood when the plugin was installed. Beside it rather than inside it,
        # and never created: `mv` took it away.
        self.old = self.plane.parent / f"{self.plane.name}-before-it-moved"
        self.plugin = self.tmp / "plugin-cache" / "charter"
        self.write(self.plugin / "hooks" / "hooks.json", _PRETOOLUSE)
        self.write(config.ROOT / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}})

    def write(self, path: Path, doc) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))
        return path

    def installed_for(self, project: Path, scope: str = "project") -> dict:
        """The record Claude Code writes on `claude plugin install --scope project`."""
        entry = {"scope": scope, "projectPath": str(project), "installPath": str(self.plugin)}
        self.write(self.home / ".claude" / "plugins" / "installed_plugins.json",
                   {"version": 2, "plugins": {"charter@charter": [entry]}})
        return entry

    def a_sighting_from_the_plugin(self) -> None:
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    @staticmethod
    def text(r) -> str:
        return f"{r.detail} {r.hint or ''}"


class TestTheGuardRowAsksWhichDirectoryTheInstallBelongsTo(MovedPlaneCase):
    def test_an_install_recorded_for_the_old_path_does_not_guard_a_session_here(self):
        self.installed_for(self.old)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        self.assertIn("NOT refused", self.text(r))
        self.assertIn(self.old.name, self.text(r),
                      "the row names the directory the install is recorded for")

    def test_its_remedy_is_the_install_measured_to_work_and_not_a_restart(self):
        """A restart was the old advice. Measured, it loads nothing: the record is still for
        the old path."""
        self.installed_for(self.old)
        r = doctor.check_guard_wired()
        self.assertIn(_FIX, r.hint)
        self.assertNotIn("wired for the NEXT session", r.detail)
        self.assertNotIn("Restart the session", r.hint)

    def test_a_sighting_from_before_the_move_does_not_turn_it_green(self):
        """The guard did fire from this plugin, under this config folder, when the plane stood
        at its old path. That is not evidence for a session at the new one."""
        self.installed_for(self.old)
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        self.assertNotIn("has fired here", r.detail)

    def test_a_settings_block_is_what_guards_it_and_is_not_called_a_duplicate(self):
        """The "declared twice" advice is to delete the settings block and keep the plugin. On a
        moved plane the block is the only declaration a session here loads, so following that
        advice would leave the plane root unguarded."""
        self.write(config.ROOT / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}, **_PRETOOLUSE})
        self.installed_for(self.old)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertNotIn("declared twice", r.detail)
        self.assertIn("settings.json", r.detail)


class TestAnInstallThatDoesReachTheSessionIsUnchanged(MovedPlaneCase):
    """The controls. Without them the tests above would pass on a row that warned about every
    project-scope install."""

    def test_an_install_recorded_for_this_plane_is_wired_once_it_has_fired(self):
        self.installed_for(self.plane)
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))

    def test_a_workspace_session_is_reached_by_the_planes_install(self):
        """Measured: a session in a directory inside the plane that enables the plugin loads the
        install recorded for the plane root."""
        self.installed_for(self.plane)
        ws = config.ROOT / "workspaces" / "fleet"
        self.write(ws / ".claude" / "settings.json", {"enabledPlugins": {"charter@charter": True}})
        os.chdir(ws.resolve())
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))

    def test_an_install_recorded_inside_the_plane_reaches_a_session_at_its_root(self):
        """Claude Code also counts an install recorded for another directory of the same
        repository, and inside the plane is how this row approximates that: the record a
        `claude plugin install --scope project` run from a workspace chat writes."""
        self.installed_for(self.plane / "workspaces" / "fleet")
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))

    def test_an_install_recorded_for_the_sessions_own_directory_reaches_it(self):
        """Even when that directory is outside the plane, as it is when `$CHARTER_ROOT` names
        the plane: `projectPath` equal to the session's directory is Claude Code's first rule."""
        outside = Path(tempfile.mkdtemp(prefix="charter-session-elsewhere-")).resolve()
        self.addCleanup(shutil.rmtree, outside, True)
        self.write(outside / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}})
        self.installed_for(outside)
        os.chdir(outside)
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))

    def test_a_user_scope_install_reaches_every_directory(self):
        self.installed_for(self.old, scope="user")
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))


class TestBothRowsTellTheSameStory(MovedPlaneCase):
    @contextmanager
    def listed(self, entries):
        """`claude plugin list --json` answering with *entries*."""
        def fake(args, cwd=None, timeout=None, **kw):
            return entries if args[:1] == ["list"] else []

        with mock.patch.object(plugincache, "available", return_value=True), \
                mock.patch.object(plugincache, "_claude_json", side_effect=fake):
            yield

    def test_both_rows_warn_and_name_the_same_fix(self):
        recorded = self.installed_for(self.old)
        with self.listed([{"id": "charter@charter", "enabled": True, **recorded}]):
            install = doctor.check_plugin_install()
        guard = doctor.check_guard_wired()
        self.assertEqual((install.status, guard.status), (WARN, WARN))
        self.assertIn(_FIX, install.hint)
        self.assertIn(_FIX, guard.hint)


if __name__ == "__main__":
    unittest.main()
