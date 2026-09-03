"""An approval names the nudge that earned it (#375).

`ask-approved` carried a session id and nothing else. Charter raises two nudges today —
`routing-ask` (`pretooluse_edit`, on `Write|Edit|MultiEdit`) and `dispatch-ask`
(`pretooluse_dispatch`, on `Task|Agent`) — and every approval of either landed in that one
undifferentiated counter. So "asked N, approved M" was answerable in aggregate and not per
guard, which is the only form the question has ever been asked in: #371 deleted the
clone-commit nudge on the strength of *its own* 471/97, and a mixed counter cannot produce
that number for any of the nudges that are left.

**Why the name and not a field.** The ask half already names its guard — `routing-ask` and
`dispatch-ask` are separate events on purpose, so that a judgement about one is never made
from rows belonging to another. The approval half now follows the same convention:
`<kind>-approved`. That makes `charter trace --summary`'s existing `by event` line answer
the question with no change to the reader at all — `routing-ask=5, routing-ask-approved=3`
reads as a ratio — where a field would have needed the reader taught to group by it.

**Why the marker filename carries the kind.** `_ask_mark`'s contents are deliberately
empty, and there is no other channel between the two halves: `PostToolUse` knows a tool
family, never which guard asked. A kind is a fixed name chosen in code, not a value from
the payload — no prompt text, no command, nothing about the work — which is exactly what
makes it safe to put in a NAME that is already read. The approval half therefore stays one
code path (`_ask_approved`) rather than one per PostToolUse family, which is the property
#371 showed matters: when two code paths answered "was this nudge approved?", one of them
was wrong for months without anybody being able to see it.

The structural half below is the part that keeps this true. A nudge whose kind is not in
`_ASK_KINDS` writes a marker nothing ever takes, so its approvals would be silently
uncountable — the precise defect #290 was filed to remove, arriving by a new door.
"""

from __future__ import annotations

import ast
import itertools
import os
import unittest
from collections import Counter
from unittest import mock

from charter import config, hooks, trace
from tests import test_dispatch_ask_is_counted as _sites
from tests._isolation import PersonaIso, PlaneIso, run_hook
from tests.test_plugin import _hooks_json as _manifest

SESSION = "s-375"
WORK = "refactor the release tagging flow, maybe something cleaner"


def _decision(r):
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


class TwoNudgesInOnePlane(PlaneIso):
    """Both surviving nudges, live in ONE session — the condition #375 names as the one
    under which the aggregate counter is actually wrong. Each fixture asserts the nudge
    really fired before anything is counted: both fire only under a specific arrangement
    (a code-writing peer in flight; a roster shown and not followed), so a fixture that
    stops reaching `_ask` must fail loudly rather than report zero approvals of zero asks.
    """

    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "default"}))
        self.make_persona("steward", routing="require",
                          **{"delegate-when": "steward work", "vault": "none"})
        self.make_persona("coder", **{"dispatch-isolation": "worktree",
                                      "delegate-when": "code work", "vault": "none"})

    # -- the overlapping-dispatch nudge: Task in, Task out --------------------- #
    def _dispatch(self, tuid: str) -> tuple[dict, dict | None]:
        payload = {"tool_name": "Task", "tool_input": {"subagent_type": "coder"},
                   "session_id": SESSION, "tool_use_id": tuid}
        return payload, run_hook(hooks.pretooluse_dispatch, payload)

    def dispatch_nudge(self) -> dict:
        self._dispatch("tu_first")                      # puts `coder` in flight
        payload, r = self._dispatch("tu_overlap")
        self.assertEqual("ask", _decision(r),
                         "fixture never reached the dispatch nudge — the count proves nothing")
        return payload

    def approve_dispatch(self, payload: dict) -> None:
        run_hook(hooks.posttooluse_dispatch, {**payload, "tool_response": ""})

    # -- the routing nudge: roster shown, nothing dispatched, then an edit ------ #
    def routing_nudge(self) -> dict:
        payload = {"session_id": SESSION, "tool_name": "Edit", "tool_use_id": "tu_edit",
                   "tool_input": {"file_path": "/tmp/x.py"}}
        with mock.patch.dict(os.environ, {"CHARTER_PERSONA": "steward"}):
            run_hook(hooks.userpromptsubmit, {"session_id": SESSION, "prompt": WORK})
            r = run_hook(hooks.pretooluse_edit, payload)
        self.assertEqual("ask", _decision(r),
                         "fixture never reached the routing nudge — the count proves nothing")
        return payload

    def approve_edit(self, payload: dict) -> None:
        run_hook(hooks.posttooluse, payload)

    def asks(self) -> dict[str, int]:
        """Every ask-shaped row this session recorded, counted — the `by event` line of
        `charter trace --summary`, restricted to the family under test."""
        c = Counter(e["event"] for e in trace.read(SESSION))
        return {k: v for k, v in c.items() if "ask" in k}

    def approvals(self) -> list[str]:
        """Every approval row, found by SHAPE and read out of the WHOLE trace.

        Deliberately not `asks()`: that filters on ``"ask" in k``, which is right for the
        ratio line and wrong for "did charter invent an approval". An approval whose kind
        went wrong need not contain "ask" at all — `None-approved` is what `_ask_approved`
        writes if its ``if kind is None: return`` is dropped — so the filter that makes the
        ratio readable is also a filter that hides the failure. Named kinds are asserted
        against `asks()`; the existence of a row nobody can attribute is asserted here.
        """
        return sorted(e["event"] for e in trace.read(SESSION)
                      if e["event"].endswith("-approved"))


class TestAnApprovalNamesTheNudgeThatEarnedIt(TwoNudgesInOnePlane):
    def test_the_dispatch_nudge_s_approval_is_named_for_it(self):
        self.approve_dispatch(self.dispatch_nudge())
        self.assertEqual({"dispatch-ask": 1, "dispatch-ask-approved": 1}, self.asks())

    def test_the_routing_nudge_s_approval_is_named_for_it(self):
        self.approve_edit(self.routing_nudge())
        self.assertEqual({"routing-ask": 1, "routing-ask-approved": 1}, self.asks())

    def test_two_nudges_in_one_session_are_told_apart(self):
        """The whole issue: one approved, one not, and the record says WHICH.

        Against the old aggregate row this reads `ask-approved=1` beside `routing-ask=1`
        and `dispatch-ask=1` — a 50% approval rate that belongs to neither guard.
        """
        self.approve_edit(self.routing_nudge())   # approved: the tool ran
        self.dispatch_nudge()                     # declined: no PostToolUse ever arrives
        self.assertEqual({"routing-ask": 1, "routing-ask-approved": 1, "dispatch-ask": 1},
                         self.asks())

    def test_every_approval_row_written_names_a_nudge(self):
        """Stated separately from the counts above, because the failure it guards is a
        SECOND row rather than a missing one: an approval recorded under both names would
        satisfy every assertion here except this one, and would double every tally.

        Asserted as the whole list of `-approved` rows rather than as
        ``asks().get("ask-approved", 0) == 0``. That literal names one string of the many
        that cannot be attributed to a guard, and it is a string no code path can produce
        any more — so it holds only against a mutation that reintroduces that exact spelling
        and lets every other unattributable row through, `None-approved` included.
        """
        self.approve_dispatch(self.dispatch_nudge())
        self.assertEqual(["dispatch-ask-approved"], self.approvals(),
                         "an approval row was written that no guard can be credited with "
                         "or debited for (#375)")


class TestTheMarkerIsWhatCarriesTheKind(TwoNudgesInOnePlane):
    """The mechanism, asserted on its own so a failure is diagnosed where it happened."""

    def test_the_pending_marker_names_the_nudge(self):
        self.dispatch_nudge()
        names = [p.name for p in config.SESSIONS_DIR.glob("*.ask-pending")]
        self.assertEqual(1, len(names), f"expected one pending ask, got {names}")
        self.assertIn("dispatch-ask", names[0])

    def test_taking_it_reports_which_nudge_it_was(self):
        hooks._ask_mark_set("s-mark", "tu_1", "routing-ask")
        self.assertEqual("routing-ask", hooks._ask_mark_take("s-mark", "tu_1"))
        self.assertIsNone(hooks._ask_mark_take("s-mark", "tu_1"),
                          "the unlink IS the idempotency — a replay must report nothing")

    def test_a_marker_of_one_kind_is_not_taken_as_another(self):
        hooks._ask_mark_set("s-mark", "tu_1", "routing-ask")
        self.assertTrue(hooks._ask_mark("s-mark", "tu_1", "routing-ask").exists())
        self.assertFalse(hooks._ask_mark("s-mark", "tu_1", "dispatch-ask").exists())


class TestANewNudgeCannotBecomeUncountable(unittest.TestCase):
    """`_ASK_KINDS` is what `_ask_mark_take` looks for, and it is the one place a third
    nudge can be forgotten. A kind that is not in it produces a marker nothing ever takes:
    the ask is counted, the approval never is, and the ratio reads as "nobody ever approves
    this" — which is the conclusion #371 acted on, arrived at falsely.

    Reuses the AST scan `test_dispatch_ask_is_counted` already holds rather than growing a
    second one, so both structural rules see exactly the same set of call sites.
    """

    SITES = _sites.TestEveryAskSiteRecordsAnAsk

    @classmethod
    def _sites_that_ask(cls) -> dict[str, ast.FunctionDef]:
        found = cls.SITES._functions_that_ask()
        assert found, "precondition: the scan found no `_ask` call site at all"
        return found

    @staticmethod
    def _kinds_passed(fn: ast.FunctionDef) -> list[str | None]:
        """The `kind` every `_ask(...)` call in *fn* passes — ``None`` for a non-literal,
        which is itself a failure: a kind computed at runtime cannot be checked here."""
        out: list[str | None] = []
        for call in ast.walk(fn):
            if not (isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                    and call.func.id == "_ask"):
                continue
            kw = {k.arg: k.value for k in call.keywords}
            node = kw.get("kind") or (call.args[2] if len(call.args) > 2 else None)
            out.append(node.value if isinstance(node, ast.Constant) else None)
        return out

    def test_every_ask_site_passes_a_registered_kind(self):
        for name, fn in sorted(self._sites_that_ask().items()):
            with self.subTest(handler=name):
                kinds = self._kinds_passed(fn)
                self.assertTrue(kinds, f"{name} calls _ask without a kind")
                for kind in kinds:
                    self.assertIn(kind, hooks._ASK_KINDS,
                                  f"{name} raises a nudge `_ask_mark_take` never looks "
                                  f"for, so its approvals are uncountable")

    def test_the_kind_is_the_event_the_site_records(self):
        """`<kind>` and `<kind>-approved` have to pair by NAME in the summary — which they
        only do if the kind is the same string the site traces for the ask itself."""
        for name, fn in sorted(self._sites_that_ask().items()):
            with self.subTest(handler=name):
                self.assertEqual(set(self._kinds_passed(fn)),
                                 set(self.SITES._ask_shaped_events(fn)),
                                 f"{name}'s ask row and its approval row would not pair")

    def test_the_registry_holds_nothing_no_site_raises(self):
        """A stale kind costs a `stat()` on every tool call for a nudge that cannot fire,
        and reads as a nudge that exists."""
        used = {k for fn in self._sites_that_ask().values() for k in self._kinds_passed(fn)}
        self.assertEqual(set(hooks._ASK_KINDS), used)


class TestOneToolUseIdCarriesAtMostOnePendingAsk(unittest.TestCase):
    """`_ask_mark_take` answers with ONE kind, so two markers on one `tool_use_id` would
    resolve to whichever kind stands first in `_ASK_KINDS` and leave the other to age out
    — counted as an ask and never as an approval, which is precisely the shape #371 read a
    guard's deletion off.

    That cannot happen today, and the reason is not in `hooks.py`: the two nudges are raised
    from PreToolUse handlers whose SHIPPED matchers name disjoint tool families, so no
    single tool call reaches both. This asserts that, rather than the docstring asserting it
    — the assumption lives in `hooks/hooks.json`, which nothing else here reads, and a third
    nudge added on `Write|Edit|MultiEdit` would satisfy every other test in this file while
    silently making one of the two counts wrong.

    Reads the manifest through `test_plugin`'s reader rather than a second one, so both
    files see the same shipped file.
    """

    @staticmethod
    def _matcher_of(handler: str) -> str:
        """The tool matcher `hooks.json` ships for ``charter hook <handler>``."""
        found = [entry.get("matcher") for entries in _manifest().values()
                 for entry in entries for hook in entry["hooks"]
                 if f"charter hook {handler} " in hook["command"] + " "]
        assert len(found) == 1, f"{handler}: expected one manifest entry, got {found}"
        return found[0] or ""

    @classmethod
    def _families(cls) -> dict[str, set[str]]:
        """Tool name -> the nudge-raising handlers registered for it."""
        names = {n for n in TestANewNudgeCannotBecomeUncountable._sites_that_ask()}
        # `_HANDLERS` maps the manifest's hyphenated name onto the function.
        by_fn = {fn.__name__: name for name, fn in hooks._HANDLERS.items()}
        return {n: set(cls._matcher_of(by_fn[n]).split("|")) for n in sorted(names)}

    def test_the_scan_found_the_real_handlers_in_the_shipped_manifest(self):
        """Precondition: a matcher lookup that silently found nothing would make the
        disjointness below vacuously true."""
        fams = self._families()
        self.assertEqual({"pretooluse_dispatch", "pretooluse_edit"}, set(fams))
        for name, tools in fams.items():
            self.assertTrue(tools and all(tools), f"{name} has no shipped matcher: {tools}")

    def test_no_two_nudges_can_be_raised_on_the_same_tool(self):
        fams = self._families()
        for a, b in itertools.combinations(sorted(fams), 2):
            with self.subTest(pair=(a, b)):
                self.assertEqual(set(), fams[a] & fams[b],
                                 f"{a} and {b} can both raise on the same tool call, so one "
                                 f"tool_use_id can carry two pending asks — `_ask_mark_take` "
                                 f"answers with one kind and the other is never counted")


if __name__ == "__main__":
    unittest.main()
