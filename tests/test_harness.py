"""Which harness charter is running inside — one answer, read at the edge.

ADR 0015: the harness injects ``$CHARTER_HARNESS`` into every shell it spawns, so
"which harness am I?" is answered once instead of inside every function that would
otherwise sniff for Claude-Code-shaped environment variables.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import harness


class HarnessCurrent(unittest.TestCase):
    def test_the_harness_names_itself_in_the_environment(self):
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}, clear=True):
            self.assertEqual(harness.current(), "opencode")

    def test_an_unretrofitted_claude_code_is_still_recognised(self):
        """`$CHARTER_HARNESS` reaches Claude Code only once `init` has written it into
        settings. Until then — and in every session already running — the plugin's own
        variable is the evidence, so detection must not regress to "no harness"."""
        with mock.patch.dict(os.environ, {"CLAUDE_PLUGIN_ROOT": "/p"}, clear=True):
            self.assertEqual(harness.current(), "claude-code")


class HarnessDeficits(unittest.TestCase):
    """A harness whose ceiling is lower must say where. Absence is not health
    (`tests/test_doctor_absent_is_not_health.py`) — a capability charter cannot offer here
    is indistinguishable from a broken integration unless something names it."""

    def test_opencode_names_the_capabilities_it_cannot_carry(self):
        keys = {d.key for d in harness.deficits(harness.OPENCODE)}
        self.assertEqual(keys, {"status-bar", "prompt-hook", "ask-decisions"})

    def test_the_guards_that_refuse_are_not_a_ceiling(self):
        """`ask-decisions` is a narrow claim and must stay narrow. A DENY is carried in
        full here — `tool.execute.before` throwing is what denial IS — so the vault guard,
        the one-credential rule and the containment rule all refuse on opencode exactly as
        they do on Claude Code (#433). Only the middle answer has no spelling."""
        d = next(d for d in harness.deficits(harness.OPENCODE) if d.key == "ask-decisions")
        self.assertIn("Denials are unaffected", d.detail)

    def test_the_session_lock_is_not_a_ceiling(self):
        """Verified against opencode 1.18.18, not its docs: `shell.env` receives
        ``{cwd, sessionID, callID}`` per invocation. The published example shows `cwd`
        alone, which is what this deficit was written from — and a deficit charter reports
        that is not real is worse than none, because it argues against a capability it
        actually has."""
        keys = {d.key for d in harness.deficits(harness.OPENCODE)}
        self.assertNotIn("session-lock", keys)

    def test_every_deficit_carries_a_sentence_a_reader_can_act_on(self):
        for d in harness.deficits(harness.OPENCODE):
            with self.subTest(key=d.key):
                self.assertTrue(d.detail.strip(), f"{d.key} has no detail")

    def test_claude_code_has_no_deficits(self):
        self.assertEqual(harness.deficits(harness.CLAUDE_CODE), ())


if __name__ == "__main__":
    unittest.main()
