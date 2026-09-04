"""`doctor` reports whether the plane-root guard FIRES, not how charter is packaged (#168).

0.30.0's headline feature lives in `charter hook pretooluse`. If nothing is wired to call
it, no branch move is ever refused — and `check_plugin_skew` printed a green
``✓ not running under the Claude Code plugin`` over exactly that state.

The reporter's framing, which is the whole issue: **the absence of a protection renders as
health.** Someone upgrades specifically to get the guard, runs `doctor` to confirm the
upgrade is healthy, sees all green, and reasonably believes they are protected. They are
not, and this plane had already branched its root six times.

Whether the plane runs as a plugin is an implementation detail; whether the guard fires is
the fact the operator needs.
"""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path

from charter import guardseen, commands, config, doctor
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN


class GuardWiredCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        (self.tmp / "charter.toml").write_text("schema = 1\n")
        config.HAS_CONTROL_PLANE = True
        os.environ.pop("CLAUDE_PLUGIN_ROOT", None)
        # A HOME with no user-level settings, so the check cannot pass on the developer's
        # own machine config and silently prove nothing on CI.
        self.home = self.tmp / "home"
        self.home.mkdir(exist_ok=True)
        self._real_home = os.environ.get("HOME")
        os.environ["HOME"] = str(self.home)
        self.addCleanup(self._restore_home)
        # This session is rooted AT the plane — the case every test below is about, and
        # since #851 something the fixture has to say out loud. `check_guard_wired` reads
        # the settings the host would read for the running session, which is the working
        # directory and never an ancestor of it; without this chdir these tests would
        # write into `config.ROOT` and then read the developer's own checkout, passing or
        # failing on whatever is wired there. Registered after `PersonaIso`'s cleanup so
        # it runs before it (LIFO) and the tmp tree is never removed under our own cwd.
        self.addCleanup(os.chdir, os.getcwd())
        os.chdir(config.ROOT)
        self.assertTrue(doctor.session_is_the_plane(),
                        "these tests are about a session rooted at the plane")

    def _restore_home(self):
        if self._real_home is None:
            os.environ.pop("HOME", None)
        else:
            os.environ["HOME"] = self._real_home

    def settings(self, path: Path, body: dict) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(body, indent=2))

    def wired(self) -> dict:
        return {"hooks": {"PreToolUse": [
            {"matcher": "Bash",
             "hooks": [{"type": "command", "command": "charter hook pretooluse"}]}]}}


class TestAnUnwiredPlaneIsNotGreen(GuardWiredCase):
    def test_it_warns_when_nothing_is_wired(self):
        r = doctor.check_guard_wired()
        self.assertEqual(r.status, WARN)

    def test_it_says_branch_moves_are_not_refused(self):
        """Naming the consequence, not the configuration. The operator's question is "am I
        protected", and "no hook is wired" only answers that if you already know #157 lives
        in a hook."""
        r = doctor.check_guard_wired()
        self.assertIn("NOT refused", f"{r.detail} {r.hint}")

    def test_the_hint_names_a_command_that_wires_it(self):
        self.assertIn("charter reinit", doctor.check_guard_wired().hint)

    def test_doctor_runs_it(self):
        self.assertIn("plane-root guard", {r.name for r in doctor.run_all()})


class TestEveryWiringRouteCounts(GuardWiredCase):
    """A plane that IS wired but gets warned every session teaches people to ignore the
    row — the failure `check_memory_indexes` already records. All four routes count."""

    def _install(self, pid="charter@charter", wires_guard=True, enabled=True):
        """An installed plugin, optionally enabled, optionally wiring the guard."""
        plug = self.tmp / pid.replace("@", "-")
        (plug / "hooks").mkdir(parents=True, exist_ok=True)
        cmd = "charter hook pretooluse" if wires_guard else "other-tool hook"
        (plug / "hooks" / "hooks.json").write_text(json.dumps(
            {"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": cmd}]}]}}))
        man = self.home / ".claude" / "plugins" / "installed_plugins.json"
        man.parent.mkdir(parents=True, exist_ok=True)
        man.write_text(json.dumps({"version": 2, "plugins": {
            pid: [{"scope": "user", "installPath": str(plug)}]}}))
        if enabled:
            s = Path(config.ROOT) / ".claude" / "settings.json"
            self.settings(s, {"enabledPlugins": {pid: True}})
        return plug

    def test_an_installed_but_DISABLED_plugin_does_not_count(self):
        """#177, and the reason 0.31.1 was wrong: installed, enabled and wired are three
        different states, and only the third protects anything. The reporting plane had the
        plugin installed and disabled, and `git checkout -b` in the root succeeded under a
        tick asserting the guard was wired."""
        self._install(enabled=False)
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_an_enabled_plugin_counts(self):
        self._install(enabled=True)
        # #261: enabled is not loaded — a plugin's hooks arrive at session start, so the
        # tick now needs proof the declaration actually fired. This test's own reasoning
        # already rested on that ("the guard demonstrably fired"); it is now checkable.
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_a_plugin_disabled_by_an_explicit_false_does_not_count(self):
        self._install(enabled=False)
        self.settings(Path(config.ROOT) / ".claude" / "settings.json",
                      {"enabledPlugins": {"charter@charter": False}})
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_an_OLD_enabled_plugin_still_counts(self):
        """Deliberately against #177's suggestion to require a minimum plugin version.

        A plugin supplies only the WIRING; the handler is whatever `charter` is on PATH. The
        reporting plane's 0.29.1 plugin predates the guard and its hooks.json still
        dispatches `charter hook pretooluse` — which runs today's CLI, guard included.
        Requiring a version would warn on a plane that is genuinely protected, which is the
        cry-wolf failure 0.31.1 was itself fixing.
        """
        plug = self._install(enabled=True)
        (plug / ".claude-plugin").mkdir(parents=True, exist_ok=True)
        (plug / ".claude-plugin" / "plugin.json").write_text(json.dumps({"version": "0.29.1"}))
        # #261: enabled is not loaded — a plugin's hooks arrive at session start, so the
        # tick now needs proof the declaration actually fired. This test's own reasoning
        # already rested on that ("the guard demonstrably fired"); it is now checkable.
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_an_enabled_plugin_that_wires_something_else_does_not_count(self):
        self._install(wires_guard=False, enabled=True)
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_an_INSTALLED_plugin_counts_even_without_the_env_var(self):
        """The false positive this check shipped with. `CLAUDE_PLUGIN_ROOT` is set only for
        the plugin's OWN processes, so a `charter doctor` a human runs in a terminal never
        sees it — and warned on a machine where the plugin was installed and the guard
        demonstrably fired. Read from `installed_plugins.json`, which is what the host
        actually installed, rather than the plugin cache, which keeps every version ever
        fetched and would answer "wired" for one since removed.
        """
        plug = self.tmp / "plug"
        (plug / "hooks").mkdir(parents=True)
        (plug / "hooks" / "hooks.json").write_text(json.dumps(
            {"hooks": {"PreToolUse": [{"hooks": [
                {"type": "command", "command": "charter hook pretooluse"}]}]}}))
        man = self.home / ".claude" / "plugins" / "installed_plugins.json"
        man.parent.mkdir(parents=True, exist_ok=True)
        man.write_text(json.dumps({"version": 2, "plugins": {
            "charter@charter": [{"scope": "user", "installPath": str(plug)}]}}))
        # Enabled too, since #177: installation alone no longer satisfies the check.
        self.settings(Path(config.ROOT) / ".claude" / "settings.json",
                      {"enabledPlugins": {"charter@charter": True}})
        # And fired, since #261: enabled is not loaded. This test's own reasoning is that
        # the check warned "on a machine where the plugin was installed and the guard
        # demonstrably fired" — the sighting is that demonstration, now recorded.
        guardseen.mark(harness="claude-code", source=guardseen.PLUGIN)
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_a_plugin_that_does_not_declare_the_guard_does_not_count(self):
        plug = self.tmp / "other"
        (plug / "hooks").mkdir(parents=True)
        (plug / "hooks" / "hooks.json").write_text(json.dumps(
            {"hooks": {"SessionStart": [{"hooks": [
                {"type": "command", "command": "other-tool hook"}]}]}}))
        man = self.home / ".claude" / "plugins" / "installed_plugins.json"
        man.parent.mkdir(parents=True, exist_ok=True)
        man.write_text(json.dumps({"version": 2, "plugins": {
            "other@market": [{"scope": "user", "installPath": str(plug)}]}}))
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_the_plugin_counts(self):
        os.environ["CLAUDE_PLUGIN_ROOT"] = str(self.tmp / "plugin")
        self.addCleanup(os.environ.pop, "CLAUDE_PLUGIN_ROOT", None)
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_project_settings_count(self):
        self.settings(Path(config.ROOT) / ".claude" / "settings.json", self.wired())
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_project_local_settings_count(self):
        self.settings(Path(config.ROOT) / ".claude" / "settings.local.json", self.wired())
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_user_settings_count(self):
        self.settings(self.home / ".claude" / "settings.json", self.wired())
        self.assertEqual(doctor.check_guard_wired().status, OK)


class TestItAssertsTheRightHandler(GuardWiredCase):
    def test_wiring_only_another_hook_is_still_unprotected(self):
        """This issue one level down: a plane wiring `sessionstart` looks configured and is
        not guarded. The guard lives in `pretooluse` specifically."""
        self.settings(Path(config.ROOT) / ".claude" / "settings.json",
                      {"hooks": {"SessionStart": [
                          {"hooks": [{"type": "command",
                                      "command": "charter hook sessionstart"}]}]}})
        self.assertEqual(doctor.check_guard_wired().status, WARN)

    def test_an_unreadable_settings_file_does_not_crash_the_check(self):
        p = Path(config.ROOT) / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(b"\xff\xfe not text")
        self.assertEqual(doctor.check_guard_wired().status, WARN)


class TestInitAndReinitWireIt(GuardWiredCase):
    """A safety feature that ships off by default stays off — the reporter's own point."""

    def test_it_writes_the_hook_when_the_file_is_absent(self):
        status, _ = commands._ensure_guard_hook(Path(config.ROOT))
        self.assertEqual(status, "created")
        self.assertEqual(doctor.check_guard_wired().status, OK)

    def test_it_is_idempotent(self):
        commands._ensure_guard_hook(Path(config.ROOT))
        status, _ = commands._ensure_guard_hook(Path(config.ROOT))
        self.assertEqual(status, "present")
        body = json.loads((Path(config.ROOT) / ".claude" / "settings.json").read_text())
        self.assertEqual(len(body["hooks"]["PreToolUse"]), 1)

    def test_it_preserves_keys_charter_does_not_own(self):
        """`_ensure_guard_hook`'s rule, applied: that file is user-owned and holds keys
        charter has no business touching. Only the one entry is added."""
        p = Path(config.ROOT) / ".claude" / "settings.json"
        self.settings(p, {"permissions": {"allow": ["Bash(ls:*)"]},
                          "statusLine": {"type": "command", "command": "charter statusline"}})
        commands._ensure_guard_hook(Path(config.ROOT))
        body = json.loads(p.read_text())
        self.assertEqual(body["permissions"], {"allow": ["Bash(ls:*)"]})
        self.assertIn("statusLine", body)
        self.assertIn("PreToolUse", body["hooks"])

    def test_a_malformed_file_is_left_completely_alone(self):
        p = Path(config.ROOT) / ".claude" / "settings.json"
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("{ not json")
        status, detail = commands._ensure_guard_hook(Path(config.ROOT))
        self.assertEqual(status, "malformed")
        self.assertEqual(p.read_text(), "{ not json")

    def test_an_existing_pretooluse_entry_is_not_duplicated(self):
        """Wiring a second copy would fire the guard twice. Harmless for a denial, but it
        is still charter writing noise into a user's file."""
        p = Path(config.ROOT) / ".claude" / "settings.json"
        self.settings(p, self.wired())
        status, _ = commands._ensure_guard_hook(Path(config.ROOT))
        self.assertEqual(status, "present")


class TestOnlyTheGuardIsWired(GuardWiredCase):
    def test_no_other_hook_is_written(self):
        """Only `pretooluse`, deliberately. If the plugin is installed later, everything
        wired here fires TWICE — idempotent for a denial, but `sessionstart` would render
        the persona briefing, memory digest and todo list twice every session."""
        commands._ensure_guard_hook(Path(config.ROOT))
        body = json.loads((Path(config.ROOT) / ".claude" / "settings.json").read_text())
        self.assertEqual(list(body["hooks"]), ["PreToolUse"])
        blob = json.dumps(body)
        for other in ("sessionstart", "userpromptsubmit", "posttooluse"):
            self.assertNotIn(other, blob, other)


if __name__ == "__main__":
    unittest.main()
