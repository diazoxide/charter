"""`trace` must resolve "who is this session" the way everything else does.

`charter.session` exists because three implementations of that question disagreed, and its
docstring names the failure a shared bucket causes. `trace._session` was a fourth, and the
one left behind: it read `CLAUDE_CODE_SESSION_ID` directly and never learned the neutral
`CHARTER_SESSION_ID`. Under any harness that sets only the neutral name — opencode's plugin
reads it off `shell.env` per invocation, Codex requires the same field names — every trace
line landed in `nosession`, mixing concurrent sessions into one bucket that is also
documented as prunable.

The bug is invisible on the harness charter was written against, which is why these assert
the neutral variable specifically.
"""

import os
import unittest
from unittest import mock

from tests._isolation import PersonaIso
from charter import config, session, trace


class TraceSessionCase(PersonaIso):
    def record_with(self, env: dict, **kw) -> list[str]:
        """Record one event with exactly `env` set, and return the trace file names."""
        clean = {k: v for k, v in os.environ.items()
                 if k not in ("CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID")}
        with mock.patch.dict(os.environ, {**clean, **env}, clear=True):
            trace.record("probe", **kw)
        return sorted(p.name for p in (config.PERSONA_STATE_DIR / "trace").glob("*.jsonl"))


class TestTheNeutralVariableIsHonoured(TraceSessionCase):
    def test_charter_session_id_names_the_file(self):
        self.assertEqual(["neutral.jsonl"], self.record_with({"CHARTER_SESSION_ID": "neutral"}))

    def test_it_does_not_fall_into_the_shared_bucket(self):
        """The regression: a neutral harness wrote every session into `nosession`."""
        self.assertNotIn(f"{session.NO_SESSION}.jsonl",
                         self.record_with({"CHARTER_SESSION_ID": "neutral"}))


class TestItAgreesWithTheSharedResolver(TraceSessionCase):
    """Same precedence as `session.bucket`, because it must BE `session.bucket`."""

    def test_explicit_wins_over_both_env_vars(self):
        files = self.record_with(
            {"CHARTER_SESSION_ID": "neutral", "CLAUDE_CODE_SESSION_ID": "claude"},
            session="explicit")
        self.assertEqual(["explicit.jsonl"], files)

    def test_neutral_wins_over_the_claude_specific_name(self):
        files = self.record_with(
            {"CHARTER_SESSION_ID": "neutral", "CLAUDE_CODE_SESSION_ID": "claude"})
        self.assertEqual(["neutral.jsonl"], files)

    def test_the_claude_specific_name_still_works_alone(self):
        """No session already running may regress the day the neutral name ships."""
        self.assertEqual(["claude.jsonl"], self.record_with({"CLAUDE_CODE_SESSION_ID": "claude"}))

    def test_absence_still_falls_back_to_the_shared_bucket(self):
        self.assertEqual([f"{session.NO_SESSION}.jsonl"], self.record_with({}))

    def test_the_name_is_sanitised_because_it_becomes_a_filename(self):
        """Separators must not survive — the value becomes a path segment. `.` is kept
        (it is in `session._SAFE`'s allowed set), so the assertion is about traversal,
        not about a particular spelling."""
        files = self.record_with({"CHARTER_SESSION_ID": "../../etc/passwd"})
        self.assertEqual(1, len(files))
        self.assertNotIn("/", files[0])
        self.assertTrue((config.PERSONA_STATE_DIR / "trace" / files[0]).exists())


class TestReadsFindWhatWritesWrote(TraceSessionCase):
    """A resolver used on write and not on read would be worse than the bug."""

    def test_round_trip_under_the_neutral_variable(self):
        clean = {k: v for k, v in os.environ.items()
                 if k not in ("CHARTER_SESSION_ID", "CLAUDE_CODE_SESSION_ID")}
        with mock.patch.dict(os.environ, {**clean, "CHARTER_SESSION_ID": "neutral"}, clear=True):
            trace.record("deny", reason="leak")
            self.assertEqual(["deny"], [e["event"] for e in trace.read()])


if __name__ == "__main__":
    unittest.main()
