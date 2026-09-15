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

Measured again for the review of 30ab6ae, in the same folders: a record with no `projectPath`,
an empty one, or one holding a NUL byte is a record for no directory — Claude Code loads the list
and not that install. A record reaches a session only while its `installPath` is a directory:
with every reaching install's files gone Claude Code loads nothing, and with one of them intact
it loads the plugin. `claude plugin list --json` puts missing files back; a new session does not.
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
_LIST = "claude plugin list --json"


class MovedPlaneCase(PersonaIso):
    """A plane whose plugin is enabled in its own settings, and a session rooted at it."""

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
        self.assertTrue(doctor.session_is_the_plane(),
                        "these tests are about a session rooted at the plane")
        self.plane = Path(config.ROOT).resolve()
        # Where the plane stood when the plugin was installed. Beside it rather than inside it,
        # and never created: `mv` took it away.
        self.old = self.plane.parent / f"{self.plane.name}-before-it-moved"
        self.plugin = self.tmp / "plugin-cache" / "charter"
        self.write(self.plugin / "hooks" / "hooks.json", _PRETOOLUSE)
        # Files that are gone: an install path nothing ever created.
        self.gone = str(self.tmp / "plugin-cache" / "gone")
        self.write(config.ROOT / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}})

    def write(self, path: Path, doc) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))
        return path

    @property
    def manifest(self) -> Path:
        return self.home / ".claude" / "plugins" / "installed_plugins.json"

    def record(self, project, scope: str = "project") -> dict:
        """The record Claude Code writes on `claude plugin install --scope <scope>`."""
        return {"scope": scope, "projectPath": str(project), "installPath": str(self.plugin)}

    def records(self, *entries, pid: str = "charter@charter") -> None:
        """*pid*'s install records exactly as given, shapes Claude Code never writes included."""
        self.write(self.manifest, {"version": 2, "plugins": {pid: list(entries)}})

    def installed_for(self, project, scope: str = "project") -> dict:
        entry = self.record(project, scope)
        self.records(entry)
        return entry

    def settings_block(self) -> None:
        """The plane's settings declare the guard themselves, beside enabling the plugin."""
        self.write(config.ROOT / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}, **_PRETOOLUSE})

    def a_sighting_from_the_plugin(self) -> None:
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    def declared_a_moment_ago(self, pid: str = "charter@charter"):
        """`_plugin_declaring_guard` found the plugin in this same manifest a moment ago."""
        return mock.patch.object(doctor, "_plugin_declaring_guard", return_value=pid)

    @staticmethod
    def text(r) -> str:
        return f"{r.detail} {r.hint or ''}"

    def assert_could_not_tell(self, r) -> None:
        """A sentence that says charter could not tell, names the file, and names the list that
        can answer — not a traceback, and not the row's answer to a question it did not ask."""
        self.assertEqual(r.status, WARN, self.text(r))
        self.assertIn("could not tell", r.detail)
        self.assertIn("installed_plugins.json", r.detail)
        self.assertIn(_LIST, r.hint)
        self.assertIn(_FIX, r.hint)
        self.assertIn("repository", r.hint)


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

    def test_its_hint_names_the_same_repository_rule_the_row_follows(self):
        self.installed_for(self.old)
        self.assertIn("repository", doctor.check_guard_wired().hint)

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
        self.settings_block()
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


class TestEachScopeIsReadAsClaudeCodeReadsIt(MovedPlaneCase):
    """2.1.272 knows four scopes — `managed`, `user`, `project`, `local` — and binds only the last
    two to a directory."""

    def test_a_local_scope_install_is_bound_to_its_directory_as_a_project_one_is(self):
        """Measured: `claude plugin install --scope local` records `"scope": "local"` and the
        directory, enables the plugin in that directory's `settings.local.json`, and after `mv` a
        session at the new path runs none of the six hooks it ran before."""
        self.installed_for(self.old, scope="local")
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        self.assertIn(self.old.name, r.detail)
        self.assertNotIn("could not tell", r.detail)
        self.assertIn(_FIX, r.hint)

    def test_a_local_scope_install_for_this_plane_reaches_it(self):
        self.installed_for(self.plane, scope="local")
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))

    def test_a_managed_install_reaches_every_directory(self):
        self.installed_for(self.old, scope="managed")
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))


class TestAListClaudeCodeCannotReadIsCouldNotTell(MovedPlaneCase):
    """A file charter did not write, in a shape it did not expect, reads as "could not tell" with
    a sentence: never a traceback, and never a pass over the question it could not answer.

    The declaration is stubbed here: it was found in this same file a moment ago, so each whole-file
    shape is the file changing between that read and this one, as `claude plugin install` rewrites
    it. Each case carries a sighting from the plugin, which on its own would turn the row green.
    """

    ABSENT, A_DIRECTORY = object(), object()

    def row_over(self, raw):
        if self.manifest.is_dir():
            shutil.rmtree(self.manifest)
        elif self.manifest.exists():
            self.manifest.unlink()
        if raw is self.A_DIRECTORY:
            self.manifest.mkdir(parents=True)
        elif isinstance(raw, bytes):
            self.manifest.parent.mkdir(parents=True, exist_ok=True)
            self.manifest.write_bytes(raw)
        elif raw is not self.ABSENT:
            self.write(self.manifest, raw)
        self.a_sighting_from_the_plugin()
        with self.declared_a_moment_ago():
            return doctor.check_guard_wired()

    def test_a_manifest_that_is_not_readable_json(self):
        for label, raw in (("absent", self.ABSENT), ("a directory", self.A_DIRECTORY),
                           ("empty", ""), ("not JSON", "{not json"),
                           ("not UTF-8", b"\xff\xfe")):
            with self.subTest(label):
                self.assert_could_not_tell(self.row_over(raw))

    def test_a_manifest_in_a_shape_claude_code_does_not_read(self):
        record = self.record(self.plane)
        for label, raw in (
                ("a list", "[]"), ("a string", '"x"'), ("null", "null"),
                ("no version", {"plugins": {"charter@charter": [record]}}),
                ("version 3", {"version": 3, "plugins": {"charter@charter": [record]}}),
                # Shaped as version 1 would be, so it is `true`, and not the shape, that refuses it.
                ("version true", {"version": True, "plugins": {
                    "charter@charter": {"installPath": str(self.plugin)}}}),
                ("plugins null", {"version": 2, "plugins": None}),
                ("plugins a list", {"version": 2, "plugins": []}),
                ("plugins a string", {"version": 2, "plugins": "x"}),
                ("no record of the plugin", {"version": 2, "plugins": {}}),
                ("its records null", {"version": 2, "plugins": {"charter@charter": None}}),
                ("its records an object", {"version": 2, "plugins": {"charter@charter": record}}),
                ("its records a string", {"version": 2, "plugins": {"charter@charter": "user"}}),
                ("its records a number", {"version": 2, "plugins": {"charter@charter": 1}}),
                ("its records empty", {"version": 2, "plugins": {"charter@charter": []}}),
                ("a version-1 plugin that is not an object",
                 {"version": 1, "plugins": {"charter@charter": "x"}})):
            with self.subTest(label):
                self.assert_could_not_tell(self.row_over(raw))


class TestARecordClaudeCodeCannotReadIsCouldNotTell(MovedPlaneCase):
    """Measured: one record Claude Code cannot read, anywhere in the list and under any plugin,
    and it loads no plugin from the list at all. With the declaration real and that record FIRST,
    so `_plugin_declaring_guard` meets it before any record it can read."""

    def test_a_record_claude_code_cannot_read_comes_first(self):
        elsewhere = self.record(self.old)
        for label, junk in (
                ("a string", "junk"), ("null", None), ("a number", 7),
                ("no installPath", {"scope": "project", "projectPath": str(self.old)}),
                ("an installPath that is a number", {**elsewhere, "installPath": 5}),
                ("a null projectPath", {**elsewhere, "projectPath": None}),
                ("a projectPath that is a number", {**elsewhere, "projectPath": 7}),
                ("a scope Claude Code does not write", {**elsewhere, "scope": "workspace"}),
                ("no scope", {k: v for k, v in elsewhere.items() if k != "scope"})):
            with self.subTest(label):
                self.records(junk, elsewhere)
                self.a_sighting_from_the_plugin()
                self.assert_could_not_tell(doctor.check_guard_wired())

    def test_under_another_plugin_it_unreads_the_list_too(self):
        self.write(self.manifest, {"version": 2, "plugins": {
            "other@x": ["junk"], "charter@charter": [self.record(self.plane)]}})
        self.a_sighting_from_the_plugin()
        self.assert_could_not_tell(doctor.check_guard_wired())

    def test_even_beside_a_record_that_reaches_this_session(self):
        """Not "one record that reaches settles it": Claude Code loads nothing from this list."""
        self.records("junk", self.record(self.plane))
        self.a_sighting_from_the_plugin()
        self.assert_could_not_tell(doctor.check_guard_wired())


class TestARecordForNoDirectoryIsNotThisOne(MovedPlaneCase):
    """2.1.272 places a record with no `projectPath` as not this directory
    (`if(!e.projectPath)return!1`), and loads the list with it. Measured for a missing, an empty and
    a NUL-bearing path. That is knowable, so it is never "could not tell"."""

    def no_directory(self):
        return (("no projectPath", {"scope": "project", "installPath": str(self.plugin)}),
                ("an empty projectPath",
                 {"scope": "project", "projectPath": "", "installPath": str(self.plugin)}))

    def test_it_is_not_installed_here(self):
        nul = ("a projectPath with a NUL byte",
               {"scope": "local", "projectPath": "/x\x00y", "installPath": str(self.plugin)})
        for label, record in (*self.no_directory(), nul):
            with self.subTest(label):
                self.records(record)
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN, self.text(r))
                self.assertNotIn("could not tell", r.detail)
                self.assertIn("not for this directory", r.detail)
                self.assertNotIn("\x00", r.detail)
                self.assertIn(_FIX, r.hint)

    def test_it_is_named_as_no_directory(self):
        for label, record in self.no_directory():
            with self.subTest(label):
                self.records(record)
                self.assertIn("no directory", doctor.check_guard_wired().detail)

    def test_beside_a_record_for_this_plane_the_plugin_loads(self):
        self.records({"scope": "project", "installPath": str(self.plugin)}, self.record(self.plane))
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))


class TestASettingsBlockIsTheGuardWhateverTheListSays(MovedPlaneCase):
    """A `charter hook pretooluse` block in this session's settings runs whatever the plugin does,
    so nothing the list says about the plugin can make the row say no hook runs here."""

    def test_beside_a_list_claude_code_cannot_read(self):
        """The review's probe: a block, and a record with no `installPath` (which 2.1.272 refuses
        the whole list for) beside one for the old path."""
        self.settings_block()
        self.records(self.record(self.old), {"scope": "project"})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))
        self.assertIn("settings.json", r.detail)

    def test_beside_an_install_whose_files_are_gone(self):
        """Only that: no record for another directory beside it, which would carry the row
        green on its own."""
        self.settings_block()
        self.records({**self.record(self.plane), "installPath": self.gone})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))
        self.assertIn("settings.json", r.detail)

    def test_without_the_block_the_same_list_is_could_not_tell(self):
        self.records(self.record(self.old), {"scope": "project"})
        self.assert_could_not_tell(doctor.check_guard_wired())


class TestAnInstallWhoseFilesAreGoneLoadsNothing(MovedPlaneCase):
    """Measured: a record that reaches the session but whose `installPath` is not a directory
    loads nothing ("not cached at"), and with every reaching install in that state no hook runs.
    `claude plugin list --json` puts the files back — as `charter doctor --fix` does, because it
    lists them — and a new session alone does not."""

    def assert_files_gone(self, r) -> None:
        self.assertEqual(r.status, WARN, self.text(r))
        self.assertIn("files are gone", r.detail)
        self.assertNotIn("has fired here", r.detail)
        self.assertIn(_FIX, r.hint)
        self.assertIn(_LIST, r.hint)

    def test_the_reviews_probe(self):
        """A user-scope install whose files are gone, beside a project install for the old path."""
        self.records({"scope": "user", "installPath": self.gone}, self.record(self.old))
        self.a_sighting_from_the_plugin()
        self.assert_files_gone(doctor.check_guard_wired())

    def test_a_project_install_for_this_plane_whose_files_are_gone(self):
        self.records({**self.record(self.plane), "installPath": self.gone}, self.record(self.old))
        self.a_sighting_from_the_plugin()
        self.assert_files_gone(doctor.check_guard_wired())

    def test_charters_own_plugin_with_no_files_left_anywhere(self):
        """No `hooks.json` is left to show the plugin dispatches the guard, so the declaration is
        not found; charter's own id is what names it."""
        self.records({**self.record(self.plane), "installPath": self.gone})
        self.assert_files_gone(doctor.check_guard_wired())

    def test_another_plugins_missing_files_are_not_this_rows_to_report(self):
        self.write(config.ROOT / ".claude" / "settings.json", {"enabledPlugins": {"other@x": True}})
        self.write(self.manifest, {"version": 2, "plugins": {
            "other@x": [{"scope": "user", "installPath": self.gone}],
            "charter@charter": [{**self.record(self.plane), "installPath": self.gone}]}})
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN, self.text(r))
        self.assertIn("pretooluse is not wired", r.detail)
        self.assertNotIn("files are gone", r.detail)

    def test_one_reaching_install_with_its_files_is_enough(self):
        self.records({"scope": "user", "installPath": self.gone}, self.record(self.plane))
        self.a_sighting_from_the_plugin()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK, self.text(r))


class TestNothingTakenFromTheListReachesTheTerminalRaw(MovedPlaneCase):
    """`projectPath`, `installPath`, the plugin id and the folder are text in files a chat can
    write — `mkdir` and `claude plugin install --scope project` make the first. A newline in one
    of them writes a second line in charter's voice; an escape clears the operator's screen."""

    HOSTILE = "x\n\x1b[2Jboom"

    def assert_contained(self, r) -> None:
        self.assertNotIn("\n", r.detail)
        self.assertNotIn("\x1b", r.detail)
        self.assertIn("[2Jboom", r.detail, "contained, not dropped")
        rendered = r.render()
        self.assertNotIn("\x1b[2J", rendered)
        self.assertEqual(rendered.count("\n"), 1, rendered)

    def test_a_projectPath(self):
        self.installed_for(f"{self.old}{self.HOSTILE}")
        self.assert_contained(doctor.check_guard_wired())

    def test_an_installPath_whose_files_are_gone(self):
        self.records({"scope": "user", "installPath": f"{self.gone}{self.HOSTILE}"},
                     self.record(self.old))
        self.assert_contained(doctor.check_guard_wired())

    def test_a_plugin_id(self):
        pid = f"evil{self.HOSTILE}@x"
        self.records(self.record(self.old), pid=pid)
        with self.declared_a_moment_ago(pid):
            self.assert_contained(doctor.check_guard_wired())

    def test_the_config_folder_named_in_could_not_tell(self):
        folder = self.tmp / f"cfg{self.HOSTILE}"
        self.write(folder / "plugins" / "installed_plugins.json", "{not json")
        with mock.patch.dict(os.environ, {"CLAUDE_CONFIG_DIR": str(folder)}), \
                self.declared_a_moment_ago():
            self.assert_contained(doctor.check_guard_wired())


class TestOnlyAnEnabledPluginIsLookedUp(MovedPlaneCase):
    def test_running_under_the_plugin_is_proof_no_record_can_overrule(self):
        """`$CLAUDE_PLUGIN_ROOT` is set only for a process the plugin launched, so the plugin is
        loaded in this very session. The row spells that `wired (Claude Code plugin)`, and the same
        label is how the row knows not to look the plugin up in the manifest — so it is pinned
        here, whole: a reword that reached one use and not the other would send this session's
        proof to a lookup that, over this manifest, could not tell."""
        self.write(self.manifest, "{not json")
        with mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": str(self.plugin)}):
            r = doctor.check_guard_wired()
        self.assertEqual((r.status, r.detail), (OK, "wired (Claude Code plugin)"))

    def test_with_no_plugin_enabled_the_list_is_not_consulted(self):
        self.write(config.ROOT / ".claude" / "settings.json", {})
        self.write(self.manifest, "{not json")
        for label, found in (("none", None), ("empty", "")):
            with self.subTest(label), \
                    mock.patch.object(doctor, "_plugin_declaring_guard", return_value=found):
                r = doctor.check_guard_wired()
                self.assertEqual(r.status, WARN)
                self.assertIn("pretooluse is not wired", r.detail)
                self.assertNotIn("could not tell", r.detail)

    def test_an_enabled_plugin_over_a_list_charter_cannot_read_is_could_not_tell(self):
        """No plugin id was found, because the list could not be read to find one."""
        self.write(self.manifest, "{not json")
        self.assert_could_not_tell(doctor.check_guard_wired())


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
