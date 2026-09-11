"""`doctor` answers for the Claude Code config folder in use, not for `~/.claude` (#969).

Fifth variant of the family #168, #177, #261 and #851 belong to — a checker reading a proxy
instead of the fact. *Packaged* read as protection, *installed* read as wired, *enabled* read
as loaded, *elsewhere* read as here. This one is **the default config folder read as the one
in use**.

`$CLAUDE_CONFIG_DIR` points Claude Code at another folder, typically for a second account.
The `plugin install` row asks `claude plugin list --json`, which follows it. The `plane-root
guard` row read `~/.claude/plugins/installed_plugins.json` directly, the settings rows read
`~/.claude/settings.json`, and the `mcp` row read `~/.claude.json`. So one report said the
plugin was not installed for this plane and, rows earlier, that the guard was wired and had
fired here — while under that folder no charter hook ran at all.

And "has fired here" was a sighting from an earlier session, under whichever folder THAT
session used. A sighting is evidence for the folder it ran under and for no other.

Every fixture states `$HOME`, `$CLAUDE_CONFIG_DIR` and `$CLAUDE_PLUGIN_ROOT` itself, so none of
this can pass on the developer's own Claude Code install and prove nothing on CI.
"""

from __future__ import annotations

import json
import os
import unittest
from datetime import datetime, timezone
from pathlib import Path
from unittest import mock

from charter import commands, config, doctor, guardseen
from charter.harness import claude_code
from tests._isolation import PersonaIso

OK, WARN, FAIL = doctor.OK, doctor.WARN, doctor.FAIL

#: Hand-spelled, never imported from the module under test — a report compared against the
#: constant that spells it asserts nothing.
_PRETOOLUSE = {"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
    {"type": "command", "command": "charter hook pretooluse"}]}]}}


class ConfigFolderCase(PersonaIso):
    """A plane that enables charter's plugin, a home folder, and a second config folder."""

    def setUp(self) -> None:
        super().setUp()
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))
        self.home = self.tmp / "home"
        (self.home / ".claude").mkdir(parents=True)
        self.other = self.tmp / "second-account"
        self.other.mkdir()
        # Every environment value these rows depend on, stated. `patch.dict` restores the
        # whole mapping on exit, so the two removals below are undone with it.
        self.enterContext(mock.patch.dict(os.environ, {"HOME": str(self.home)}))
        os.environ.pop("CLAUDE_CONFIG_DIR", None)
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        # Rooted at the plane, so the plane's `.claude/settings.json` is the project file the
        # host would read for this "session" (#851).
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        self.assertTrue(doctor.session_is_the_plane(),
                        "these tests are about a session rooted at the plane")
        self._plugin: Path | None = None

    # --- the operator's shell -------------------------------------------------------------

    def use_folder(self, folder: Path) -> None:
        """Export `$CLAUDE_CONFIG_DIR`, as a second-account shell does."""
        os.environ["CLAUDE_CONFIG_DIR"] = str(folder)

    def use_the_default_folder(self) -> None:
        os.environ.pop("CLAUDE_CONFIG_DIR", None)

    # --- what Claude Code would find ------------------------------------------------------

    def plugin(self) -> Path:
        """An installed copy of a plugin whose hooks dispatch the guard."""
        if self._plugin is None:
            self._plugin = self.tmp / "plugin-cache" / "charter"
            (self._plugin / "hooks").mkdir(parents=True)
            (self._plugin / "hooks" / "hooks.json").write_text(json.dumps(_PRETOOLUSE))
        return self._plugin

    def install_in(self, folder: Path) -> None:
        """charter's plugin in *folder*'s manifest — the file Claude Code writes on install."""
        man = folder / "plugins" / "installed_plugins.json"
        man.parent.mkdir(parents=True, exist_ok=True)
        man.write_text(json.dumps({"version": 2, "plugins": {"charter@charter": [
            {"scope": "project", "projectPath": str(config.ROOT),
             "installPath": str(self.plugin())}]}}))

    def enable_in_the_plane(self) -> None:
        p = config.ROOT / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"enabledPlugins": {"charter@charter": True}}))

    def a_sighting_under(self, folder: Path | None) -> None:
        """The plugin's guard firing in a session that used *folder* (``None``: the default).

        The environment is the only thing a hook process knows about its config folder, so
        the sighting is made with that environment in force."""
        if folder is None:
            self.use_the_default_folder()
        else:
            self.use_folder(folder)
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)

    def write(self, path: Path, doc) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(doc if isinstance(doc, str) else json.dumps(doc))
        return path

    def rooted_in_a_workspace(self) -> Path:
        ws = (config.ROOT / "workspaces" / "fleet")
        ws.mkdir(parents=True, exist_ok=True)
        ws = ws.resolve()
        os.chdir(ws)
        return ws


class TestAFolderWithoutThePluginIsNotGreen(ConfigFolderCase):
    """The report, reproduced: installed and fired under `~/.claude`, run under another."""

    def setUp(self) -> None:
        super().setUp()
        self.install_in(self.home / ".claude")
        self.enable_in_the_plane()
        self.a_sighting_under(None)

    def test_the_default_folder_is_still_green(self):
        """The control. Without it the case below would pass on a row that is never green."""
        self.use_the_default_folder()
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn("and it has fired here", r.detail)

    def test_a_named_folder_without_the_plugin_is_not_green(self):
        self.use_folder(self.other)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        self.assertIn("NOT refused", r.detail)
        self.assertNotIn("and it has fired here", r.detail)


class TestThePluginInTheNamedFolderIsTheOneRead(ConfigFolderCase):
    def test_an_install_only_in_the_named_folder_wires_the_guard(self):
        """The other direction. Refusing `~/.claude` is half a fix; a second account whose
        own folder carries the plugin is wired, and the row has to find it there."""
        self.install_in(self.other)
        self.enable_in_the_plane()
        self.a_sighting_under(self.other)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn("and it has fired here", r.detail)


class TestSettingsComeFromTheFolderInUse(ConfigFolderCase):
    def test_the_user_settings_file_is_the_default_folders_when_nothing_names_one(self):
        self.assertEqual(doctor._settings_files()[2], self.home / ".claude" / "settings.json")

    def test_the_user_settings_file_is_the_named_folders(self):
        self.use_folder(self.other)
        self.assertEqual(doctor._settings_files()[2], self.other / "settings.json")

    def test_a_hook_only_in_the_default_folders_settings_does_not_wire_a_named_one(self):
        self.write(self.home / ".claude" / "settings.json", _PRETOOLUSE)
        self.assertEqual(doctor.check_guard_wired().status, OK, "the control")
        self.use_folder(self.other)
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_a_hook_in_the_named_folders_settings_wires_it(self):
        declared = self.write(self.other / "settings.json", _PRETOOLUSE)
        self.use_folder(self.other)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn(str(declared), r.detail)

    def test_the_remedy_names_the_named_folders_settings_file(self):
        """A session rooted below the plane is told where to declare the guard. Naming
        `~/.claude/settings.json` to a session that never reads it is #851's remedy-that-
        looks-like-a-fix, one folder over."""
        self.rooted_in_a_workspace()
        self.use_folder(self.other)
        hint = doctor.check_guard_wired().hint
        self.assertIn(str(self.other / "settings.json"), hint)
        self.assertNotIn("~/.claude/settings.json", hint)

    def test_the_session_root_row_names_the_folder_it_read(self):
        self.rooted_in_a_workspace()
        self.assertIn("~/.claude/", doctor.check_session_root().detail, "the control")
        self.use_folder(self.other)
        detail = doctor.check_session_root().detail
        self.assertIn(str(self.other), detail)
        self.assertNotIn("~/.claude/", detail)

    def test_the_session_root_row_names_it_for_a_harness_charter_has_not_measured(self):
        """The sentence has a second spelling, for a harness with no discovery rules."""
        self.rooted_in_a_workspace()
        self.use_folder(self.other)
        with mock.patch.object(doctor, "_discovery_rules", return_value=([], [])):
            detail = doctor.check_session_root().detail
        self.assertIn(str(self.other), detail)
        self.assertNotIn("~/.claude/", detail)


class TestMcpReadsTheClaudeJsonInsideTheFolderInUse(ConfigFolderCase):
    """Measured on 2.1.268: with `$CLAUDE_CONFIG_DIR` set, Claude Code writes `.claude.json`
    inside that folder, so its `mcpServers` are there and not in `~/.claude.json`."""

    def broken(self) -> dict:
        return {"mcpServers": {"gone": {"command": str(self.tmp / "nowhere" / "bin" / "srv")}}}

    def test_a_broken_launcher_registered_under_the_named_folder_fails(self):
        self.write(self.other / ".claude.json", self.broken())
        self.use_folder(self.other)
        self.assertEqual(doctor.check_mcp_launchers().status, FAIL)

    def test_the_default_claude_json_is_not_read_under_a_named_folder(self):
        self.write(self.home / ".claude.json", self.broken())
        self.assertEqual(doctor.check_mcp_launchers().status, FAIL, "the control")
        self.use_folder(self.other)
        self.assertEqual(doctor.check_mcp_launchers().status, OK)

    def test_an_unreadable_file_is_named_by_the_path_it_was_read_from(self):
        self.write(self.other / ".claude.json", "{ not json")
        self.use_folder(self.other)
        r = doctor.check_mcp_launchers()
        self.assertEqual(r.status, WARN)
        self.assertIn(str(self.other / ".claude.json"), r.detail)


class TestTheFolderIsResolvedTheWayClaudeCodeResolvesIt(ConfigFolderCase):
    """Read off the 2.1.268 binary rather than guessed:

    * the config home is ``(CLAUDE_CONFIG_DIR ?? join(homedir(), ".claude")).normalize("NFC")``
    * `.claude.json` is ``<home>/.config.json`` when that exists, else
      ``join(CLAUDE_CONFIG_DIR || homedir(), ".claude.json")``

    `??` and `||` are not the same operator, and the empty string is where they part."""

    def test_nothing_named_is_the_home_folder(self):
        self.assertEqual(claude_code.config_home(), self.home / ".claude")
        self.assertEqual(claude_code.global_config_file(), self.home / ".claude.json")

    def test_a_named_folder_holds_both(self):
        self.use_folder(self.other)
        self.assertEqual(claude_code.config_home(), self.other)
        self.assertEqual(claude_code.global_config_file(), self.other / ".claude.json")

    def test_an_empty_value_names_the_working_directory_not_home(self):
        """Measured: `CLAUDE_CONFIG_DIR= claude plugin list --json` listed a plugin from the
        `plugins/installed_plugins.json` in the directory it ran in. An empty string is not
        nullish, so `??` keeps it, and a relative path is relative to the working directory."""
        os.environ["CLAUDE_CONFIG_DIR"] = ""
        self.assertEqual(claude_code.config_home(), Path(""))

    def test_an_empty_value_still_leaves_claude_json_in_home(self):
        """`||` treats the empty string as absent, so that one file falls back to home."""
        os.environ["CLAUDE_CONFIG_DIR"] = ""
        self.assertEqual(claude_code.global_config_file(), self.home / ".claude.json")

    def test_a_legacy_config_json_in_the_folder_is_the_file_read(self):
        self.use_folder(self.other)
        legacy = self.write(self.other / ".config.json", "{}")
        self.write(self.other / ".claude.json", "{}")
        self.assertEqual(claude_code.global_config_file(), legacy)

    def test_the_folder_is_nfc_normalised_and_the_claude_json_parent_is_not(self):
        """On a filesystem that keeps normalisation forms apart, a decomposed `é` and a
        composed one are two directories. The binary normalises the config home and joins
        `.claude.json` onto the raw value, so both halves are pinned as it spells them."""
        # Escapes, not literals: an editor or a copy-paste can silently turn two spellings
        # of an accented letter into one, and this would then compare a string with itself.
        decomposed = str(self.tmp / "cafe\u0301")
        composed = str(self.tmp / "caf\u00e9")
        self.assertNotEqual(decomposed, composed)
        os.environ["CLAUDE_CONFIG_DIR"] = decomposed
        self.assertEqual(str(claude_code.config_home()), composed)
        self.assertEqual(str(claude_code.global_config_file()),
                         str(Path(decomposed) / ".claude.json"))


class TestAPastSightingDoesNotVouchForAnotherFolder(ConfigFolderCase):
    """Both folders carry the plugin, so the only thing left to decide the row is the
    sighting — and a sighting belongs to the folder it ran under (#261's rule, one field on)."""

    def setUp(self) -> None:
        super().setUp()
        self.install_in(self.home / ".claude")
        self.install_in(self.other)
        self.enable_in_the_plane()

    def test_a_sighting_under_the_default_folder_does_not_vouch_for_a_named_one(self):
        self.a_sighting_under(None)
        self.use_folder(self.other)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        self.assertNotIn("and it has fired here", r.detail)

    def test_the_warning_names_the_folder_it_answers_for(self):
        """`guard seen` still reads "last ran 0m ago" on the next row, which is true. Without
        the folder here the two rows would contradict each other, which is #969 again."""
        self.a_sighting_under(None)
        self.use_folder(self.other)
        self.assertIn(str(self.other), doctor.check_guard_wired().detail)

    def test_a_sighting_under_the_named_folder_does(self):
        self.a_sighting_under(self.other)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn("and it has fired here", r.detail)

    def test_a_sighting_from_before_the_folder_was_recorded_does_not_vouch(self):
        """Unknown is not suspect, and it is not evidence either. The same call #261 made
        for a sighting with no `source`: it clears on the next guarded Bash call."""
        p = guardseen.path()
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(json.dumps({"ts": datetime.now(timezone.utc).isoformat(timespec="seconds"),
                                 "harness": "claude-code", "source": guardseen.PLUGIN}))
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_a_sighting_records_the_folder_it_ran_under(self):
        self.a_sighting_under(self.other)
        self.assertEqual(guardseen.last()["claude_config_dir"], str(self.other))
        self.assertEqual(guardseen.last_claude_config_dir(), str(self.other))

    def test_no_sighting_has_no_folder(self):
        self.assertIsNone(guardseen.last_claude_config_dir())

    def test_a_folder_that_cannot_be_resolved_still_records_the_sighting(self):
        """`mark` runs inside the guard and never raises. `Path.home()` can, with no `$HOME`
        and no passwd entry — and a sighting with no folder simply cannot vouch."""
        with mock.patch.object(claude_code, "config_home",
                               side_effect=RuntimeError("Could not determine home directory.")):
            self.assertIsNotNone(guardseen.mark(harness="claude-code", source=guardseen.PLUGIN))
        rec = guardseen.last()
        self.assertIn("claude_config_dir", rec)
        self.assertIsNone(rec["claude_config_dir"])


class TestARelativeFolderIsTheOneItNamedWhereItWasRead(ConfigFolderCase):
    """`$CLAUDE_CONFIG_DIR=cfg` names a different folder in every directory it is read from.

    Recorded as spelled, a sighting made in the plane root compared equal to a `doctor` run
    from a workspace, where ``cfg`` is somewhere else — a false green by string equality."""

    def setUp(self) -> None:
        super().setUp()
        self.ws = self.rooted_in_a_workspace()
        # Both directories carry the plugin under `cfg` and the session's own settings enable
        # it, so the only thing left to decide the row is which folder the sighting names.
        self.install_in(config.ROOT / "cfg")
        self.install_in(self.ws / "cfg")
        self.write(self.ws / ".claude" / "settings.json",
                   {"enabledPlugins": {"charter@charter": True}})
        os.environ["CLAUDE_CONFIG_DIR"] = "cfg"

    def fired_in(self, where: Path) -> None:
        os.chdir(where)
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        os.chdir(self.ws)

    def test_fired_in_the_plane_root_it_does_not_vouch_for_a_workspace(self):
        self.fired_in(config.ROOT)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)
        # The plugin's own warning, not "not wired": the plugin IS found here, so a WARN for
        # any other reason would pass without the comparison ever being reached.
        self.assertIn("nothing has fired here under", r.detail)

    def test_fired_in_the_workspace_itself_it_does(self):
        self.fired_in(self.ws)
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, OK)
        self.assertIn("and it has fired here", r.detail)

    def test_a_working_directory_that_is_gone_still_records_the_sighting(self):
        """Making a relative folder absolute asks for the working directory, and a deleted
        one raises. `mark` runs inside the guard, which never fails a turn."""
        with mock.patch("os.getcwd", side_effect=FileNotFoundError("gone")):
            self.assertIsNotNone(guardseen.mark(harness="claude-code", source=guardseen.PLUGIN))
        self.assertIsNone(guardseen.last()["claude_config_dir"])


class TestReinitAsksTheFolderDoctorAsks(ConfigFolderCase):
    """`charter reinit` skips writing the guard hook when an enabled plugin already dispatches
    it, and it asks `doctor`'s own function, because a writer and a checker answering "is this
    wired?" from different evidence is how the guard came to be declared twice. So it follows
    `$CLAUDE_CONFIG_DIR` too. Kept on `~/.claude`, a second-account shell would hear `doctor`
    say the guard is not wired and `reinit` answer that nothing needs doing."""

    def setUp(self) -> None:
        super().setUp()
        self.install_in(self.home / ".claude")
        self.enable_in_the_plane()

    def test_the_default_folder_sees_the_plugin(self):
        self.assertEqual(commands._plugin_dispatches_guard(config.ROOT), "charter@charter")

    def test_a_folder_without_the_plugin_does_not(self):
        self.use_folder(self.other)
        self.assertIsNone(commands._plugin_dispatches_guard(config.ROOT))

    def test_so_reinit_wires_the_hook_only_for_the_folder_without_the_plugin(self):
        settings = config.ROOT / ".claude" / "settings.json"
        self.assertEqual(commands._ensure_guard_hook(config.ROOT)[0], "present")
        self.assertNotIn("charter hook pretooluse", settings.read_text())
        self.use_folder(self.other)
        self.assertEqual(commands._ensure_guard_hook(config.ROOT)[0], "created")
        self.assertIn("charter hook pretooluse", settings.read_text())


if __name__ == "__main__":
    unittest.main()
