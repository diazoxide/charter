"""What `workspace use` promises must be what it wrote.

`set_active` writes up to two pointers: a **per-terminal** one, which is what lets a pane
keep its workspace across closing and reopening Claude, and a **per-session** one, which
lasts exactly as long as the session. Only the first survives a restart.

It reported both as ``"session"`` — the terminal branch set the scope and the session
branch overwrote it — and `_scope_note` printed "kept across closing/reopening Claude" for
that value. So a terminal that could not supply a pane id (no ``TERM_SESSION_ID``, no
``TMUX_PANE``, no ``STY``, no ``SSH_TTY``, no tty — an agent shell, a CI runner) got the
persistence promise while nothing persistent had been written. Reopen, and the workspace
is `default` again with nothing having said so.

The scope is the *reach* of what was written, so it must name the strongest pointer that
actually landed, and the message must follow it rather than assume the good case.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock

from charter import config, workspace
from charter.commands_workspace import _scope_note
from tests._isolation import PersonaIso


class ScopeBase(PersonaIso):
    SID = "sess-scope-test"

    def setUp(self) -> None:
        super().setUp()
        config.WORKSPACES_DIR.mkdir(parents=True, exist_ok=True)
        (config.WORKSPACES_DIR / "alpha").mkdir(exist_ok=True)

        # Stand outside every workspace tree: the cwd rung outranks the pointers, and
        # this suite is about the pointers.
        self._cwd = os.getcwd()
        os.chdir(self.tmp)
        self.addCleanup(lambda: os.chdir(self._cwd))

        self._env = {k: os.environ.get(k)
                     for k in ("CLAUDE_CODE_SESSION_ID", "CHARTER_WORKSPACE")}
        os.environ["CLAUDE_CODE_SESSION_ID"] = self.SID
        os.environ.pop("CHARTER_WORKSPACE", None)
        self.addCleanup(self._restore_env)

    def _restore_env(self) -> None:
        for k, v in self._env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


class TestScopeNamesWhatWasWritten(ScopeBase):
    def test_a_pane_id_yields_terminal_scope(self):
        with mock.patch.object(workspace, "_terminal_id", return_value="pane-1"):
            self.assertEqual(workspace.set_active("alpha"), "terminal")

    def test_no_pane_id_yields_session_scope(self):
        with mock.patch.object(workspace, "_terminal_id", return_value=None):
            self.assertEqual(workspace.set_active("alpha"), "session")

    def test_terminal_scope_means_a_terminal_pointer_exists(self):
        """The claim and the file have to travel together."""
        with mock.patch.object(workspace, "_terminal_id", return_value="pane-1"):
            scope = workspace.set_active("alpha")
        self.assertEqual(scope, "terminal")
        self.assertTrue(workspace._terminal_file("pane-1").exists())

    def test_session_scope_leaves_nothing_a_new_session_can_read(self):
        """Which is the whole reason it must not claim to survive one."""
        with mock.patch.object(workspace, "_terminal_id", return_value=None):
            workspace.set_active("alpha")
        self.assertEqual(workspace.resolve(session_id="a-different-session"),
                         config.DEFAULT_WORKSPACE)

    def test_the_session_pointer_is_still_written_either_way(self):
        """Reporting the terminal scope must not cost the status line its pointer."""
        with mock.patch.object(workspace, "_terminal_id", return_value="pane-1"):
            workspace.set_active("alpha")
        self.assertEqual(workspace.resolve(session_id=self.SID), "alpha")


class TestAskingWhatOneSessionChose(ScopeBase):
    """`workspace.for_session` — the per-session pointer rung, asked about a session that
    is not this process's.

    Public since #512, for one caller that genuinely has to distinguish it from the rest of
    the chain: a frame panel. Inside a frame the frame IS the charter session, so `charter
    workspace use` writes this pointer under the FRAME's id and `docs/frame.md` promises
    that "moves the panels too". The panels also carry a launch-time answer the launcher
    recorded for them, and an operator's live choice has to outrank it — which is only
    askable if this rung can be asked ON ITS OWN. `resolve()` cannot answer it: it returns
    a workspace whatever happened, and `source()`'s label is a sentence for a status line.
    """

    def test_it_names_the_workspace_that_session_chose(self):
        with mock.patch.object(workspace, "_terminal_id", return_value=None):
            workspace.set_active("alpha")
        self.assertEqual(workspace.for_session(self.SID), "alpha")

    def test_a_session_that_chose_nothing_answers_none_rather_than_a_default(self):
        """The distinction the caller needs and `resolve()` cannot make: `resolve` would
        answer `default` here, which is a workspace, not "nobody chose"."""
        self.assertIsNone(workspace.for_session("a-session-that-never-chose"))
        self.assertEqual(workspace.resolve(session_id="a-session-that-never-chose"),
                         config.DEFAULT_WORKSPACE)

    def test_it_answers_for_the_session_asked_about_not_this_process(self):
        """The whole reason it takes an argument: a panel asks about the FRAME's id while
        running as its own process."""
        with mock.patch.object(workspace, "_terminal_id", return_value=None):
            workspace.set_active("alpha")
        self.assertIsNone(workspace.for_session("some-other-session"))

    def test_a_name_that_could_escape_the_workspaces_directory_is_refused(self):
        """It feeds `workspace_dir()`'s join like every other rung, and #442 is what an
        unchecked `../../` in that position cost once already."""
        config.SESSIONS_DIR.mkdir(parents=True, exist_ok=True)
        workspace._session_file("hostile").write_text("../../escaped\n")
        self.assertIsNone(workspace.for_session("hostile"))

    def test_no_session_id_at_all_is_not_an_error(self):
        self.assertIsNone(workspace.for_session(""))


class TestTheMessageFollowsTheScope(ScopeBase):
    def test_terminal_scope_may_promise_a_restart(self):
        self.assertIn("reopening", _scope_note("terminal"))

    def test_session_scope_must_not_promise_a_restart(self):
        note = _scope_note("session")
        self.assertNotIn("reopening", note)
        self.assertIn("session", note.lower())

    def test_session_scope_says_what_happens_instead(self):
        """A limit the reader has to discover by losing their workspace was concealed."""
        self.assertIn(config.DEFAULT_WORKSPACE, _scope_note("session"))

    def test_nothing_written_promises_nothing(self):
        self.assertEqual(_scope_note("none"), "")


if __name__ == "__main__":
    unittest.main()
