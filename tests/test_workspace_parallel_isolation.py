"""Two Claude sessions must never share a workspace behind the user's back.

Parallel workspaces are charter's reason to exist, and `set_active`'s own docstring
promises "selecting a workspace in one pane never changes another". It did, and this is
the case that proves it does not:

    two Claude sessions in ONE terminal window
    session A: charter ws use user-reporting
    session B: (never chose anything) → silently on user-reporting, `source()` = terminal

`_terminal_id` preferred `WINDOWID`, which identifies a WINDOW — one window holds many
tabs and splits, so every session in it got the same "terminal" id, wrote the same pointer
and read each other's. Nothing here was covered by a test, which is why it shipped.

The rule now: key the pointer on something that identifies a PANE, or write none at all.
"""
from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import session, workspace
from tests._isolation import PersonaIso


def _env(**kw):
    """A clean environment with no pane/window hints except the ones given, and no tty —
    `_terminal_id` falls back to `os.ttyname(0)`, which in an interactive test runner
    would otherwise supply a real id and make these non-deterministic."""
    keep = {k: v for k, v in os.environ.items()
            if k not in (*session._PANE_ID_VARS, *session._WINDOW_ID_VARS)}
    return mock.patch.dict(os.environ, {**keep, **kw}, clear=True)


class WindowIdIsNotAPaneId(unittest.TestCase):
    def test_a_window_id_alone_yields_no_terminal_id(self):
        """The bug, stated directly. A window is not a pane."""
        with _env(WINDOWID="107374182523"), mock.patch.object(os, "ttyname",
                                                              side_effect=OSError):
            self.assertIsNone(workspace._terminal_id())

    def test_a_pane_id_is_used(self):
        with _env(TERM_SESSION_ID="w0t1p0:ABC"), mock.patch.object(os, "ttyname",
                                                                   side_effect=OSError):
            self.assertEqual(workspace._terminal_id(), "w0t1p0-ABC")

    def test_a_pane_id_wins_over_a_window_id(self):
        with _env(TERM_SESSION_ID="pane-1", WINDOWID="999"), \
             mock.patch.object(os, "ttyname", side_effect=OSError):
            self.assertEqual(workspace._terminal_id(), "pane-1")

    def test_tmux_and_screen_still_count(self):
        for var, val in (("TMUX_PANE", "%3"), ("STY", "1234.pts-0.host")):
            with self.subTest(var=var):
                with _env(**{var: val}), mock.patch.object(os, "ttyname",
                                                           side_effect=OSError):
                    self.assertIsNotNone(workspace._terminal_id())

    def test_the_tty_is_a_pane_and_still_used(self):
        with _env(), mock.patch.object(os, "ttyname", return_value="/dev/ttys004"):
            self.assertEqual(workspace._terminal_id(), "-dev-ttys004")


class TwoSessionsInOneWindowStayIndependent(PersonaIso):
    """The end-to-end case the user hit."""

    A = "aaaaaaaa-0000-0000-0000-000000000001"
    B = "bbbbbbbb-0000-0000-0000-000000000002"

    def test_choosing_in_one_session_does_not_move_the_other(self):
        with _env(WINDOWID="107374182523"), mock.patch.object(os, "ttyname",
                                                              side_effect=OSError):
            workspace.ensure("user-reporting")
            workspace.set_active("user-reporting", session_id=self.A)
            self.assertEqual(workspace.resolve(session_id=self.A), "user-reporting")
            self.assertEqual(workspace.resolve(session_id=self.B),
                             workspace.config.DEFAULT_WORKSPACE)

    def test_no_terminal_pointer_is_written_from_a_window_id(self):
        """The pointer itself is the leak — a session that never chose anything read it."""
        with _env(WINDOWID="107374182523"), mock.patch.object(os, "ttyname",
                                                              side_effect=OSError):
            workspace.ensure("alpha")
            workspace.set_active("alpha", session_id=self.A)
        written = list(workspace.config.TERMINALS_DIR.glob("*.workspace")) \
            if workspace.config.TERMINALS_DIR.exists() else []
        self.assertEqual(written, [])

    def test_a_real_pane_still_keeps_its_workspace_across_sessions(self):
        """The feature the terminal pointer exists for is intact where the terminal can
        actually tell its panes apart: same pane, new Claude session, same workspace."""
        with _env(TERM_SESSION_ID="pane-7"), mock.patch.object(os, "ttyname",
                                                               side_effect=OSError):
            workspace.ensure("beta")
            workspace.set_active("beta", session_id=self.A)
            # A different session id, same pane, and no pointer of its own yet.
            self.assertEqual(workspace.resolve(session_id=self.B), "beta")

    def test_a_different_pane_is_unaffected(self):
        with _env(TERM_SESSION_ID="pane-7"), mock.patch.object(os, "ttyname",
                                                               side_effect=OSError):
            workspace.ensure("beta")
            workspace.set_active("beta", session_id=self.A)
        with _env(TERM_SESSION_ID="pane-9"), mock.patch.object(os, "ttyname",
                                                               side_effect=OSError):
            self.assertEqual(workspace.resolve(session_id=self.B),
                             workspace.config.DEFAULT_WORKSPACE)


if __name__ == "__main__":
    unittest.main()
