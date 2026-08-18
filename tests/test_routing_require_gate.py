"""`routing: require` — the tool-time half: an ask on the first edit with no dispatch.

At `advise` the roster is a line of context and nothing more. At `require` the acting
persona has declared that work of another persona's should actually leave this session, so
the first Write/Edit of a turn where the roster fired and nothing was dispatched surfaces a
permission prompt.

Two properties are load-bearing, and both were settled before any of this was written:

* **It asks; it never denies.** `toolgate.py` promises the same, and "hooks never break a
  turn" is a stated convention. The failure mode of denying is a genuine cross-cutting
  change — which the front door's own charter says stays with it — becoming unworkable,
  and the fix someone reaches for is `routing: off`, permanently.
* **It names no owner.** ADR 0016. The strongest sentence available is "the roster was
  shown and nothing was dispatched", which is a fact about this session rather than a
  claim about who owns the request.

Sub-agents are excluded *by construction* rather than by detection: the pending mark is
cleared the moment a dispatch begins, so inside the sub-agent there is nothing left to
fire. A persona that was handed work has by definition already been routed to, and telling
it to route again is the loop that gets a feature switched off on day one.
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import hooks
from tests._isolation import PersonaIso, run_hook

WORK = "refactor the release tagging flow, maybe something cleaner"


def _decision(r) -> str | None:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecision")


def _reason(r) -> str:
    return (r or {}).get("hookSpecificOutput", {}).get("permissionDecisionReason", "")


class RequireCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": "default"}))

    def p(self, name, **meta):
        return self.make_persona(name, role=name.title(), vault="none",
                                 **{"delegate-when": f"{name} work", **meta})

    def acting(self, name):
        return mock.patch.dict(os.environ, {"CHARTER_PERSONA": name})

    def prompt(self, text=WORK, sid="s1"):
        return run_hook(hooks.userpromptsubmit, {"session_id": sid, "prompt": text})

    def edit(self, sid="s1"):
        return run_hook(hooks.pretooluse_edit,
                        {"session_id": sid, "tool_name": "Edit",
                         "tool_input": {"file_path": "/tmp/x.py"}})

    def dispatch_starts(self, sid="s1"):
        return run_hook(hooks.pretooluse_dispatch,
                        {"session_id": sid, "tool_name": "Task",
                         "tool_input": {"subagent_type": "forge"}})


class TestItAsks(RequireCase):
    def test_an_edit_after_an_unfollowed_roster_asks(self):
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.assertEqual(_decision(self.edit()), "ask")

    def test_it_never_denies(self):
        """The one property protected everywhere else in this repo."""
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.assertNotEqual(_decision(self.edit()), "deny")

    def test_the_reason_states_the_fact_not_a_verdict(self):
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            reason = _reason(self.edit()).lower()
        self.assertIn("nothing was dispatched", reason)
        self.assertIn("forge", reason)

    def test_it_asks_once_per_turn_not_once_per_edit(self):
        """A prompt on every edit of a long turn is a prompt people click through."""
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.assertEqual(_decision(self.edit()), "ask")
            self.assertIsNone(_decision(self.edit()))


class TestItStaysQuiet(RequireCase):
    def test_advise_never_asks(self):
        """`advise` is context, not a gate — that is the whole difference between them."""
        self.p("steward", routing="advise")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.assertIsNone(_decision(self.edit()))

    def test_off_never_asks(self):
        self.p("steward", routing="off")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.assertIsNone(_decision(self.edit()))

    def test_a_dispatch_clears_it(self):
        """The routing happened. Asking afterwards would be charter arguing with a
        decision it asked for."""
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.dispatch_starts()
            self.assertIsNone(_decision(self.edit()))

    def test_an_edit_inside_a_dispatched_sub_agent_does_not_ask(self):
        """Excluded by construction: the dispatch cleared the mark, so a persona that was
        handed work is never told to hand it on."""
        self.p("steward", routing="require")
        self.p("forge", routing="require")
        with self.acting("steward"):
            self.prompt()
            self.dispatch_starts()
        with self.acting("forge"):
            self.assertIsNone(_decision(self.edit()))

    def test_a_later_turn_does_not_inherit_the_mark(self):
        """The gate has a cooldown, so a prompt or two later the roster does not re-fire.
        A mark that outlived its turn would ask about a roster nobody was shown."""
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.prompt()
            self.prompt("carry on", sid="s1")     # cooldown: no roster this time
            self.assertIsNone(_decision(self.edit()))

    def test_an_edit_with_no_prompt_before_it_never_asks(self):
        self.p("steward", routing="require")
        self.p("forge")
        with self.acting("steward"):
            self.assertIsNone(_decision(self.edit()))


if __name__ == "__main__":
    unittest.main()
