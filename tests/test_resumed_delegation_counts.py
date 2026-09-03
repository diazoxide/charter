"""Handing more work to a running persona is delegation the tally could not see.

`posttooluse_dispatch` fires on `Task`/`Agent`, so it records the moment a sub-agent is
CREATED. Continuing that agent — `SendMessage` to it — is work handed to a persona and
recorded nowhere. On the day the roster block shipped, `release` cut two releases and ran a
plugin sweep; the sweep was a resume, so the tally read 2, and the fired-vs-followed pair
undercounted the thing it exists to measure.

Two decisions shape this, and both are about not overstating:

* **A resume is not a new dispatch.** `DISP` answers "how many times was this persona
  dispatched as a sub-agent"; a resume is more work sent to one already running. Counting
  it there would inflate an answer to a question nobody asked. It gets its own event kind.
* **Attribution comes from observation, never inference.** A resume addresses an opaque
  agent id, not a persona name — every resume in the motivating session did. charter learns
  the id→persona mapping from the dispatch it already watched create that id, and when it
  has no mapping it records nothing rather than guessing.
"""
from __future__ import annotations

import unittest
from datetime import datetime, timedelta, timezone

from charter import dispatch, hooks
from tests._isolation import PersonaIso, PlaneIso, run_hook

AGENT_ID = "a1b2c3d4e5f6a7b8"


def _ago(**kw) -> datetime:
    return datetime.now(timezone.utc) - timedelta(**kw)


class ResumeCase(PlaneIso):
    def dispatched(self, persona: str, agent_id: str = AGENT_ID):
        """The harness's own Task result carries the id it just created."""
        return run_hook(hooks.posttooluse_dispatch, {
            "session_id": "s1", "tool_name": "Task",
            "tool_input": {"subagent_type": persona},
            "tool_response": f"Async agent launched successfully.\nagentId: {agent_id} "
                             f"(internal ID - do not mention to user.)"})

    def resumed(self, to: str):
        return run_hook(hooks.posttooluse_message, {
            "session_id": "s1", "tool_name": "SendMessage",
            "tool_input": {"to": to, "message": "carry on"}})


class TestAResumeIsRecorded(ResumeCase):
    def test_a_resume_by_agent_id_is_attributed_to_the_persona(self):
        """The motivating case: every resume in the real session addressed an opaque id."""
        self.dispatched("release")
        self.resumed(AGENT_ID)
        self.assertEqual(dispatch.resume_tally(), 1)

    def test_a_resume_addressed_by_persona_name_is_recorded_too(self):
        self.make_persona("release", role="R", vault="none")
        self.resumed("release")
        self.assertEqual(dispatch.resume_tally(), 1)

    def test_an_unknown_target_records_nothing(self):
        """No mapping, no persona of that name — charter does not guess who was addressed."""
        self.resumed("some-other-session")
        self.assertEqual(dispatch.resume_tally(), 0)

    def test_a_message_to_main_records_nothing(self):
        self.resumed("main")
        self.assertEqual(dispatch.resume_tally(), 0)


class TestAResumeIsNotANewDispatch(ResumeCase):
    def test_disp_counts_only_real_dispatches(self):
        """`DISP` answers "times dispatched as a sub-agent". Inflating it with resumes
        answers a question nobody asked, in the column people retire personas on."""
        self.dispatched("release")
        self.resumed(AGENT_ID)
        self.resumed(AGENT_ID)
        self.assertEqual(dispatch.tally()["release"], 1)

    def test_but_the_persona_counts_as_recently_used(self):
        """`last_seen` feeds the roster block's "last dispatched" line, and a persona that
        was working an hour ago has not been idle since its first dispatch."""
        self.dispatched("release")
        self.resumed(AGENT_ID)
        self.assertIsNotNone(dispatch.last_seen("release"))


class TestTheFiredVsFollowedPairCountsIt(ResumeCase):
    def test_a_resume_after_advice_counts_as_a_handoff(self):
        """The pair measures whether the roster moves work to a persona. It does not care
        whether that persona was already running."""
        dispatch.record_advice(when=_ago(hours=2))
        self.dispatched("release")
        self.resumed(AGENT_ID)
        self.assertEqual(dispatch.handoffs_since_first_advice(), 2)

    def test_handoffs_before_the_advice_still_do_not_count(self):
        """Written with explicit times rather than back-to-back calls: the store stamps to
        SECOND resolution, so three events in one second cannot be ordered by it. That is a
        real boundary, not a test artifact — it can only ever over-count handoffs sharing a
        second with the first advice, which is why the docstring names it."""
        dispatch.record("release", when=_ago(hours=3))
        dispatch.record_resume("release", when=_ago(hours=2))
        dispatch.record_advice(when=_ago(hours=1))
        self.assertEqual(dispatch.handoffs_since_first_advice(), 0)


class TestItNeverBreaksTheTurn(ResumeCase):
    def test_a_malformed_payload_is_survivable(self):
        self.assertEqual(run_hook(hooks.posttooluse_message, {"tool_name": "SendMessage"}), None)
        self.assertEqual(dispatch.resume_tally(), 0)

    def test_a_dispatch_result_without_an_id_still_records_the_dispatch(self):
        """The mapping is a bonus; the dispatch tally must not depend on it."""
        run_hook(hooks.posttooluse_dispatch, {
            "session_id": "s1", "tool_name": "Task",
            "tool_input": {"subagent_type": "release"}, "tool_response": "no id here"})
        self.assertEqual(dispatch.tally()["release"], 1)


if __name__ == "__main__":
    unittest.main()
