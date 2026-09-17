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
from tests import _envguard


class FrameRow(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

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
        sentence shared with `--probe`, so the two cannot drift apart.

        `assertIn`, not `assertEqual`: tmux 3.0 is below `RESIZE_HOOK_FLOOR` as well as
        below `FLOOR`, so since #387 this row carries both sentences. That is the same
        shape the row already had for an unimplemented slot (`hint += " " + ...`), and
        the claim being pinned is unchanged — the below-floor sentence is `tmuxctl`'s
        own and is not re-worded here."""
        from charter.frame import tmuxctl
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 0)):
            r = doctor.check_frame()
        self.assertNotIn("hotkey menu disabled", r.hint)
        self.assertIn(tmuxctl.below_floor_message((3, 0)), r.hint)

    def test_a_tmux_without_the_resize_hook_is_a_ceiling_this_row_names(self):
        """#387's own finding: `tmuxctl.RESIZE_HOOK_FLOOR` (3.3) sits ABOVE
        `tmuxctl.FLOOR` (3.2), and appeared in neither this row nor `frame_ready`. So an
        operator on tmux 3.2 passed the floor cleanly, read a green tick here, and had no
        resize recovery at all — the panels stretch on every terminal resize and stay
        stretched. The launcher knew (it skipped the hook) and said so only into the
        pre-attach window, where the alternate screen hides it milliseconds later.

        3.2 exactly, not 3.0: at 3.0 the below-floor sentence is also present, so a row
        that named nothing about resizing would still have looked plausible. This version
        is the gap itself — nothing else has anything to say about it."""
        from charter.frame import tmuxctl
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 2)):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.WARN)
        self.assertIn(tmuxctl.below_resize_hook_message((3, 2)), r.hint)
        self.assertNotIn(tmuxctl.below_floor_message((3, 2)), r.hint,
                         "3.2 is AT the floor — it must not be reported as below it")

    def test_a_tmux_with_the_resize_hook_is_not_warned_about_it(self):
        """The other direction, and what stops the test above from passing against a row
        that warns unconditionally: at 3.3 (`RESIZE_HOOK_FLOOR` exactly) the hook exists,
        so there is no ceiling and the row is a clean tick."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 3)):
            r = doctor.check_frame()
        self.assertEqual(r.status, doctor.OK)
        self.assertIsNone(r.hint or None)

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


class EndedTabRow(unittest.TestCase):
    """`doctor.check_ended_tab`: does a harness exit reach charter on THIS tmux?

    **Its own row rather than a fourth ceiling on `check_frame`**, and the reason is the
    shape of the question. `check_frame` answers *can a frame run here* — above its floors
    it is a tick and says nothing more, which is right for a prerequisite. This one is a
    standing property an operator has to be able to read on a healthy machine too, because
    the answer they need is "yes, an exit is reported" as often as it is "no". A ceiling
    that only exists when it is breached cannot say the first.

    **Three answers, and never a fourth dressed up as one of them.** Above the floor, below
    it, and *could not tell* — no tmux on this machine, or a `tmux -V` charter could not
    parse, which `tmuxctl.version()` reports identically as `None`. The rule this class
    holds the row to is #1098's: **a row may not assert what it has not read.** "Could not
    tell" must not render as a tick (that asserts the tab is kept) and must not render as
    the ceiling (that asserts it is lost), and it must not name which of the two reasons it
    was, because charter did not find out.
    """

    def setUp(self) -> None:
        # Outside a frame, stated rather than inherited from the launching shell
        # (#519, #521, #528) — the same reason `FrameRow` says it.
        _envguard.unset_all()

    def test_above_the_floor_it_says_an_exit_is_reported(self):
        """Green, and it STATES the finding rather than only passing: the row is where an
        operator learns which tmux their frame is on and what that buys them."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            r = doctor.check_ended_tab()
        self.assertEqual(r.status, doctor.OK)
        self.assertIn("3.7", r.render())

    def test_at_the_floor_exactly_it_is_still_green(self):
        """3.5 is the release that fixed it, so 3.5 has it. A `<=` where `<` belongs warns
        the operator who already went and upgraded."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 5)):
            r = doctor.check_ended_tab()
        self.assertEqual(r.status, doctor.OK)

    def test_a_green_row_keeps_its_fact_in_the_detail_not_the_hint(self):
        """#856's rule, which `Result.render` enforces by dropping a green row's hint: a
        fact a passing row still needs to state goes in the detail. Asserted on this row
        directly as well as by the tree-wide sweep, so the mistake fails here first."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 7)):
            r = doctor.check_ended_tab()
        self.assertEqual(r.status, doctor.OK)
        self.assertFalse(r.hint, "a green row's hint is discarded unprinted")

    def test_below_the_floor_it_warns_and_names_the_tmux_the_floor_and_the_loss(self):
        """3.4 — the version Ubuntu LTS ships, and the whole reason this floor is not
        `FLOOR`. All three facts in one row: what is running, what is needed, what is lost.
        WARN and not FAIL for `check_frame`'s reason: charter still launches, still installs
        the ended step, and still keeps the tab every time the hook does fire."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 4)):
            r = doctor.check_ended_tab()
        self.assertEqual(r.status, doctor.WARN)
        rendered = r.render()
        self.assertIn("3.4", rendered)
        self.assertIn("3.5", rendered)
        self.assertIn("tab", rendered)

    def test_below_the_floor_the_hint_is_the_one_sentence_tmuxctl_owns(self):
        """One fact, one spelling. `frame_ready` reads the same function, so the probe and
        this row cannot drift into two different accounts of the same tmux bug — which is
        exactly what `below_resize_hook_message` was extracted to stop."""
        from charter.frame import tmuxctl
        with mock.patch("charter.frame.tmuxctl.version", return_value=(3, 4)):
            r = doctor.check_ended_tab()
        self.assertEqual(r.hint, tmuxctl.below_ended_tab_message((3, 4)))

    def test_an_unreadable_tmux_says_it_could_not_tell(self):
        """`tmuxctl.version()` answers `None` for a machine with no tmux AND for a `tmux -V`
        charter cannot parse. Neither is evidence about the hook, so neither may be reported
        as one: not a tick, and not the ceiling."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            r = doctor.check_ended_tab()
        self.assertNotEqual(r.status, doctor.OK, "unknown is not a tick")
        self.assertNotEqual(r.status, doctor.FAIL, "unknown is not a failure either")
        self.assertIn("could not", r.render())

    def test_an_unreadable_tmux_is_not_told_the_ceiling(self):
        """The half a "report everything when in doubt" row would get wrong. Charter has
        not read a version, so it must not hand over the below-floor sentence — that
        sentence opens by naming a tmux, and there is none to name."""
        from charter.frame import tmuxctl
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            r = doctor.check_ended_tab()
        self.assertNotIn(tmuxctl.below_ended_tab_message((3, 4)), r.render())

    def test_an_unreadable_tmux_does_not_guess_which_reason_it_was(self):
        """`check_frame` answers "tmux not found" for both, which is an assertion it has
        not earned: a tmux that is installed and answers something charter cannot parse is
        not an absent one. This row names the reading it failed to make, and leaves which
        of the two causes it was to the row beside it."""
        with mock.patch("charter.frame.tmuxctl.version", return_value=None):
            r = doctor.check_ended_tab()
        self.assertNotIn("not found", r.render())
        self.assertNotIn("not installed", r.render())

    def test_the_three_answers_are_actually_three(self):
        """What stops the cases above from passing against a row that always says one
        thing: the renders differ from each other, pairwise."""
        renders = []
        for v in ((3, 7), (3, 4), None):
            with mock.patch("charter.frame.tmuxctl.version", return_value=v):
                renders.append(doctor.check_ended_tab().render())
        self.assertEqual(len(set(renders)), 3, "three answers, three renders")

    def test_the_row_is_in_the_preflight_column(self):
        """`_FIXED_CHECK_NAMES` sizes doctor's name column without running a check and is
        pinned by equality against `run_all`; a row added to one and not the other is a
        wrong width at best. Named here too so the row cannot be quietly dropped from the
        run while its tests go on passing."""
        self.assertIn("ended tab", doctor._FIXED_CHECK_NAMES)


if __name__ == "__main__":
    unittest.main()
