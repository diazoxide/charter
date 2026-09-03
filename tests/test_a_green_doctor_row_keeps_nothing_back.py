"""A green preflight row drops its hint, so the one sentence written to admit a blank
status line was never printed on a healthy machine (#856).

`Result.render` prints a hint only when the row is not green — which is right, because `→`
in that table means *do this* and a column of green arrows is how the yellow ones stop being
read. `check_frame` did not know that. Its clean path built
`_statusline_suppressed_note()` and handed it over as a `hint`, where it was discarded
without a trace:

    OK row render:   '  ✓  frame  tmux 3.5'
    WARN row render: '  !  frame  tmux 3.5\\n        → this hint should show'

Same `Result`, same hint. The green one loses it.

**The sentence it lost is the one ADR 0019 is about.** `_statusline_suppressed_note`'s own
docstring says why it exists: *"the failure this note exists to make impossible is an
operator seeing a blank footer and finding nothing anywhere that admits it is deliberate"*,
and ADR 0019's rule is that a surface which vanished for an invisible reason is the worst
outcome available. It reached the reader only on the path where tmux is below a version
ceiling — so on a current tmux, the ordinary machine and the one most likely to be running a
frame, the row was silent in exactly the situation that produces the blank footer.

Fixed in the `detail`, which a green row does print, on a `↳` continuation line — the shape
`check_harness` already uses for its ceilings. `↳` means *and also this*, and a status line
blanked because a frame is drawing is a capability working as designed, not a task. That is
the same distinction `check_frame` already draws in refusing to file it as a `ceilings`
entry; it is now carried through to how it is printed.
"""

from __future__ import annotations

import unittest
from unittest import mock

from charter import doctor
from tests import _envguard
from tests._isolation import PersonaIso

OK, WARN = doctor.OK, doctor.WARN

#: Hand-spelled. Asserting against `_statusline_suppressed_note()`'s own output would pass
#: just as happily over the empty string it returns outside a frame — which is the state
#: this whole module is about not being able to tell apart from a working one.
_NOTE = " This session's status line is intentionally blank: frame f1 is drawing the plane."


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


class TheFrameRowSaysItOnEveryPath(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame unless a test says otherwise, stated rather than inherited from
        # the shell the suite was launched from (#519, #521, #528).
        _envguard.unset_all()
        self.note = self.enterContext(
            mock.patch.object(doctor, "_statusline_suppressed_note", return_value=_NOTE))

    def _row(self, version=(3, 7), slots=("top", "bottom")):
        from charter import config
        with mock.patch("charter.frame.tmuxctl.version", return_value=version), \
             mock.patch.dict(config.FRAME, {"slots": list(slots)}):
            return doctor.check_frame()

    def test_a_healthy_machine_still_says_the_status_line_is_blank_on_purpose(self):
        """The regression. A current tmux with every slot implemented is the green path,
        and it is the machine most likely to have a frame drawing over its footer."""
        r = self._row()
        self.assertEqual(r.status, OK)
        self.assertIn("intentionally blank", r.render())

    def test_a_machine_below_a_ceiling_still_says_it_too(self):
        """It reached the reader here before, appended to the hint. Moving it must not
        trade one path for the other."""
        r = self._row(version=(3, 2))
        self.assertEqual(r.status, WARN)
        self.assertIn("intentionally blank", r.render())

    def test_the_ceiling_it_shares_the_row_with_is_still_reported(self):
        r = self._row(version=(3, 2))
        self.assertIn("charter frame-resize", r.render())

    def test_a_session_whose_footer_is_not_suppressed_says_nothing_about_it(self):
        """The other half of "never a warning": the note is a statement about right now,
        and a row that carried it always would be carrying noise."""
        self.note.return_value = ""
        r = self._row()
        self.assertEqual(r.status, OK)
        self.assertNotIn("intentionally blank", r.render())
        self.assertNotIn("↳", r.render())


class TestAGreenRowKeepsNothingBack(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # **The note is forced on, and without this the whole check is vacuous.** Outside a
        # live frame `_statusline_suppressed_note` returns `""`, so `check_frame` passed
        # `hint=None` and the sweep below found nothing to object to — measured against the
        # unfixed tree, where this class passed while the defect was fully present. A
        # property test that only holds in the environment where the property already holds
        # is the shape #851 was about, one layer up.
        self.enterContext(mock.patch.object(
            doctor, "_statusline_suppressed_note", return_value=_NOTE))

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
