"""tmux is a frame prerequisite, not a harness ceiling.

Filing it under `harness.deficits` would claim claude-code cannot do something it does
perfectly well — `tests/test_doctor_absent_is_not_health.py` already draws that line, which
is why this is `doctor.check_frame() -> Result`, registered in `doctor._checks()` beside
`check_harness()` rather than a line inside its deficit list, and why every assertion below
is on the `Result` — its `.status`, checked against `doctor.OK`/`doctor.WARN` — rather than
on a string `doctor` never actually returns from a check function.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import doctor


class FrameRow(unittest.TestCase):
    def test_a_present_tmux_reports_its_version(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("3.7", r.detail)

    def test_an_absent_tmux_is_named_not_silent(self):
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            r = doctor.check_frame()
        self.assertIn("tmux", r.detail)
        # Not OK: "not checked" rendered as a tick is exactly the failure
        # `tests/test_doctor_absent_is_not_health.py` was filed against (#171) — and this
        # is not even "not checked", it's a confirmed absence, which is stronger evidence
        # than that class covers, so it must not be weaker than WARN either.
        self.assertNotEqual(r.status, doctor.OK)

    def test_a_below_floor_tmux_still_warns_not_fails(self):
        """`cmd_launch` itself does not refuse below `tmuxctl.FLOOR` — it warns and the
        frame still starts, with its hotkey menu disabled (`tmuxctl.below_floor_message`).
        A doctor row that FAILED here would tell the reader `charter <harness>` cannot run
        when it, in fact, still can."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.WARN)
        self.assertNotEqual(r.status, doctor.FAIL)

    def test_tmux_is_not_reported_as_a_harness_deficit(self):
        from charter import harness
        for h in harness.all():
            with self.subTest(harness=h.name):
                self.assertNotIn("tmux", " ".join(d.key for d in h.deficits))

    def test_cmd_doctor_never_fails_solely_on_a_missing_tmux(self):
        """The behavioural half of "WARN, never FAIL": `cmd_doctor`'s own exit code is
        1 only when a FAIL is among the results (`charter/commands.py:cmd_doctor`), so a
        machine missing tmux entirely must still see `doctor` register this as WARN."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            r = doctor.check_frame()
        self.assertNotEqual(r.status, doctor.FAIL)


if __name__ == "__main__":
    unittest.main()
