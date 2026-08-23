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

from charter import config, doctor
from charter.frame import slots


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
        """`cmd_launch` itself does not refuse below `tmuxctl.FLOOR` — the frame still
        starts, and nothing is switched off (`tmuxctl.below_floor_message`). A doctor row
        that FAILED here would tell the reader `charter <harness>` cannot run when it, in
        fact, still can."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.WARN)
        self.assertNotEqual(r.status, doctor.FAIL)

    def test_the_below_floor_hint_does_not_claim_the_hotkey_is_disabled(self):
        """This hint used to say "a frame still starts, with its hotkey menu disabled".
        Nothing disables it: `cmd_launch` warns and continues, and `conf_text` emits the
        bind unchanged. The hint now comes from `tmuxctl.below_floor_message` — one
        sentence shared with `--probe`, so the two cannot drift apart."""
        from charter.frame import tmuxctl
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)):
            r = doctor.check_frame()
        self.assertNotIn("hotkey menu disabled", r.hint)
        self.assertEqual(r.hint, tmuxctl.below_floor_message((3, 0)))

    def test_a_slot_with_no_renderer_is_a_ceiling_this_row_names(self):
        """The second standing condition that moved off the launch path (see
        `commands_frame.frame_ready`): `[frame] slots` accepts a slot charter sizes
        but has no renderer for, and nothing draws in it. It used to be a
        `util.warn` printed microseconds before tmux switched the terminal to the
        alternate screen.

        `left`/`right` shipped renderers in Task 3 (#385), so the registry — not a
        hardcoded pair — is patched to simulate the one still-standing case: a slot
        `config.FRAME["slots"]` accepts with nothing in `frame.slots.SLOTS` to draw
        it, the same gap `left`/`right` used to be until this task closed it."""
        from charter import config
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(config.FRAME, {"slots": ["top", "right"]}), \
             mock.patch.dict(slots.SLOTS):
            del slots.SLOTS["right"]
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn("right", r.hint)

    def test_an_ordinary_machine_still_renders_a_clean_row(self):
        """What stops the two tests above from passing against a row that always warns."""
        from charter import config
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)), \
             mock.patch.dict(config.FRAME, {"slots": ["top", "bottom"]}):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.OK)

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
