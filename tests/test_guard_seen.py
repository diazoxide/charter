"""Declared is not dispatched, and only one of the two can be read off configuration.

`check_guard_wired` proves a hook line exists in a settings file, or that a plugin is
enabled. That was already a rung better than "is it installed" (#177). It is still not the
fact that matters: a plane root was switched between branches four times and committed to,
**unguarded**, while that check would have reported a tick the whole time. The declaration
was real. Nothing dispatched it, because the work ran through a harness charter was not
wired into — and no amount of reading configuration can see that.

Reaching `hooks.pretooluse` is the proof configuration cannot give. So the handler that
holds the guard records that it ran, and under which harness.

**An age, never a verdict** — the grammar `silent 3d` and `▸steward 7m` already keep. A
plane worked in from a plain terminal has no dispatch and is fine; a plane whose guard last
fired weeks ago under a harness you have stopped using is the incident. Charter supplies the
date and the name; the reader draws the line (ADR 0013).
"""

from __future__ import annotations

import json
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

from charter import config, doctor, guardseen, hooks
from tests._isolation import PersonaIso, run_hook


class SeenCase(PersonaIso):
    def used_plane(self) -> None:
        """A plane a human has worked in — the gate on saying anything at all."""
        self.make_persona("ops", role="Ops", vault="none")

    def wrote(self, minutes: int, harness: str | None = "claude-code") -> None:
        guardseen.mark(harness=harness,
                       when=datetime.now(timezone.utc) - timedelta(minutes=minutes))


class TestTheGuardRecordsThatItRan(SeenCase):
    def test_nothing_is_recorded_before_a_guard_runs(self):
        self.assertIsNone(guardseen.last())

    def test_the_pretooluse_handler_records_it(self):
        """Not `sessionstart`: `check_guard_wired`'s own docstring rejects that reasoning —
        "a plane wiring only `sessionstart` is unprotected while looking configured, which
        is this issue again one level down"."""
        run_hook(hooks.pretooluse, {"tool_input": {"command": "ls"},
                                    "cwd": str(config.ROOT), "session_id": "s"})
        self.assertIsNotNone(guardseen.last())

    def test_it_records_which_harness(self):
        """The sentence that explains the incident. "Last seen under claude-code three
        weeks ago" says where the protection went; "not recently" never does."""
        self.wrote(0, harness="opencode")
        self.assertEqual(guardseen.last()["harness"], "opencode")

    def test_it_is_overwritten_not_appended(self):
        self.wrote(30)
        self.wrote(0)
        blob = json.loads(guardseen.path().read_text())
        self.assertIsInstance(blob, dict)

    def test_a_malformed_record_reads_as_none(self):
        """It feeds a preflight line, which must render whatever it finds."""
        guardseen.path().parent.mkdir(parents=True, exist_ok=True)
        guardseen.path().write_text("{ not json")
        self.assertIsNone(guardseen.last())

    def test_marking_never_raises(self):
        real = guardseen.path
        guardseen.path = lambda: Path("/proc/nope/guard-seen.json")
        self.addCleanup(setattr, guardseen, "path", real)
        self.assertIsNone(guardseen.mark())


class TestDoctorReportsAnAgeNotAVerdict(SeenCase):
    def test_a_recent_dispatch_is_ok_and_names_the_harness(self):
        self.used_plane()
        self.wrote(0, harness="claude-code")
        r = doctor.check_guard_seen()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("claude-code", r.detail)

    def test_an_old_dispatch_is_still_ok_but_carries_its_age(self):
        """Charter cannot know that an old dispatch is a problem — a plane may simply not
        have been opened in a harness lately. It reports the age and stops."""
        self.used_plane()
        self.wrote(60 * 24 * 21, harness="claude-code")
        r = doctor.check_guard_seen()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("21d", r.detail)

    def test_a_used_plane_with_no_dispatch_ever_warns(self):
        """The incident, detected."""
        self.used_plane()
        self.assertEqual(doctor.check_guard_seen().status, doctor.WARN)

    def test_a_fresh_plane_says_nothing(self):
        """Nothing could have dispatched in a plane created five minutes ago, and warning
        on day one is how a row stops being read."""
        self.assertEqual(doctor.check_guard_seen().status, doctor.OK)

    def test_the_used_gate_does_not_depend_on_hook_written_state(self):
        """Session state would be circular: it is written by the very hooks whose absence is
        being detected, so the gate would silence exactly the case it exists to report. A
        persona is something a human made."""
        self.assertFalse(guardseen.plane_has_been_used())
        self.used_plane()
        self.assertTrue(guardseen.plane_has_been_used())

    def test_a_clone_also_counts_as_used(self):
        ws = Path(config.WORKSPACES_DIR) / "task" / "svc" / ".git"
        ws.mkdir(parents=True)
        self.assertTrue(guardseen.plane_has_been_used())

    def test_it_runs_in_doctor(self):
        names = [r.name for r in doctor.run_all()]
        self.assertIn("guard seen", names)

    def test_it_is_a_sibling_of_the_wired_check_not_a_replacement(self):
        """Two facts, two rows — the split `check_plugin_skew` and `check_guard_wired`
        already keep, so neither can hide behind the other."""
        names = [r.name for r in doctor.run_all()]
        self.assertIn("plane-root guard", names)


if __name__ == "__main__":
    unittest.main()
