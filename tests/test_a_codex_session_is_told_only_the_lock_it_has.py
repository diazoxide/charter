"""#954: a Codex session outside a frame has no workspace lock, and two things said it did.

**Why there is none.** `workspace.set_active` writes its lock under `session.current()`,
which reads an explicit id, `$CHARTER_SESSION_ID` and `$CLAUDE_CODE_SESSION_ID` and nothing
else. Codex's `shell_environment_policy.set` holds constants, so no per-session id reaches a
Codex shell, and `charter workspace use` typed there writes a terminal pointer and no lock.
The pointer is a selection: `is_locked` never reads it, so the next `use` switches. Measured
in the issue with codex-cli 0.147.0 through `codex sandbox`; the shell side is what these
cases stage, since nothing here runs Codex.

**What said otherwise.**

* `harness/codex.py`'s `session-lock` deficit said the lock "falls back to the terminal-pane
  key". There is no such fallback.
* SessionStart's confirm nudge told the session that `charter workspace use` **locks** the
  workspace. The hook has a payload `session_id` to look a lock up by, and nothing in the
  session can write one under it.

#939 had already fixed the third, the CLI's own line (`— unlocked.`).

**The cases pin each sentence to what confirming then does**, not to a spelling: the nudge
promises a lock exactly when a second `use` is refused, and the deficit's two claims are
each asserted beside the refusal (or its absence) that makes them true. A fix that dropped
the promise everywhere fails the Claude Code case; one that kept it fails the Codex case.

**The promise is withheld only where two things agree**: the hook sees no session id, and
the harness declares the `session-lock` deficit. A hook's environment is not its shell's,
so a Claude Code hook that sees no id still promises the lock its shell takes.
"""

from __future__ import annotations

import io
import os
import re
import unittest
from contextlib import redirect_stderr, redirect_stdout
from types import SimpleNamespace
from unittest import mock

from charter import commands_workspace, hooks, workspace
from charter.frame import state
from charter.harness import registry

from tests._isolation import PersonaIso, PlaneIso, no_background_refresh, run_hook

#: What a Codex hook payload carries as `session_id`. Codex's own id, which no `charter`
#: command in that session's shell can see.
CODEX_PAYLOAD_SID = "019a7c3e-0000-7000-8000-000000000954"

#: The mark every rendering of the confirm nudge opens with, attended or not.
_NUDGE_MARK = "⬢ "

#: A promise of a lock, in any of the nudge's spellings: "locks", "locked", "lock it".
_PROMISES_A_LOCK = re.compile(r"\block", re.IGNORECASE)


def _nudge(ctx: str) -> str:
    """The confirm nudge out of a briefing, or ``""``. Parts are joined by a blank line and
    this one holds none of its own."""
    return next((p for p in ctx.split("\n\n") if p.startswith(_NUDGE_MARK)), "")


class TheConfirmNudgePromisesOnlyTheLockConfirmingTakes(PlaneIso):
    """SessionStart's nudge, beside the `workspace use` it tells the session to run."""

    def setUp(self) -> None:
        super().setUp()
        no_background_refresh(self)   # SessionStart forks the newer-charter check (#938)
        for n in ("north", "south"):
            workspace.ensure(n)
        # The pane is STATED, as the issue's measurement had one: with no pane either,
        # `set_active` writes nothing at all, which is a different sentence.
        self.enterContext(mock.patch.object(workspace, "_terminal_id",
                                            return_value="pane-954"))

    def _use(self, name: str) -> int:
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
            return commands_workspace.cmd_workspace_use(
                SimpleNamespace(name=name, force=False, create=False))

    def _told_then_did(self, env: dict, payload: dict) -> tuple[str, bool]:
        """What the briefing promised, and whether confirming as told then refused a switch.

        Both halves in the one environment, because that is the claim: the hook and the
        session's own `charter` commands run under the same harness process.
        """
        with mock.patch.dict(os.environ, env):
            said = run_hook(hooks.sessionstart, payload) or {}
            nudge = _nudge(said.get("hookSpecificOutput", {}).get("additionalContext", ""))
            self.assertTrue(nudge, "the briefing carried no confirm nudge, so nothing is "
                                   "measured")
            self.assertEqual(self._use("north"), 0)
            refused = self._use("south") == 2
        return nudge, refused

    def test_a_codex_session_outside_a_frame_is_not_promised_a_lock(self):
        """Measured before: "That **locks** the workspace for the session — it can't be
        switched mid-session", and `use south` after `use north` succeeded."""
        nudge, refused = self._told_then_did({"CHARTER_HARNESS": "codex"},
                                             {"session_id": CODEX_PAYLOAD_SID})
        self.assertFalse(refused, "the fixture has a lock after all, so this measures nothing")
        self.assertIsNone(_PROMISES_A_LOCK.search(nudge), nudge)

    def test_an_unattended_codex_run_is_not_told_a_guess_would_lock(self):
        """The unattended variant said a guessed workspace would be locked for the session.
        It would be claimed, which is still the reason to stop; it would not be locked."""
        nudge, refused = self._told_then_did(
            {"CHARTER_HARNESS": "codex"},
            {"session_id": CODEX_PAYLOAD_SID, "permission_mode": hooks.UNATTENDED_MODE})
        self.assertIn("STOP", nudge)
        self.assertFalse(refused)
        self.assertIsNone(_PROMISES_A_LOCK.search(nudge), nudge)

    def test_a_claude_code_session_outside_a_frame_is_still_promised_its_lock(self):
        """The other side, green before the fix and kept green by it. Claude Code puts its
        session id in the environment of the processes it spawns, so `use` locks there and
        the nudge has to go on saying so."""
        sid = "cc-954-attended"
        nudge, refused = self._told_then_did(
            {"CHARTER_HARNESS": "claude-code", "CLAUDE_CODE_SESSION_ID": sid},
            {"session_id": sid})
        self.assertTrue(refused)
        self.assertIsNotNone(_PROMISES_A_LOCK.search(nudge), nudge)

    def test_an_unattended_claude_code_run_is_still_told_a_guess_would_lock(self):
        sid = "cc-954-unattended"
        nudge, refused = self._told_then_did(
            {"CHARTER_HARNESS": "claude-code", "CLAUDE_CODE_SESSION_ID": sid},
            {"session_id": sid, "permission_mode": hooks.UNATTENDED_MODE})
        self.assertTrue(refused)
        self.assertIn("lock it for the session", nudge)

    def test_a_claude_code_hook_that_sees_no_session_id_still_promises_the_lock(self):
        """The hook's environment is not the shell's, and a missing id in it is not
        evidence of none in the shell. Whether Claude Code hands `$CLAUDE_CODE_SESSION_ID` to
        a hook has not been measured; that its Bash shell has it, and that `use` locks
        there, has. So Claude Code, which declares no `session-lock` deficit, keeps the
        sentence whatever the hook sees. Red against a promise keyed on the hook's id alone,
        which dropped it here."""
        sid = "cc-954-hook-without-id"
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code"}):
            said = run_hook(hooks.sessionstart, {"session_id": sid}) or {}
            nudge = _nudge(said.get("hookSpecificOutput", {}).get("additionalContext", ""))
        self.assertTrue(nudge, "the briefing carried no confirm nudge")
        # The session's shell, where the id is: confirming there is what the sentence says.
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "claude-code",
                                          "CLAUDE_CODE_SESSION_ID": sid}):
            self.assertEqual(self._use("north"), 0)
            self.assertEqual(self._use("south"), 2)
        self.assertIsNotNone(_PROMISES_A_LOCK.search(nudge), nudge)

    def test_a_context_file_rendered_with_no_harness_and_no_id_keeps_the_sentence(self):
        """opencode reads this briefing from a file `charter init` renders, in whatever
        process ran `init` — often a shell with no session id and no harness at all. The
        opencode session that reads it does lock (its plugin sets `$CHARTER_SESSION_ID` per
        shell), and neither an unknown harness nor opencode declares `session-lock`, so the
        file keeps the sentence."""
        nudge = _nudge(hooks.context_block())
        self.assertTrue(nudge, "the context block carried no confirm nudge")
        self.assertIsNotNone(_PROMISES_A_LOCK.search(nudge), nudge)
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "opencode"}):
            nudge = _nudge(hooks.context_block())
        self.assertIsNotNone(_PROMISES_A_LOCK.search(nudge), nudge)


class TheCodexDeficitDescribesTheLockACodexShellGets(PersonaIso):
    """`charter harness list` and `doctor` print this sentence to someone choosing a harness."""

    def setUp(self) -> None:
        super().setUp()
        for n in ("north", "south"):
            workspace.ensure(n)
        self.detail = next(d.detail for d in registry.get("codex").deficits
                           if d.key == "session-lock")

    def test_outside_a_frame_it_says_there_is_no_lock_and_names_no_fallback(self):
        """Measured before: "the workspace lock falls back to the terminal-pane key". The
        terminal pointer is written, and it refuses nothing."""
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex"}), \
                mock.patch.object(workspace, "_terminal_id", return_value="codex-pane"):
            self.assertEqual(workspace.set_active("north"), "terminal")
            self.assertNotEqual(workspace.set_active("south"), "locked")
            self.assertIsNone(workspace.is_locked())
        self.assertNotIn("falls back", self.detail)
        self.assertIn("no workspace lock outside a frame", self.detail)

    def test_inside_a_frame_it_says_the_chat_is_locked(self):
        """The frame is what supplies the id: `$CHARTER_SESSION_ID` is in Codex's own
        environment and reaches its shell (the issue's first measured row), and the chat's
        launch record is its lock (#936). So the deficit is scoped to outside a frame, and
        says what a frame changes."""
        chat = "north.1"
        state.frame_dir(chat, create=True)
        state.record_workspace(chat, "north")
        state.record_identity(chat, {"CHARTER_HARNESS": "codex",
                                     "CHARTER_WORKSPACE": "", "CHARTER_PERSONA": ""})
        with mock.patch.dict(os.environ, {"CHARTER_HARNESS": "codex",
                                          "CHARTER_SESSION_ID": chat}):
            self.assertEqual(workspace.set_active("south", terminal_id=""), "locked")
        self.assertRegex(self.detail, r"[Ii]nside a frame\b.*\blocked\b")


if __name__ == "__main__":
    unittest.main()
