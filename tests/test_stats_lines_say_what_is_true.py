"""Two lines in `persona stats` asserted things the data did not support.

**"No dispatches recorded yet"** was printed from the `else` of the SKILLS-DRIFT check, so a
plane whose personas all declared their skills correctly was told its dispatch tally was
empty — beside a table showing five dispatches. A mis-attached `else`, invisible for as long
as every plane had some drift, and surfaced the moment one stopped.

**"fired N · M dispatch(es) followed"** paired the advice count against the LIFETIME
dispatch total. On the plane that shipped the feature, three of those dispatches predated it
by four days, and the line read as though the roster had caused them. That is a causal claim
from a coincidence of counters — the thing ADR 0016 exists to forbid, in the line whose only
job is to measure whether ADR 0016's mechanism works.

Both are the same failure: a number stated with more confidence than its provenance earns.
"""
from __future__ import annotations

import io
import unittest
from contextlib import redirect_stderr, redirect_stdout
from datetime import datetime, timedelta, timezone
from types import SimpleNamespace

from charter import commands_persona, dispatch
from tests._isolation import PersonaIso


def _ago(**kw) -> datetime:
    return datetime.now(timezone.utc) - timedelta(**kw)


class StatsCase(PersonaIso):
    def persona(self, name: str, **meta) -> None:
        self.make_persona(name, role=name.title(), vault="none",
                          **{"delegate-when": f"{name} work", **meta})

    def report(self) -> str:
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            commands_persona.cmd_persona_stats(SimpleNamespace(shared=False, days=None))
        return (out.getvalue() + err.getvalue()).lower()


class TestTheEmptyTallyLine(StatsCase):
    def test_it_is_not_printed_when_dispatches_exist(self):
        """The bug: it was the `else` of the skills-drift check, so a clean roster was told
        its tally was empty."""
        self.persona("forge")
        dispatch.record("forge")
        self.assertNotIn("no dispatches recorded yet", self.report())

    def test_it_is_printed_when_the_tally_really_is_empty(self):
        self.persona("forge")
        self.assertIn("no dispatches recorded yet", self.report())

    def test_skills_drift_does_not_decide_it_either_way(self):
        """Drift and an empty tally are unrelated facts; one must never speak for the other.
        A persona declaring a skill it never invokes has drift AND dispatches."""
        self.persona("forge", skills="charter:working-in-a-clone")
        dispatch.record("forge")
        self.assertNotIn("no dispatches recorded yet", self.report())


class TestTheAdviceLineComparesLikeWithLike(StatsCase):
    def test_dispatches_before_the_first_advice_are_not_counted_as_following_it(self):
        """The line's whole purpose is fired-vs-followed. A dispatch that happened before
        the roster ever appeared cannot have followed it."""
        dispatch.record("forge", when=_ago(days=4))
        dispatch.record_advice(when=_ago(hours=1))
        self.assertEqual(dispatch.dispatches_since_first_advice(), 0)

    def test_a_dispatch_after_the_first_advice_counts(self):
        dispatch.record_advice(when=_ago(hours=2))
        dispatch.record("forge", when=_ago(hours=1))
        self.assertEqual(dispatch.dispatches_since_first_advice(), 1)

    def test_no_advice_means_nothing_to_compare(self):
        dispatch.record("forge")
        self.assertEqual(dispatch.dispatches_since_first_advice(), 0)

    def test_the_report_uses_that_number_not_the_lifetime_total(self):
        self.persona("forge")
        dispatch.record("forge", when=_ago(days=4))     # long before the feature existed
        dispatch.record_advice(when=_ago(hours=1))
        text = self.report()
        self.assertIn("routing advice", text)
        # The old line said "1 time(s) · 1 dispatch(es) followed" from the lifetime total.
        self.assertNotIn("1 dispatch(es) followed", text)

    def test_it_says_since_when(self):
        """A bare pair of numbers invites the reader to supply the window themselves, and
        the window is the whole reason the pair means anything."""
        self.persona("forge")
        dispatch.record_advice(when=_ago(hours=1))
        self.assertIn("since", self.report())


if __name__ == "__main__":
    unittest.main()
