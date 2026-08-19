"""An `ask` charter cannot see the answer to is a nudge it can never justify.

`_ask` emitted 231 clone-commit prompts in 23 traced sessions and charter recorded not one
outcome — so "is this guard earning its interruptions?" had no evidentiary answer, and the
argument for keeping or deleting it could only be made from irritation. Same failure as the
`cmd=head` gap one level up: charter traced what it DECIDED and never what HAPPENED NEXT.

The signal is deterministic and already in the protocol: a hook `ask` blocks the tool, so a
`PostToolUse` for that same `tool_use_id` means it ran, which means it was approved. An ask
that is declined simply never gets one — its marker is still there, which is what makes
"asked 231 times, approved 231 times" countable.
"""

import unittest

from tests._isolation import run_hook
from tests.test_hooks import InAControlPlane
from charter import hooks, trace

ASKED = {"tool_input": {"command": "cd workspaces/default/x && git commit -m y"},
         "cwd": "", "session_id": "s", "tool_use_id": "tu_1"}


class AskOutcomeCase(InAControlPlane):
    def ask(self, **over):
        payload = {**ASKED, **over}
        payload["cwd"] = str(self.tmp / "workspaces" / "default" / "x")
        (self.tmp / "workspaces" / "default" / "x").mkdir(parents=True, exist_ok=True)
        r = run_hook(hooks.pretooluse, payload)
        self.assertEqual("ask", r["hookSpecificOutput"]["permissionDecision"], r)
        return payload

    def events(self, sid="s"):
        return [e["event"] for e in trace.read(sid)]


class TestAnApprovedAskIsRecorded(AskOutcomeCase):
    def test_the_tool_running_marks_it_approved(self):
        p = self.ask()
        run_hook(hooks.posttooluse_bash,
                 {"tool_name": "Bash", "session_id": p["session_id"],
                  "tool_use_id": p["tool_use_id"]})
        self.assertIn("ask-approved", self.events())

    def test_the_ask_itself_is_still_traced(self):
        self.ask()
        self.assertIn("ask", self.events())


class TestAnUnansweredAskIsNotRecordedAsApproved(AskOutcomeCase):
    def test_no_post_tool_use_means_no_approval(self):
        """A declined ask blocks the tool, so PostToolUse never fires for it."""
        self.ask()
        self.assertNotIn("ask-approved", self.events())

    def test_a_different_tool_call_does_not_resolve_it(self):
        """The correlation is per `tool_use_id` — another Bash call in the same session
        must not be mistaken for the answer to this one."""
        self.ask()
        run_hook(hooks.posttooluse_bash,
                 {"tool_name": "Bash", "session_id": "s", "tool_use_id": "tu_OTHER"})
        self.assertNotIn("ask-approved", self.events())


class TestItIsCheapAndSafeOnTheHotPath(AskOutcomeCase):
    def test_an_ordinary_bash_call_traces_nothing(self):
        """The overwhelmingly common case: no ask was pending, so the handler is a stat()
        and a return. It must not write a trace line per Bash call."""
        run_hook(hooks.posttooluse_bash,
                 {"tool_name": "Bash", "session_id": "quiet", "tool_use_id": "tu_x"})
        self.assertEqual([], self.events("quiet"))

    def test_a_missing_tool_use_id_is_survivable(self):
        self.assertIsNone(run_hook(hooks.posttooluse_bash,
                                   {"tool_name": "Bash", "session_id": "s"}))

    def test_garbage_input_never_raises(self):
        self.assertIsNone(run_hook(hooks.posttooluse_bash, {}))

    def test_it_resolves_only_once(self):
        """A replayed PostToolUse must not inflate the approval count."""
        p = self.ask()
        for _ in range(3):
            run_hook(hooks.posttooluse_bash,
                     {"tool_name": "Bash", "session_id": p["session_id"],
                      "tool_use_id": p["tool_use_id"]})
        self.assertEqual(1, self.events().count("ask-approved"))


class TestItIsWired(unittest.TestCase):
    """A handler the manifest does not dispatch is a handler that does not run — the lesson
    `test_vault_read_guard` already had to learn once."""

    def test_the_manifest_registers_a_bash_posttooluse(self):
        import json
        from pathlib import Path
        root = Path(__file__).resolve().parent.parent
        hooks_json = json.loads((root / "hooks" / "hooks.json").read_text())["hooks"]
        cmds = [h["command"] for e in hooks_json["PostToolUse"] for h in e["hooks"]]
        named = {c.split("charter hook ")[1].split()[0] for c in cmds if "charter hook " in c}
        self.assertIn("posttooluse-bash", named)

    def test_the_engine_knows_the_handler(self):
        self.assertIn("posttooluse-bash", hooks._HANDLERS)


if __name__ == "__main__":
    unittest.main()
