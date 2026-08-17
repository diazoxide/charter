"""`session.current` answers on any harness, not just Claude Code.

`$CHARTER_SESSION_ID` is what a harness sets when it knows its own session id — opencode's
plugin reads it off `shell.env`'s `input.sessionID` per invocation. `$CLAUDE_CODE_SESSION_ID`
stays as the fallback so nothing already running regresses.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import session


class SessionIsHarnessNeutral(unittest.TestCase):
    def test_the_harness_neutral_variable_is_read(self):
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "ses_abc"}, clear=True):
            self.assertEqual(session.current(), "ses_abc")

    def test_claude_codes_own_variable_still_answers(self):
        with mock.patch.dict(os.environ, {"CLAUDE_CODE_SESSION_ID": "cc-1"}, clear=True):
            self.assertEqual(session.current(), "cc-1")

    def test_the_explicit_argument_still_outranks_both(self):
        """The status line receives its id in the stdin payload because Claude Code
        scrubs the environment — that path must not become second."""
        with mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "ses_abc"}, clear=True):
            self.assertEqual(session.current("from-payload"), "from-payload")

    def test_absence_is_still_representable(self):
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertIsNone(session.current())


if __name__ == "__main__":
    unittest.main()
