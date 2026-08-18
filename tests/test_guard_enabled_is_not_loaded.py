"""Enabled is not loaded, and a sighting belongs to the declaration that produced it (#261).

Third variant of one family. #168 was *packaged* read as protection; #177 was *installed*
read as wired; this is **enabled read as loaded**.

A plugin's hooks are loaded by the harness at session start. Install the plugin mid-session
— or follow #251's correct advice and delete the now-duplicate `hooks` block from
`.claude/settings.json` — and the running session holds no declaration at all, while
`check_guard_wired` reports a tick because one is *enabled*. The reporter branched the plane
root in that window and nothing refused it.

`guard seen` made it worse rather than better: it was true and it was about the wrong thing.
The sighting came from the settings block that had just been deleted, and read as evidence
for the plugin that replaced it. A recent sighting of a removed declaration is not evidence
for a surviving one, and until now nothing recorded which declaration a sighting came from.

Both halves are fixed here, and both keep the grammar these rows already use: an observation
with an age, never a verdict (ADR 0013).
"""
from __future__ import annotations

import os
import unittest
from datetime import datetime, timedelta, timezone
from unittest import mock

from charter import config, doctor, guardseen
from tests._isolation import PersonaIso


class GuardCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # A plane the operator has actually worked in, so the "not worked in yet" gate does
        # not silence the rows under test.
        self.make_persona("someone", role="Someone", vault="none")
        (config.ROOT / "charter.toml").write_text('schema = 1\n')
        # `HAS_CONTROL_PLANE` is derived when the isolation fixture points config at the
        # tmp dir — before the charter.toml above exists — and `check_guard_wired` returns
        # early without one. Asserted here rather than worked around, so the fixture says
        # out loud that these rows are about a real plane.
        self.enterContext(mock.patch.object(config, "HAS_CONTROL_PLANE", True))

    def plugin_enabled(self):
        """The plugin is enabled in settings — declared, and loaded only at session start."""
        return mock.patch.object(doctor, "_plugin_declaring_guard", return_value="charter@charter")

    def running_under_plugin(self):
        return mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": "/somewhere"})

    def not_running_under_plugin(self):
        return mock.patch.dict(
            os.environ, {k: v for k, v in os.environ.items() if k != "CLAUDE_PLUGIN_ROOT"},
            clear=True)

    def seen(self, source: str | None, minutes: int = 1) -> None:
        guardseen.mark(harness="claude-code", source=source,
                       when=datetime.now(timezone.utc) - timedelta(minutes=minutes))


class TestASightingRemembersItsDeclaration(GuardCase):
    def test_the_source_is_recorded(self):
        """Two values, not a plugin id: the handler knows whether IT was launched by a
        plugin (`$CLAUDE_PLUGIN_ROOT`), which is the distinction that matters. Recording an
        id would imply charter can tell one enabled plugin's sighting from another's, and
        it cannot."""
        self.seen(guardseen.PLUGIN)
        self.assertEqual(guardseen.last().get("source"), guardseen.PLUGIN)

    def test_an_older_record_without_a_source_still_reads(self):
        """The file is overwritten in place and predates this field; a plane that upgrades
        must not have its history read as corrupt."""
        guardseen.path().parent.mkdir(parents=True, exist_ok=True)
        guardseen.path().write_text('{"ts": "2026-08-01T00:00:00+00:00", "harness": "x"}')
        self.assertIsNotNone(guardseen.last())
        self.assertIsNone(guardseen.last().get("source"))


class TestEnabledIsNotLoaded(GuardCase):
    def test_an_enabled_plugin_with_no_sighting_from_it_is_not_a_tick(self):
        """The reported bug: green while the running session cannot fire the guard."""
        with self.plugin_enabled(), self.not_running_under_plugin():
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.WARN)

    def test_and_it_says_the_session_is_the_unguarded_one(self):
        with self.plugin_enabled(), self.not_running_under_plugin():
            r = doctor.check_guard_wired()
        text = (r.detail + " " + (r.hint or "")).lower()
        self.assertIn("next session", text)

    def test_a_sighting_from_the_plugin_is_the_proof_and_restores_the_tick(self):
        """Reaching the handler is the only thing that proves the declaration is live —
        which is exactly what `guardseen` was created to record."""
        self.seen(guardseen.PLUGIN)
        with self.plugin_enabled(), self.not_running_under_plugin():
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.OK)

    def test_running_under_the_plugin_is_proof_enough_on_its_own(self):
        """`$CLAUDE_PLUGIN_ROOT` means this very process was launched by it."""
        with self.running_under_plugin():
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.OK)

    def test_a_settings_declaration_is_unaffected(self):
        """It is read by the session that is running now, so it was never in doubt."""
        (config.ROOT / ".claude").mkdir(parents=True, exist_ok=True)
        (config.ROOT / ".claude" / "settings.json").write_text(
            '{"hooks": {"PreToolUse": [{"hooks": [{"command": "charter hook pretooluse"}]}]}}')
        with self.not_running_under_plugin():
            r = doctor.check_guard_wired()
        self.assertEqual(r.status, doctor.OK)


class TestASightingDoesNotOutliveItsDeclaration(GuardCase):
    def _settings_declare(self) -> None:
        (config.ROOT / ".claude").mkdir(parents=True, exist_ok=True)
        (config.ROOT / ".claude" / "settings.json").write_text(
            '{"hooks": {"PreToolUse": [{"hooks": [{"command": "charter hook pretooluse"}]}]}}')

    def test_a_sighting_from_a_removed_settings_block_is_not_credited(self):
        """The subtle half of the report. The block that fired minutes ago is gone; saying
        'last ran 0m ago' invites the reader to conclude the survivor is working."""
        self.seen(guardseen.SETTINGS)
        with self.plugin_enabled(), self.not_running_under_plugin():
            r = doctor.check_guard_seen()
        self.assertIn("no longer", (r.detail + " " + (r.hint or "")).lower())

    def test_a_sighting_from_a_declaration_still_present_reads_normally(self):
        self._settings_declare()
        self.seen(guardseen.SETTINGS)
        with self.not_running_under_plugin():
            r = doctor.check_guard_seen()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("last ran", r.detail)

    def test_a_sighting_with_no_recorded_source_is_still_reported(self):
        """Recorded before this field existed. Unknown provenance is not evidence of a
        problem, and inventing one would be the same overreach in the other direction."""
        guardseen.path().parent.mkdir(parents=True, exist_ok=True)
        guardseen.path().write_text('{"ts": "2026-08-18T00:00:00+00:00", "harness": "x"}')
        with self.not_running_under_plugin():
            r = doctor.check_guard_seen()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("last ran", r.detail)


if __name__ == "__main__":
    unittest.main()
