"""What the generated plugin RUNS — not what its tables say.

#433 was a routing table that nothing read: `PRE_HOOKS` could have been correct and the
call site still spelled ``charter hook pretooluse``, and for four releases that is exactly
the state that shipped. The round-one fix put the tables in and every new assertion looked
at the tables, so the same mutation — put the literal back at either call site, leave the
tables alone — kept the whole suite green. The guard was pinned by its name, not by its
identity.

So this module executes the shim, with a stand-in for Bun's `$` that records the COMMAND
LINE each call site builds. A handler name only counts here if the code actually passed
it. Skipped, loudly, when neither runtime is installed — never passed quietly.

`tests/fixtures/opencode_driver.mjs` is the stand-in. It projects `CHARTER_*` out of the
env the shim sets and nothing else: an assertion that dumped a whole environment would put
whatever the developer has exported into the failure log.
"""

from __future__ import annotations

import json
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

from charter.harness import opencode

_RUNTIME = shutil.which("bun") or shutil.which("node")
_DRIVER = Path(__file__).parent / "fixtures" / "opencode_driver.mjs"

#: opencode tool ids with no `PRE_HOOKS` entry — including two the plugin has never heard
#: of, because "a tool charter does not know" is the case the catch-all exists for.
_UNROUTED_PRE = ("bash", "glob", "skill", "webfetch", "todowrite", "question")

#: opencode tool ids with no `POST_HOOKS` entry. Nothing may be spawned for these.
_UNROUTED_POST = ("read", "grep", "glob", "webfetch", "todowrite", "question")

_ALLOW = '{"hookSpecificOutput":{"hookEventName":"PreToolUse"}}'


def _hook_of(command: str) -> str | None:
    """The handler name `charter hook` was actually invoked with, or ``None``."""
    m = re.search(r"\bcharter hook (\S+)", command)
    return m.group(1) if m else None


@unittest.skipIf(_RUNTIME is None, "neither bun nor node is installed")
class RuntimeDispatch(unittest.TestCase):
    """Drive the real shim and read back what it ran."""

    @classmethod
    def setUpClass(cls) -> None:
        cls.dir = Path(tempfile.mkdtemp(prefix="charter-oc-run-"))
        (cls.dir / "charter.mjs").write_text(opencode.SHIM)
        (cls.dir / "drive.mjs").write_text(_DRIVER.read_text())

    @classmethod
    def tearDownClass(cls) -> None:
        shutil.rmtree(cls.dir, True)

    def _run(self, *scenarios: dict) -> list[dict]:
        proc = subprocess.run([_RUNTIME, "drive.mjs"], input=json.dumps(list(scenarios)),
                              capture_output=True, text=True, timeout=120, cwd=self.dir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return json.loads(proc.stdout)

    def _before(self, tool: str, args: dict | None = None, reply: str = _ALLOW) -> dict:
        return self._run({"event": "before", "tool": tool, "args": args or {},
                          "sessionID": "s1", "reply": reply})[0]

    def _after(self, tool: str, reply: str, output: str = "ran") -> dict:
        return self._run({"event": "after", "tool": tool, "args": {}, "sessionID": "s1",
                          "reply": reply, "output": output})[0]

    # -- PreToolUse -------------------------------------------------------------

    def test_every_routed_tool_invokes_the_handler_its_table_entry_names(self):
        """The assertion #433 needed. Not `PRE_HOOKS["read"] == "pretooluse-read"` — that
        was true while the call site ignored it — but: run `read`, and see
        `charter hook pretooluse-read` on the command line."""
        for tool, handler in opencode.PRE_HOOKS.items():
            with self.subTest(tool=tool):
                res = self._before(tool, {"filePath": "notes.md"})
                self.assertEqual([_hook_of(c["command"]) for c in res["calls"]], [handler])

    def test_the_vault_read_guard_runs_on_a_read_of_a_vault(self):
        """Named alone so a failure says which invariant went missing. `pretooluse` guards
        Bash by reading `tool_input.command` and never looks at `tool_name`, so a `read`
        sent there reaches no guard at all."""
        res = self._before("read", {"filePath": ".charter/vaults/devops.json"})
        self.assertEqual([_hook_of(c["command"]) for c in res["calls"]],
                         ["pretooluse-read"])

    def test_the_guard_is_handed_the_argument_name_opencode_actually_uses(self):
        """Routing alone would have shipped a guard that ran and decided nothing: opencode's
        `read` takes `filePath`, and `pretooluse_read` used to read `file_path` only."""
        res = self._before("read", {"filePath": ".charter/vaults/devops.json"})
        self.assertEqual(res["calls"][0]["payload"]["tool_input"],
                         {"filePath": ".charter/vaults/devops.json"})
        self.assertEqual(res["calls"][0]["payload"]["tool_name"], "Read")

    def test_a_tool_with_no_entry_falls_to_the_catch_all_and_not_to_silence(self):
        for tool in _UNROUTED_PRE:
            with self.subTest(tool=tool):
                res = self._before(tool, {"command": "ls"})
                self.assertEqual([_hook_of(c["command"]) for c in res["calls"]],
                                 [opencode.DEFAULT_PRE_HOOK])

    def test_no_tool_reaches_two_guards(self):
        """One call per tool call. A second spawn would double every tally and ask twice."""
        for tool in list(opencode.PRE_HOOKS) + list(_UNROUTED_PRE):
            with self.subTest(tool=tool):
                self.assertEqual(len(self._before(tool, {"command": "ls"})["calls"]), 1)

    def test_the_payload_goes_in_on_stdin(self):
        """`< ${blob}` is how Bun's shell takes stdin; a `.stdin()` METHOD does not exist
        and calling one threw on every tool call while everything looked wired."""
        res = self._before("read", {"filePath": "notes.md"})
        self.assertIn("<stdin>", res["calls"][0]["command"])
        self.assertEqual(res["calls"][0]["payload"]["hook_event_name"], "PreToolUse")

    def test_a_deny_stops_the_tool_and_carries_charters_reason(self):
        res = self._before("read", {"filePath": ".charter/vaults/devops.json"},
                           reply='{"hookSpecificOutput":{"permissionDecision":"deny",'
                                 '"permissionDecisionReason":"charter guard: vault"}}')
        self.assertEqual(res["threw"], "charter guard: vault")

    def test_a_guard_that_cannot_run_fails_open(self):
        """An unwired plane must not make the harness unusable; `doctor` is what reports
        one. Silence and unreadable output both allow."""
        for reply in ("", "not json at all"):
            with self.subTest(reply=reply):
                self.assertIsNone(self._before("read", {"filePath": "x"}, reply)["threw"])

    # -- PostToolUse ------------------------------------------------------------

    def test_every_routed_tool_invokes_the_post_handler_its_table_entry_names(self):
        for tool, handler in opencode.POST_HOOKS.items():
            with self.subTest(tool=tool):
                res = self._after(tool, _ALLOW)
                self.assertEqual([_hook_of(c["command"]) for c in res["calls"]], [handler])

    def test_a_tool_with_no_post_entry_spawns_nothing(self):
        """No catch-all after the fact: there is nothing safe to run for a tool nobody
        wrote a handler for, and every one of these spawns a process."""
        for tool in _UNROUTED_POST:
            with self.subTest(tool=tool):
                self.assertEqual(self._after(tool, _ALLOW)["calls"], [])

    def test_running_a_post_handler_and_appending_its_answer_are_two_questions(self):
        """`skill` and `task` get their handler run — that is how a tally is recorded —
        without their result being rewritten. Conflating the two gates is why neither was
        ever tallied on this harness."""
        note = '{"hookSpecificOutput":{"additionalContext":"charter says so"}}'
        for tool in opencode.POST_HOOKS:
            with self.subTest(tool=tool):
                res = self._after(tool, note, output="ran")
                self.assertEqual(len(res["calls"]), 1)
                carried = "charter says so" in res["output"]
                self.assertEqual(carried, tool in opencode.EFFECTFUL_TOOLS)
                if carried:
                    self.assertIn("--- charter ---", res["output"])
                    self.assertTrue(res["output"].startswith("ran"))

    # -- the shell every tool spawns --------------------------------------------

    def test_the_harness_name_reaches_every_shell_and_every_hook_process(self):
        """`harness.current()` reads `$CHARTER_HARNESS`. It has to be on the shells
        opencode spawns AND on the hook charter itself runs, or the guard decides as
        though it were on Claude Code."""
        env = self._run({"event": "shellenv", "sessionID": "sess-9"})[0]["env"]
        self.assertEqual(env, {"CHARTER_HARNESS": "opencode",
                               "CHARTER_SESSION_ID": "sess-9"})
        res = self._before("read", {"filePath": "notes.md"})
        self.assertEqual(res["calls"][0]["env"],
                         {"CHARTER_HARNESS": "opencode", "CHARTER_SESSION_ID": "s1"})


if __name__ == "__main__":
    unittest.main()
