"""Every nudge charter emits is countable, including the dispatch one (#367).

`_ask` leaves an `ask-pending` marker so the outcome can be tallied: `_ask_approved` clears
it when the tool actually runs, and that clearing is recorded as `<kind>-approved` — named
for the nudge that earned it since #375, so `dispatch-ask` pairs with
`dispatch-ask-approved` rather than both guards sharing one counter. `_ask` deliberately
does not trace the ask itself — "callers trace their own event name, so they must not also
record an 'ask' that never reached anybody".

Three sites called it. `pretooluse` traced `ask`, `pretooluse_edit` traces `routing-ask`,
and `pretooluse_dispatch` traced nothing while still passing `data` — so its approvals were
recorded against a denominator that never counted the ask. That is the exact shape #290 was
filed to remove, applied to two sites of three. Two sites remain: `pretooluse`'s
clone-commit nudge was deleted in #371, and `_functions_that_ask` below is what will notice
if a third is ever added without an ask-shaped row to count it.

**This cannot be observed on a plane that does not use worktree isolation**, which is every
plane the audit had: the nudge fires only when a peer declaring `dispatch-isolation:
worktree` is already in flight, so a test could pass by never reaching the handler at all.
Every counting assertion below is therefore paired with a precondition assertion that the
nudge actually fired — the emitted decision — so a fixture that stops reaching the `_ask`
fails loudly instead of passing vacuously.

The event is `dispatch-ask`, not `ask`. `routing-ask` set that precedent, and folding this
into `ask` would have corrupted a series whose every historical row means the clone-commit
guard — the evidence the judgement about that guard rested on, and did: 471 rows, all one
rule, 97 of 98 approved, and the guard was deleted (#371). Keeping the series clean is what
made that answerable, so the separation stays.
"""

from __future__ import annotations

import ast
import unittest
from pathlib import Path

from charter import config, hooks, inflight, trace
from tests._isolation import PersonaIso, PlaneIso, run_hook

SESSION = "s-dispatch"


class DispatchAskCase(PlaneIso):
    """A plane that DOES use worktree isolation — the fixture this defect needs."""

    def setUp(self) -> None:
        super().setUp()
        self.make_persona("coder", **{"dispatch-isolation": "worktree"})
        self.make_persona("explorer")

    def dispatch(self, agent: str = "coder", **extra):
        """One Task dispatch through the handler; returns the emitted JSON or None."""
        payload = {"tool_name": "Task", "tool_input": {"subagent_type": agent},
                   "session_id": SESSION, "tool_use_id": f"toolu_{agent}", **extra}
        return run_hook(hooks.pretooluse_dispatch, payload)

    def events(self, name: str) -> list[dict]:
        return [e for e in trace.read(SESSION) if e.get("event") == name]

    def assert_nudged(self, out) -> None:
        """The precondition every count below depends on: the handler REACHED the ask."""
        self.assertIsNotNone(out, "fixture never reached the nudge — the count proves nothing")
        decision = out["hookSpecificOutput"]["permissionDecision"]
        self.assertEqual(decision, "ask", "must nudge, never deny")


class TestTheFixtureItself(DispatchAskCase):
    """Asserted separately so a broken fixture is diagnosed as a broken fixture."""

    def test_a_first_dispatch_does_not_nudge(self):
        self.assertIsNone(self.dispatch())

    def test_an_overlapping_code_writer_does_nudge(self):
        self.dispatch()
        self.assert_nudged(self.dispatch())

    def test_the_peer_really_is_recorded_in_flight(self):
        self.dispatch()
        self.assertEqual(inflight.live(), ["coder"])


class TestTheAskIsCounted(DispatchAskCase):
    def test_an_overlapping_dispatch_records_a_dispatch_ask(self):
        self.dispatch()
        self.assert_nudged(self.dispatch())
        self.assertEqual(len(self.events("dispatch-ask")), 1)

    def test_the_ask_is_recorded_alongside_the_marker_that_counts_its_approval(self):
        """The two halves of one tally. The marker was already being left; the row that
        gives it a denominator was not."""
        self.dispatch()
        self.assert_nudged(self.dispatch())
        markers = list(config.SESSIONS_DIR.glob("*.ask-pending"))
        self.assertEqual(len(markers), 1, "the approval half was already being recorded")
        self.assertEqual(len(self.events("dispatch-ask")), 1, "so the ask half must be too")

    def test_it_does_not_pollute_the_bare_ask_series(self):
        """Historical `ask` rows all mean the clone-commit guard, which no longer exists.
        This one says what it is, so a store holding both stays readable."""
        self.dispatch()
        self.assert_nudged(self.dispatch())
        self.assertEqual(self.events("ask"), [])

    def test_the_row_names_the_dispatched_agent(self):
        self.dispatch()
        self.assert_nudged(self.dispatch())
        self.assertEqual(self.events("dispatch-ask")[0].get("agent"), "coder")


class TestWhatIsNotCounted(DispatchAskCase):
    def test_a_first_dispatch_records_no_ask(self):
        self.assertIsNone(self.dispatch(), "precondition: nothing was asked")
        self.assertEqual(self.events("dispatch-ask"), [])

    def test_a_read_only_peer_overlapping_records_no_ask(self):
        self.dispatch()
        self.assertIsNone(self.dispatch("explorer"), "precondition: nothing was asked")
        self.assertEqual(self.events("dispatch-ask"), [])

    def test_an_unattended_nudge_is_not_counted_as_an_ask(self):
        """Nobody was there to answer, so it is `ask-unattended` — the distinction
        `_ask` keeps, and the double count it exists to prevent."""
        self.dispatch()
        out = self.dispatch(permission_mode=hooks.UNATTENDED_MODE)
        self.assertIsNotNone(out, "fixture never reached the nudge")
        self.assertEqual(out["hookSpecificOutput"]["permissionDecision"], "allow",
                         "an unattended nudge allows rather than floors the run at a prompt")
        self.assertEqual(len(self.events("ask-unattended")), 1)
        self.assertEqual(self.events("dispatch-ask"), [], "counted once, under one name")


class TestEveryAskSiteRecordsAnAsk(unittest.TestCase):
    """The structural half: a fourth call site cannot quietly forget.

    `_ask`'s docstring defends keeping the trace OUTSIDE it — callers name their own event —
    and that reasoning is sound, so the fix is not to move the trace inward. This asserts the
    property that reasoning assumes instead: any function that asks also records an
    ask-shaped event. `test_guards_that_must_not_be_refactored_away.py` is the precedent for
    holding a rule the type system cannot.
    """

    @staticmethod
    def _functions_that_ask() -> dict[str, ast.FunctionDef]:
        src = Path(hooks.__file__).read_text()
        tree = ast.parse(src)
        out = {}
        for node in ast.walk(tree):
            if not isinstance(node, ast.FunctionDef) or node.name == "_ask":
                continue
            for call in ast.walk(node):
                if (isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                        and call.func.id == "_ask"):
                    out[node.name] = node
                    break
        return out

    @staticmethod
    def _ask_shaped_events(fn: ast.FunctionDef) -> list[str]:
        names = []
        for call in ast.walk(fn):
            if (isinstance(call, ast.Call) and isinstance(call.func, ast.Name)
                    and call.func.id == "_trace" and call.args
                    and isinstance(call.args[0], ast.Constant)
                    and isinstance(call.args[0].value, str)
                    and "ask" in call.args[0].value):
                names.append(call.args[0].value)
        return names

    def test_the_known_sites_are_all_found(self):
        """Precondition: the scan sees the real call sites, so a pass means something."""
        self.assertEqual(set(self._functions_that_ask()),
                         {"pretooluse_dispatch", "pretooluse_edit"})

    def test_every_function_that_asks_also_traces_an_ask(self):
        for name, fn in sorted(self._functions_that_ask().items()):
            with self.subTest(handler=name):
                self.assertTrue(self._ask_shaped_events(fn),
                                f"{name} calls _ask but records no ask-shaped event, so its "
                                f"approvals count against a denominator nothing incremented")


if __name__ == "__main__":
    unittest.main()
