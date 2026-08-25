"""One answer to "which session is this", and the leak that came from having three.

`workspace._session_id` returned ``None`` when there was no session; `persona._session_id`
returned the string ``"nosession"``. That sentinel is a SHARED KEY, and
`persona.gc_ephemeral` compared it against the id of the session it was told to preserve —
so when the GC itself ran without one, which is most of the time since it runs from a
hook, ``nosession`` compared equal to "the live session" and was skipped on every pass.
Ephemeral scratch from every id-less session accumulated there forever, which is the
opposite of what "ephemeral" promises.
"""
from __future__ import annotations

import os
import time
import unittest
from unittest import mock

from charter import config, persona, session, workspace
from tests import _envguard
from tests._isolation import PersonaIso


class AbsenceIsRepresentable(unittest.TestCase):
    def setUp(self) -> None:
        # Outside a frame, with no session id and no pinned workspace: stated here
        # rather than inherited from the shell the suite was launched from
        # (#519, #521, #528).
        _envguard.unset_all()

    def test_current_is_none_when_there_is_no_session(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(session.current())

    def test_current_reads_the_environment(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_SESSION_ID": "abc-123"}):
            self.assertEqual(session.current(), "abc-123")

    def test_an_explicit_id_wins(self):
        """The status line receives the id in its stdin payload — Claude Code scrubs its
        environment."""
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_SESSION_ID": "env"}):
            self.assertEqual(session.current("explicit"), "explicit")

    def test_the_id_is_sanitised_because_it_becomes_a_filename(self):
        self.assertEqual(session.current("../../etc/passwd"), "....etcpasswd")

    def test_bucket_falls_back_to_the_shared_name(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(session.bucket(), session.NO_SESSION)

    def test_both_readers_now_agree(self):
        """They disagreed about the case that matters — what absence means."""
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_SESSION_ID": "s-1"}):
            self.assertEqual(workspace._session_id(), "s-1")
            self.assertEqual(persona._session_id(), "s-1")
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(workspace._session_id())
            self.assertEqual(persona._session_id(), session.NO_SESSION)


class TheSharedBucketIsCollectable(PersonaIso):
    """The leak. `gc_ephemeral` must never treat the shared bucket as the live session."""

    def _bucket(self, name: str, age_hours: float = 0.0):
        d = config.PERSONA_STATE_DIR / "ephemeral" / name / "p"
        d.mkdir(parents=True, exist_ok=True)
        (d / "note.md").write_text("scratch")
        if age_hours:
            when = time.time() - age_hours * 3600
            for p in (config.PERSONA_STATE_DIR / "ephemeral" / name).rglob("*"):
                os.utime(p, (when, when))
            os.utime(config.PERSONA_STATE_DIR / "ephemeral" / name, (when, when))
        return d

    def _names(self):
        root = config.PERSONA_STATE_DIR / "ephemeral"
        return sorted(p.name for p in root.iterdir()) if root.exists() else []

    def test_a_stale_shared_bucket_is_pruned_when_there_is_no_current_session(self):
        """The exact case that never fired: the GC runs from a hook, usually with no id."""
        self._bucket(session.NO_SESSION, age_hours=24)
        self.assertEqual(persona.gc_ephemeral(current=None), 1)
        self.assertNotIn(session.NO_SESSION, self._names())

    def test_a_recently_touched_bucket_survives_on_AGE_not_on_being_current(self):
        """Age is the guard when there is no session to preserve — concurrent live
        sessions are still never clobbered."""
        self._bucket(session.NO_SESSION, age_hours=0)
        persona.gc_ephemeral(current=None)
        self.assertIn(session.NO_SESSION, self._names())

    def test_the_current_session_is_still_exempt(self):
        self._bucket("live-1", age_hours=24)
        persona.gc_ephemeral(current="live-1")
        self.assertIn("live-1", self._names())

    def test_other_stale_sessions_are_still_pruned(self):
        self._bucket("old-1", age_hours=24)
        self._bucket("live-1", age_hours=0)
        persona.gc_ephemeral(current="live-1")
        self.assertEqual(self._names(), ["live-1"])


if __name__ == "__main__":
    unittest.main()
