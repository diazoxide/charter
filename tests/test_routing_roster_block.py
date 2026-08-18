"""The roster block: who else exists, what each claims, when each was last dispatched.

charter never names the owner of a prompt (ADR 0016). On a work-shaped prompt, when the
acting persona declares `routing: advise` or `require`, it injects the facts it owns and
lets the reader route. That is the whole mechanism — no matcher, no `triggers:` field, no
path ownership.

It rides the EXISTING commitment-point gate rather than adding a second hook message: two
blocks on one prompt is how wallpaper gets manufactured, and that gate's own comment
records what happens to a nudge people learn to skim.
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, dispatch, hooks, persona
from tests._isolation import PersonaIso, run_hook

#: Trips the commitment classifier: an action verb plus a real fork (open-ended wording).
WORK = "refactor the statusline column widths, maybe something cleaner"
#: Pure information-seeking — the gate stays silent, so the roster must too.
LOOKUP = "what does the statusline column width do"


def _context(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")


class RosterCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        # The workspace confirm nudge is a different signal with its own several hundred
        # words; pin the workspace so assertions here read only what this block adds.
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "default"}))

    def p(self, name, **meta):
        return self.make_persona(name, role=name.title(), vault="none",
                                 **{"delegate-when": f"{name} work", **meta})

    def prompt(self, text=WORK, sid="s1") -> str:
        return _context(run_hook(hooks.userpromptsubmit,
                                 {"session_id": sid, "prompt": text}))

    def acting(self, name):
        """Make *name* the acting persona for the assertions that follow."""
        return mock.patch.dict(os.environ, {"CHARTER_PERSONA": name})


class TestWhenItFires(RosterCase):
    def test_advise_puts_the_roster_on_a_work_shaped_prompt(self):
        self.p("steward", routing="advise")
        self.p("forge", **{"delegate-when": "GitHub APIs, CI state"})
        with self.acting("steward"):
            out = self.prompt()
        self.assertIn("forge", out)
        self.assertIn("GitHub APIs, CI state", out)

    def test_require_fires_the_same_block(self):
        """At prompt time the two levels say the same thing; `require` differs later, at
        tool time, which is a separate increment."""
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.assertIn("forge", self.prompt())

    def test_off_says_nothing(self):
        self.p("steward", routing="off")
        self.p("forge")
        with self.acting("steward"):
            self.assertNotIn("forge", self.prompt())

    def test_an_undeclared_level_says_nothing(self):
        """Absent is `off`: upgrading a plane that declared nothing changes nothing."""
        self.p("steward")
        self.p("forge")
        with self.acting("steward"):
            self.assertNotIn("forge", self.prompt())

    def test_a_lookup_prompt_says_nothing(self):
        """It shares the gate's trigger. A question is not a commitment point, and a block
        that fires on one teaches the reader to skim the block that matters."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            self.assertNotIn("forge", self.prompt(LOOKUP))

    def test_an_empty_roster_says_nothing(self):
        """A plane whose only persona is the one acting has nobody to route to, and a
        block saying so every time would be the purest wallpaper this design could ship."""
        self.p("steward", routing="advise")
        with self.acting("steward"):
            out = self.prompt()
        self.assertNotIn("routing", out.lower().split("commitment point")[0])

    def test_no_acting_persona_means_no_level_and_no_block(self):
        """No declared front door, no active persona: the plane has opted out of personas
        and charter says nothing. `doctor` reports the inertness; a prompt hook does not."""
        self.p("forge")
        with mock.patch.dict(os.environ, {k: v for k, v in os.environ.items()
                                          if k != "CHARTER_PERSONA"}, clear=True):
            self.assertNotIn("forge", self.prompt())


class TestWhatItSays(RosterCase):
    def test_it_lists_every_persona_rather_than_choosing_one(self):
        """ADR 0016: charter presents the roster and never guesses the owner. Naming a
        winner is the one change that would break this block."""
        self.p("steward", routing="advise")
        self.p("forge")
        self.p("release")
        with self.acting("steward"):
            out = self.prompt()
        self.assertIn("forge", out)
        self.assertIn("release", out)

    def test_it_reports_a_persona_that_has_never_been_dispatched(self):
        """The date is the evidence. 'Never dispatched' is the finding this whole design
        exists to act on."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            out = self.prompt()
        self.assertIn("never", out.lower())

    def test_it_reports_the_date_of_a_real_dispatch(self):
        self.p("steward", routing="advise")
        self.p("forge")
        dispatch.record("forge")
        with self.acting("steward"):
            out = self.prompt().lower()
        self.assertIn("last dispatched", out)
        self.assertNotIn("never dispatched", out)

    def test_the_acting_persona_is_not_offered_as_a_destination(self):
        """It is named — the block says whose posture is speaking, and asks for one line
        if the work stays put. What it must never do is list the acting persona as
        somewhere to route TO, which is noise in a block that cannot afford any."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            out = self.prompt()
        rows = [ln for ln in out.splitlines() if ln.strip().startswith("•")]
        self.assertTrue(rows)
        self.assertFalse([ln for ln in rows if "`steward`" in ln])


class TestItIsMeasured(RosterCase):
    def test_firing_is_tallied(self):
        """Everything here is a bet that visibility changes routing. Shipping without the
        number that could falsify it repeats the gap `dispatch.py` was written about."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
        self.assertEqual(dispatch.advice_tally(), 1)

    def test_not_firing_is_not_tallied(self):
        self.p("steward", routing="off")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
        self.assertEqual(dispatch.advice_tally(), 0)

    def test_the_tally_records_no_prompt_text(self):
        """The dispatch store is counts and dates only — there is no secret surface to
        scan, and this event must not become the first one."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            self.prompt("refactor the statusline, maybe with SECRETVALUE in it")
        blob = "".join(f.read_text() for f in (config.PERSONAS_DIR / "_dispatch").glob("*.jsonl"))
        self.assertNotIn("SECRETVALUE", blob)


if __name__ == "__main__":
    unittest.main()


class TestItIsReported(RosterCase):
    """The fired-vs-followed pair has to reach a human surface, or measuring it is the
    same write-only gesture the todo store's docstring warns about."""

    def _stats(self) -> str:
        import io
        from contextlib import redirect_stderr, redirect_stdout
        from types import SimpleNamespace
        from charter import commands_persona
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands_persona.cmd_persona_stats(SimpleNamespace(shared=False, days=None))
        return out.getvalue() + err.getvalue()

    def test_stats_reports_advice_shown_against_dispatches(self):
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
        dispatch.record("forge")
        out = self._stats().lower()
        self.assertIn("routing advice", out)
        self.assertIn("fired 1", out)

    def test_stats_says_nothing_about_advice_that_never_fired(self):
        """A line reading 'fired 0 · dispatched 0' on every plane that has not opted in is
        a row people learn to skip, and it takes the rest of the report's credibility."""
        self.p("steward")
        self.p("forge")
        self.assertNotIn("routing advice", self._stats().lower())


class TestDoctorSaysWhenRoutingIsInert(RosterCase):
    def test_personas_but_no_front_door_is_named(self):
        """Settled during the grill: no declared default and no active persona means the
        roster block can never fire. That is a legitimate choice, so it is said once in
        `doctor` — not on every prompt."""
        from charter import doctor
        self.p("forge")
        self.p("release")
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        r = doctor.check_front_door()
        self.assertIn("inert", (r.detail + " " + (r.hint or "")).lower())

    def test_a_plane_with_no_personas_at_all_is_silent(self):
        """Nothing to route to and nothing declared — an empty plane is not a problem."""
        from charter import doctor
        (config.ROOT / "charter.toml").write_text("schema = 1\n")
        r = doctor.check_front_door()
        self.assertNotIn("inert", (r.detail + " " + (r.hint or "")).lower())
