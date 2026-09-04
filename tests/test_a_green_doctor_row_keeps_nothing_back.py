"""A green preflight row drops its hint, so a fact put in the wrong field is never printed
on a healthy machine (#856).

`Result.render` prints a hint only when the row is not green — which is right, because `→`
in that table means *do this* and a column of green arrows is how the yellow ones stop being
read. `check_frame` did not know that. Its clean path built a sentence and handed it over as
a `hint`, where it was discarded without a trace:

    OK row render:   '  ✓  frame  tmux 3.5'
    WARN row render: '  !  frame  tmux 3.5\\n        → this hint should show'

Same `Result`, same hint. The green one loses it.

**The sentence it lost was ADR 0019's** — *this session's status line is intentionally
blank, a frame is drawing the plane instead* — and it reached the reader only on the path
where tmux is below a version ceiling, so on a current tmux, the ordinary machine and the
one most likely to be running a frame, the row was silent in exactly the situation that
produces the blank footer. The fix was to say it in the `detail`, which a green row does
print, on a `↳` continuation line.

**#895 deleted that sentence.** Charter no longer wires a status line into Claude Code, so
there is no footer for a frame to blank and `_statusline_suppressed_note` is gone. What
survives is the rule the incident bought, which was never about the status line: *a fact a
passing row still needs to state goes in the detail, never in the hint*. This module holds
every check to it.

**It has to be able to fail.** The old fixture forced the note on for exactly that reason —
outside a live frame it returned `""`, `check_frame` passed no hint, and the sweep found
nothing to object to while the defect was fully present. With the note gone the sweep has no
real violator to find on any machine, which would make it a test that passes because there
is nothing to catch. `TheSweepCanActuallyFail` plants one, so the sweep's own predicate is
under test rather than assumed.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import doctor
from tests import _envguard
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN


class TheRenderContract(unittest.TestCase):
    def test_a_row_that_is_not_green_prints_its_hint(self):
        r = doctor.Result("frame", WARN, detail="tmux 3.5", hint="do the thing")
        self.assertIn("do the thing", r.render())

    def test_a_green_row_does_not(self):
        """Not a bug to be fixed by printing it — `→` is a remedy, and a passed check has
        none. The bug is a check that puts a fact where a remedy goes."""
        r = doctor.Result("frame", OK, detail="tmux 3.5", hint="do the thing")
        self.assertNotIn("do the thing", r.render())

    def test_a_green_row_prints_a_continuation_line_in_its_detail(self):
        """Which is where a fact a passing row still needs to state belongs."""
        r = doctor.Result("frame", OK, detail="tmux 3.5\n        ↳ and also this")
        self.assertIn("and also this", r.render())


class TheFrameRowStillReachesTheReader(unittest.TestCase):
    """What `check_frame` has left to say, on both paths.

    This class used to be `TheFrameRowSaysItOnEveryPath` and its four tests were all about
    the suppressed-footer note. Three of them died with the note in #895; the fourth — that
    the ceiling sharing the row is still reported — was never about the status line at all,
    and is the reason this class is retargeted rather than deleted. A row that lost its only
    remaining sentence would otherwise fail nothing.
    """

    def setUp(self) -> None:
        # Outside a frame unless a test says otherwise, stated rather than inherited from
        # the shell the suite was launched from (#519, #521, #528).
        _envguard.unset_all()

    def _row(self, version=(3, 7), slots=("top", "bottom")):
        from charter import config
        with mock.patch("charter.frame.tmuxctl.version", return_value=version), \
             mock.patch.dict(config.FRAME, {"slots": list(slots)}):
            return doctor.check_frame()

    def test_a_healthy_machine_states_the_version_it_measured(self):
        """The green path says what it found rather than only that it passed — the row is
        how an operator learns which tmux the frame is actually running on."""
        r = self._row()
        self.assertEqual(r.status, OK)
        self.assertIn("tmux 3.7", r.render())

    def test_a_healthy_machine_adds_no_continuation_line(self):
        """The other half of the #856 rule: a `↳` is for a fact that has to be stated, and
        a frame with nothing to report must not grow one out of habit. Until #895 this
        machine printed the suppressed-footer note here."""
        r = self._row()
        self.assertNotIn("↳", r.render())

    def test_the_ceiling_it_shares_the_row_with_is_still_reported(self):
        r = self._row(version=(3, 2))
        self.assertEqual(r.status, WARN)
        self.assertIn("charter frame-resize", r.render())


class TheSweepCanActuallyFail(unittest.TestCase):
    """The sweep below has no real violator to find, so this proves it would find one.

    Without this, `test_no_check_hides_a_sentence_in_a_hint_it_will_never_print` passes on
    a tree where the assertion had been deleted, the predicate inverted, or `run_all`
    stubbed to `[]` — which is the same shape as the defect the module was written for.
    """

    def test_a_planted_green_row_with_a_hint_is_caught(self):
        planted = [doctor.Result("planted", OK, detail="fine", hint="never printed")]
        with mock.patch.object(doctor, "run_all", return_value=planted):
            case = TestAGreenRowKeepsNothingBack(
                "test_no_check_hides_a_sentence_in_a_hint_it_will_never_print")
            result = case.run()
        self.assertEqual(len(result.failures), 1, "the sweep let a green row keep a hint")
        self.assertIn("never printed", result.failures[0][1])

    def test_the_real_check_set_is_not_empty(self):
        """`run_all` returning nothing would satisfy the sweep vacuously, and it is the
        one input the sweep does not choose for itself."""
        self.assertTrue(doctor.check_names())


class TestAGreenRowKeepsNothingBack(PersonaIso):
    def test_no_check_hides_a_sentence_in_a_hint_it_will_never_print(self):
        """Held against the real check set rather than against `check_frame` alone.

        The defect was not that `check_frame` was written carelessly — it was that
        `Result` accepts a hint on a green row and says nothing, so the mistake is
        invisible at the call site and invisible in the output. This is the assertion
        that makes it visible, on the commit that introduces it.
        """
        for r in doctor.run_all():
            if r.status == OK and r.hint:
                self.fail(
                    f"the {r.name!r} row passes a hint on a green row, and `Result.render` "
                    f"drops it — nobody will ever read {r.hint!r}. A fact a passing row "
                    f"needs to state goes in its detail, on a `↳` continuation line.")


if __name__ == "__main__":
    unittest.main()
