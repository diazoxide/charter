"""Under `bypassPermissions` there is nobody to answer, so charter must not ask.

A hook `ask` is not a suggestion the host may overrule — Claude Code's changelog: *"Fixed
auto mode overriding a PreToolUse hook's `ask` decision for unsandboxed Bash — a hook `ask`
now floors the decision at a prompt."* So every `_ask` charter emits is an unconditional
stop that no permission mode can lift, and an unattended run walks into it.

Two deliberate boundaries, both asserted here:

* **`bypassPermissions` ONLY.** `auto` usually does have a human watching; suppressing a
  nudge there would silently swallow something the operator wanted. Being wrong toward
  asking costs one prompt, being wrong toward silence costs a guard — so this fails toward
  the prompt.
* **The floor is untouched.** `bypassPermissions` is the operator saying *stop asking me*,
  not *stop knowing things*. Every `deny` still denies.

The nudge these cases drive used to be the clone-commit one, on `pretooluse`. #371 deleted
it, so the fixture moved to the overlapping-dispatch nudge on `pretooluse_dispatch` — a real
nudge on real committed frontmatter (`dispatch-isolation: worktree`), which is the shape
that matters here: the payload has to reach `_ask` for the downgrade to happen at all.

That fixture only fires when a code-writing peer is already in flight, so it can stop firing
silently. Every mode assertion below is therefore paired with `assert_reached`, which fails
if the handler emitted nothing — otherwise "no ask under bypassPermissions" would pass for a
plane where nothing could ever have asked.
"""

import unittest

from tests._isolation import run_hook
from tests.test_hooks import InAControlPlane
from charter import hooks, trace

SSH_CLONE = "git clone git@github.com:acme/x.git"


class ModeCase(InAControlPlane):
    """Two decision paths: the NUDGE (dispatch) and the FLOOR (Bash guards)."""

    def setUp(self) -> None:
        super().setUp()
        self.make_persona("coder", **{"dispatch-isolation": "worktree"})

    def _dispatch(self, mode, sid, tuid):
        payload = {"tool_name": "Task", "tool_input": {"subagent_type": "coder"},
                   "session_id": sid, "tool_use_id": tuid}
        if mode is not None:
            payload["permission_mode"] = mode
        return run_hook(hooks.pretooluse_dispatch, payload)

    def nudge(self, mode: str | None, sid="s"):
        """One overlapping dispatch — the first puts `coder` in flight, the second nudges."""
        self._dispatch(mode, sid, "tu_0")
        r = self._dispatch(mode, sid, "tu_1")
        self.assert_reached(r)
        return r["hookSpecificOutput"]["permissionDecision"]

    def assert_reached(self, r):
        self.assertIsNotNone(r, "fixture never reached `_ask` — this assertion proves nothing")

    def decide(self, cmd: str, mode: str | None, sid="s"):
        """The FLOOR path: a Bash command through the guards."""
        payload = {"tool_input": {"command": cmd}, "cwd": str(self.tmp),
                   "session_id": sid, "tool_use_id": "tu_1"}
        if mode is not None:
            payload["permission_mode"] = mode
        r = run_hook(hooks.pretooluse, payload)
        return None if r is None else r["hookSpecificOutput"]["permissionDecision"]


class TestBypassPermissionsTurnsAnAskIntoAnAllow(ModeCase):
    def test_the_nudge_no_longer_blocks(self):
        self.assertEqual("allow", self.nudge("bypassPermissions"))

    def test_it_is_still_counted(self):
        """Suppressed is not the same as invisible — the tally must still show it fired."""
        self.nudge("bypassPermissions")
        self.assertIn("ask-unattended", [e["event"] for e in trace.read("s")])


class TestEveryOtherModeStillAsks(ModeCase):
    def test_auto_still_asks(self):
        """`auto` usually HAS a human watching. Fail toward the prompt."""
        self.assertEqual("ask", self.nudge("auto"))

    def test_default_still_asks(self):
        self.assertEqual("ask", self.nudge("default"))

    def test_plan_still_asks(self):
        self.assertEqual("ask", self.nudge("plan"))

    def test_an_absent_mode_still_asks(self):
        """Hosts that send no `permission_mode` must not be read as unattended."""
        self.assertEqual("ask", self.nudge(None))

    def test_an_unknown_mode_still_asks(self):
        self.assertEqual("ask", self.nudge("some-future-mode"))


class TestTheFloorIsNotAPermission(ModeCase):
    """`bypassPermissions` means stop asking me, not stop knowing things."""

    def test_the_single_credential_rule_still_denies(self):
        self.assertEqual("deny", self.decide(SSH_CLONE, "bypassPermissions"))

    def test_the_secret_leak_rule_still_denies(self):
        self.assertEqual("deny",
                         self.decide("charter persona secret get v k --reveal",
                                     "bypassPermissions"))

    def test_a_vault_read_still_denies(self):
        self.assertEqual("deny",
                         self.decide("cat .charter/vaults/db.json", "bypassPermissions"))


class TestTheInjectedProseStopsTellingTheAgentToQuiz(InAControlPlane):
    """No permission mode can fix charter's own text — `hooks.py` tells the model to call
    `AskUserQuestion`, which hangs just as hard as a hook `ask` does."""

    def context(self, mode: str | None) -> str:
        payload = {"prompt": "build me a thing that does the whole flow end to end, "
                             "and make sure it handles every case we discussed",
                   "session_id": "s", "cwd": str(self.tmp)}
        if mode is not None:
            payload["permission_mode"] = mode
        r = run_hook(hooks.userpromptsubmit, payload)
        out = (r or {}).get("hookSpecificOutput") or {}
        return out.get("additionalContext") or ""

    def test_attended_runs_are_unchanged(self):
        self.assertIn("AskUserQuestion", self.context("default"))

    def test_unattended_runs_are_not_told_to_quiz(self):
        """The block still NAMES the tool — to forbid it. An explicit prohibition beats
        silence, because silence leaves the model's default (quiz at a fork) in force. So
        the assertion is about the imperative, not about the word appearing."""
        ctx = self.context("bypassPermissions")
        self.assertNotIn("**Then quiz**", ctx)
        self.assertIn("Do NOT call AskUserQuestion", ctx)
        self.assertIn("Then decide", ctx)

    def test_unattended_runs_are_not_offered_the_human_only_step(self):
        """`/grill-with-docs` needs a human in the loop; offering it to nobody is a hang."""
        ctx = self.context("bypassPermissions")
        self.assertNotIn("Offer the human-only framing", ctx)
        self.assertIn("cannot run here", ctx)

    def test_the_guidance_itself_survives(self):
        """Suppression would lose the substance — scouting first is still correct with
        nobody watching. Only the consultation verb is wrong."""
        ctx = self.context("bypassPermissions")
        if ctx:
            self.assertIn("Scout first", ctx)


class TestAnUnattendedRunWithNoWorkspaceFailsFast(InAControlPlane):
    """The one block that does NOT get an assume-and-continue rewrite. Every other nudge
    names a preference; this one names a missing input, and guessing it silently claims
    somebody else's job for the whole session."""

    def context(self, mode: str | None) -> str:
        payload = {"session_id": "s", "cwd": str(self.tmp), "source": "startup"}
        if mode is not None:
            payload["permission_mode"] = mode
        r = run_hook(hooks.sessionstart, payload)
        out = (r or {}).get("hookSpecificOutput") or {}
        return out.get("additionalContext") or ""

    def test_it_refuses_rather_than_choosing(self):
        ctx = self.context("bypassPermissions")
        self.assertIn("STOP", ctx)
        self.assertNotIn("AskUserQuestion", ctx)

    def test_it_names_the_remedy_the_launcher_can_actually_apply(self):
        self.assertIn("CHARTER_WORKSPACE", self.context("bypassPermissions"))

    def test_an_attended_run_still_quizzes(self):
        self.assertIn("AskUserQuestion", self.context("default"))


class TestASuppressedAskIsCountedExactlyOnce(ModeCase):
    """`_ask` traces the suppression and the call site traces the ask — so an unattended
    run recorded BOTH, and the tally that exists to separate "asked" from "would have
    asked" counted one nudge as two. Found by running the hook, not by a test."""

    def events(self, sid):
        return [e["event"] for e in trace.read(sid)]

    def test_unattended_records_only_the_suppression(self):
        self.nudge("bypassPermissions", sid="u")
        self.assertEqual(["ask-unattended"], self.events("u"))

    def test_attended_records_only_the_ask(self):
        self.nudge("default", sid="d")
        self.assertEqual(["dispatch-ask"], self.events("d"))


if __name__ == "__main__":
    unittest.main()
