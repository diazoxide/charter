"""An `ask` charter cannot see the answer to is a nudge it can never justify.

`_ask` emitted 231 clone-commit prompts in 23 traced sessions and charter recorded not one
outcome — so "is this guard earning its interruptions?" had no evidentiary answer, and the
argument for keeping or deleting it could only be made from irritation. Same failure as the
`cmd=head` gap one level up: charter traced what it DECIDED and never what HAPPENED NEXT.

The signal is deterministic and already in the protocol: a hook `ask` blocks the tool, so a
`PostToolUse` for that same `tool_use_id` means it ran, which means it was approved. An ask
that is declined simply never gets one — its marker is still there, which is what makes
"asked N times, approved M times" countable.

**Why this file no longer drives the clone-commit nudge.** That nudge is gone (#371): it was
counted, and the count is what deleted it — 471 asks, all one rule, 97 of 98 approved. It
was also the only ask charter raised on the **Bash** tool, and `_ask_mark_take` was wired to
`posttooluse_bash` alone. Removing the producer therefore left the take half with no reachable
caller at all: the two surviving nudges raise on `Task|Agent` (`pretooluse_dispatch`) and on
`Write|Edit|MultiEdit` (`pretooluse_edit`), and *those* PostToolUse handlers never took the
marker. Every one of their approvals was already uncountable, and deleting the clone nudge
would have made "asked N, approved M" permanently `M = 0` — the exact defect #290 was filed
to remove, arriving by the back door.

So the take half now lives in one helper (`_ask_approved`) called from all three PostToolUse
handlers, and the cases below assert it on each tool family. The fixtures are real nudges on
real committed frontmatter, and each one asserts the ask actually fired before asserting
anything about the count — a fixture that stops reaching `_ask` must fail loudly rather than
report zero approvals of zero asks.

**The approval event is named for its nudge** (#375): `dispatch-ask-approved`, not a bare
`ask-approved`. Every assertion below therefore names the kind the fixture actually raises.
That matters more than a rename usually would — the negative cases here (`assertNotIn`,
`count`) would go vacuously green against the string `ask-approved`, which no code path can
produce any more, so a test asserting the old name would still pass while testing nothing.
"""

import unittest

from tests._isolation import run_hook
from tests.test_hooks import InAControlPlane
from charter import config, hooks, trace


class AskOutcomeCase(InAControlPlane):
    """The overlapping-dispatch nudge: `Task` in, `Task` out, same `tool_use_id`."""

    SID = "s"
    TUID = "tu_1"
    #: What this fixture's nudge is called, and so what its approval is called (#375).
    KIND = "dispatch-ask"
    APPROVED = "dispatch-ask-approved"

    def setUp(self) -> None:
        super().setUp()
        self.make_persona("coder", **{"dispatch-isolation": "worktree"})

    def ask(self, tuid: str | None = None) -> dict:
        """Raise one real nudge; returns the payload whose approval can now be recorded."""
        first = {"tool_name": "Task", "tool_input": {"subagent_type": "coder"},
                 "session_id": self.SID, "tool_use_id": "tu_0"}
        run_hook(hooks.pretooluse_dispatch, first)
        payload = {**first, "tool_use_id": tuid or self.TUID}
        r = run_hook(hooks.pretooluse_dispatch, payload)
        self.assertIsNotNone(r, "fixture never reached `_ask` — the count proves nothing")
        self.assertEqual("ask", r["hookSpecificOutput"]["permissionDecision"], r)
        return payload

    def approve(self, payload: dict) -> None:
        run_hook(hooks.posttooluse_dispatch, {**payload, "tool_response": ""})

    def events(self, sid=None):
        return [e["event"] for e in trace.read(sid or self.SID)]

    def markers(self):
        return list(config.SESSIONS_DIR.glob("*.ask-pending"))


class TestAnApprovedAskIsRecorded(AskOutcomeCase):
    def test_the_tool_running_marks_it_approved(self):
        self.approve(self.ask())
        self.assertIn(self.APPROVED, self.events())

    def test_the_ask_itself_is_still_traced(self):
        """Both halves, or the ratio has no denominator."""
        self.ask()
        self.assertIn(self.KIND, self.events())

    def test_the_two_halves_pair_by_name(self):
        """Why the approval is named for its nudge (#375): `charter trace --summary`
        aggregates event NAMES, so `dispatch-ask` beside `dispatch-ask-approved` is a
        ratio on the line that already exists. A shared counter could not be attributed
        to either guard, and a `reason` field would not reach the summary at all."""
        self.approve(self.ask())
        self.assertEqual([self.KIND, self.APPROVED],
                         [e for e in self.events() if "ask" in e])

    def test_the_marker_is_consumed(self):
        p = self.ask()
        self.assertEqual(1, len(self.markers()), "precondition: an ask was pending")
        self.approve(p)
        self.assertEqual([], self.markers())


class TestAnUnansweredAskIsNotRecordedAsApproved(AskOutcomeCase):
    def test_no_post_tool_use_means_no_approval(self):
        """A declined ask blocks the tool, so PostToolUse never fires for it."""
        self.ask()
        self.assertNotIn(self.APPROVED, self.events())

    def test_a_different_tool_call_does_not_resolve_it(self):
        p = self.ask()
        self.approve({**p, "tool_use_id": "tu_other"})
        self.assertNotIn(self.APPROVED, self.events())


class TestItIsCheapAndSafeOnTheHotPath(AskOutcomeCase):
    def test_a_post_tool_use_with_no_pending_ask_records_nothing(self):
        """The overwhelmingly common case: no marker, so the handler is a stat() and a
        return. It must never invent a row."""
        self.approve({"tool_name": "Task", "tool_input": {"subagent_type": "coder"},
                      "session_id": self.SID, "tool_use_id": "tu_never_asked"})
        self.assertNotIn(self.APPROVED, self.events())

    def test_it_resolves_only_once(self):
        """The unlink IS the idempotency — a replayed PostToolUse cannot inflate the count."""
        p = self.ask()
        self.approve(p)
        self.approve(p)
        self.assertEqual(1, self.events().count(self.APPROVED))


class TestEveryToolFamilyThatCanAskCanAlsoRecordTheApproval(InAControlPlane):
    """The gap #371 exposed: `_ask_mark_take` lived only in `posttooluse_bash`, so an ask
    raised on `Task|Agent` or on `Write|Edit|MultiEdit` could be approved and never counted.

    Asserted directly on the marker rather than through a nudge fixture, because the claim
    is about the HANDLER — that each PostToolUse family consumes a pending ask — and a
    per-family nudge fixture would make three different preconditions carry one assertion.

    Each row carries the KIND its pending marker was left under, and the handler is checked
    against `<kind>-approved` (#375). The kinds differ per row on purpose: a handler must
    take whatever nudge is pending on its tool_use_id, not one kind it is hardcoded for.
    `posttooluse_bash` gets `routing-ask` precisely because no nudge raises on **Bash**
    today — the handler still has to work the day one does, which is the property whose
    absence #371 exposed.
    """

    HANDLERS = (("posttooluse_bash", "routing-ask", {"tool_name": "Bash"}),
                ("posttooluse", "routing-ask",
                 {"tool_name": "Edit", "tool_input": {"file_path": "/x/y.py"}}),
                ("posttooluse_dispatch", "dispatch-ask",
                 {"tool_name": "Task", "tool_input": {"subagent_type": "coder"},
                  "tool_response": ""}))

    def test_each_one_consumes_a_pending_ask_and_records_it(self):
        for name, kind, payload in self.HANDLERS:
            with self.subTest(handler=name):
                sid, tuid = f"s-{name}", "tu_1"
                hooks._ask_mark_set(sid, tuid, kind)
                self.assertTrue(hooks._ask_mark(sid, tuid, kind).exists(),
                                "precondition: an ask is pending")
                run_hook(getattr(hooks, name), {**payload, "session_id": sid,
                                                "tool_use_id": tuid})
                self.assertFalse(hooks._ask_mark(sid, tuid, kind).exists(),
                                 f"{name} left the marker behind")
                self.assertIn(f"{kind}-approved", [e["event"] for e in trace.read(sid)],
                              f"{name} consumed the marker without recording the approval "
                              f"under the name of the nudge that earned it")


if __name__ == "__main__":
    unittest.main()
