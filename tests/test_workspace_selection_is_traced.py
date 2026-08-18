"""Selecting a workspace leaves a trace, so "who moved my workspace" is answerable.

`persona.set_active` has always recorded a `persona-use` event. `workspace.set_active` —
the function that writes BOTH pointer files and the session lock — recorded nothing at all,
and neither did the two other things that can change what a session resolves to: a refused
switch, and the SessionStart reconcile that seeds a pointer from the pane.

That asymmetry is what #254 cost. A session appeared to have been moved to a workspace
nobody selected; the only way to investigate was to read pointer files after the fact,
where a file says *what* it holds and never *who wrote it, when, or why*. Two separate
investigations reached confident wrong conclusions from that evidence before a transcript
settled it — and a transcript is not a thing charter can rely on being kept.

Nothing here changes what is written. It records what was already happening, in the store
built for exactly this question, which is the cheapest possible answer to "prove it".
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import trace, workspace
from tests._isolation import PersonaIso

SID = "s-trace"


class TraceCase(PersonaIso):
    def setUp(self) -> None:
        super().setUp()
        self.enterContext(mock.patch.dict(
            os.environ,
            {k: v for k, v in os.environ.items()
             if k not in ("CHARTER_WORKSPACE", "CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID",
                          "TERM_SESSION_ID", "TMUX_PANE", "STY", "SSH_TTY")},
            clear=True))
        self.enterContext(mock.patch("os.ttyname", side_effect=OSError("no tty")))
        workspace.ensure("alpha")
        workspace.ensure("beta")

    def events(self, kind: str | None = None) -> list[dict]:
        evs = trace.read(SID)
        return [e for e in evs if kind is None or e.get("event") == kind]


class TestASelectionIsRecorded(TraceCase):
    def test_choosing_a_workspace_records_it(self):
        workspace.set_active("alpha", session_id=SID)
        self.assertEqual([e["workspace"] for e in self.events("workspace-use")], ["alpha"])

    def test_it_lands_in_the_trace_of_the_session_that_made_it(self):
        """The whole point: the record identifies WHO selected, and a per-session file is
        that identification. #254 turned on which session a pointer belonged to."""
        workspace.set_active("alpha", session_id=SID)
        self.assertEqual(trace.read("someone-else"), [])

    def test_the_reach_is_recorded_with_it(self):
        """`session` vs `terminal` vs `plane` is what decides whether a selection outlives
        the session, and it is the first thing asked when one appears to have persisted."""
        workspace.set_active("alpha", session_id=SID)
        self.assertIn(self.events("workspace-use")[0].get("scope"),
                      ("session", "terminal", "none"))


class TestARefusalIsRecordedToo(TraceCase):
    def test_a_locked_session_refusing_a_switch_says_so(self):
        """A refusal explains an absence — "why is it still on alpha" — and an absence is
        the hardest thing to investigate after the fact, because nothing changed."""
        workspace.set_active("alpha", session_id=SID)
        workspace.set_active("beta", session_id=SID)
        refused = self.events("workspace-refused")
        self.assertEqual(len(refused), 1)
        self.assertEqual(refused[0].get("workspace"), "beta")
        self.assertEqual(refused[0].get("locked_to"), "alpha")

    def test_a_forced_switch_is_recorded_as_forced(self):
        workspace.set_active("alpha", session_id=SID)
        workspace.set_active("beta", session_id=SID, force=True)
        self.assertTrue(self.events("workspace-use")[-1].get("forced"))


class TestASeededPointerIsRecorded(TraceCase):
    def test_reconcile_records_where_the_pointer_came_from(self):
        """This is the one write nobody typed: a pane's selection copied into a fresh
        session at SessionStart. If any write ever *does* look like it came from nowhere,
        this is the one — so it says where it came from."""
        with mock.patch.object(workspace, "_terminal_id", return_value="pane-7"):
            workspace.set_active("alpha", session_id="earlier-session")
            workspace.reconcile(session_id=SID)
        seeded = self.events("workspace-seeded")
        self.assertEqual(len(seeded), 1)
        self.assertEqual(seeded[0].get("workspace"), "alpha")
        self.assertEqual(seeded[0].get("from"), "terminal")

    def test_nothing_is_recorded_when_there_is_nothing_to_seed(self):
        workspace.reconcile(session_id=SID)
        self.assertEqual(self.events("workspace-seeded"), [])


class TestRecordingNeverCostsTheSelection(TraceCase):
    def test_a_broken_trace_does_not_break_the_write(self):
        """Observability must never break the thing it observes — the module's own rule."""
        with mock.patch.object(trace, "record", side_effect=RuntimeError("boom")):
            workspace.set_active("alpha", session_id=SID)
        self.assertEqual(workspace.resolve(session_id=SID, cwd=str(self.tmp)), "alpha")


if __name__ == "__main__":
    unittest.main()
