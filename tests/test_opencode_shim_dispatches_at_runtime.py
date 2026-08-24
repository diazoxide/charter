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

    def _before(self, tool, args: dict | None = None, reply: str = _ALLOW,
                session: str = "s1", directory: str = "/plane") -> dict:
        return self._run({"event": "before", "tool": tool, "args": args or {},
                          "sessionID": session, "directory": directory,
                          "reply": reply})[0]

    def _after(self, tool, reply: str, output: str = "ran", args: dict | None = None,
               session: str = "s1", directory: str = "/plane") -> dict:
        return self._run({"event": "after", "tool": tool, "args": args or {},
                          "sessionID": session, "directory": directory,
                          "reply": reply, "output": output})[0]

    def _prototype_keys(self) -> list[str]:
        """Every name a plain ``TABLE[key]`` would resolve through the prototype chain.

        Asked of the RUNTIME rather than written down here. A list in Python is the shape
        this audit keeps finding one spelling short — and `Object.prototype` is a set the
        engine owns, not one charter gets to enumerate correctly forever.
        """
        keys = self._run({"event": "protokeys"})[0]["keys"]
        self.assertIn("constructor", keys)     # a fixture that returned [] would pass
        self.assertIn("__proto__", keys)       # every assertion below unfailably
        return keys

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

    def test_the_before_payload_carries_every_field_and_nothing_else(self):
        """An EQUALITY, deliberately, and against values this test chose.

        The previous round asserted two of the five fields, and a review blanked the rest
        one at a time with the whole suite green: `cwd: ""` is what the containment rule
        reads, `session_id: ""` is what every workspace resolution and every trace record
        keys on. A per-field presence check is a list of fields, which is the shape that
        goes one entry short; comparing the whole dict makes "a field the call site
        dropped" and "a field the call site invented" the same failure.
        """
        res = self._before("read", {"filePath": "notes.md"},
                           session="sess-7", directory="/some/plane")
        self.assertEqual(res["calls"][0]["payload"], {
            "hook_event_name": "PreToolUse",
            "session_id": "sess-7",
            "cwd": "/some/plane",
            "tool_name": "Read",
            "tool_input": {"filePath": "notes.md"},
        })

    def test_the_after_payload_carries_every_field_and_nothing_else(self):
        """The twin of the block above, which did not exist and is why `tool_input: {}` on
        the after-block passed the entire suite while killing the secret scan on this
        harness: `posttooluse` reads the path out of `tool_input` and returns immediately
        when it finds none."""
        res = self._after("write", _ALLOW, output="wrote 1 line",
                          args={"filePath": "notes.md", "content": "hi"},
                          session="sess-7", directory="/some/plane")
        self.assertEqual(res["calls"][0]["payload"], {
            "hook_event_name": "PostToolUse",
            "session_id": "sess-7",
            "cwd": "/some/plane",
            "tool_name": "Write",
            "tool_input": {"filePath": "notes.md", "content": "hi"},
            "tool_response": {"output": "wrote 1 line"},
        })

    def test_every_id_the_table_translates_arrives_as_the_name_it_translates_to(self):
        """`TOOL_NAMES` being right is not the same as the shim sending what it says.

        Two failures at once, both green on the full suite before this: a value equal to
        its key (`"write": "write"`) satisfied every assertion that looked for the two
        halves separately, and a key spelled `__proto__` would be swallowed by the object
        literal rather than stored. Running the shim asks the only question that matters —
        send this id, read back the name — and neither survives it.
        """
        for oc_id, charter_name in opencode.TOOL_NAMES.items():
            with self.subTest(tool=oc_id):
                res = self._before(oc_id, {})
                self.assertEqual(res["calls"][0]["payload"]["tool_name"], charter_name)
                self.assertNotEqual(charter_name, oc_id)

    def test_a_tool_id_that_names_an_inherited_property_is_not_a_table_entry(self):
        """`PRE_HOOKS["constructor"]` used to be `Object` — a function, so `??` never fired,
        the command line became `charter hook function Object() { [native code] }`, charter
        exited non-zero and this shim failed OPEN. The tool then reached no guard at all,
        not even the Bash catch-all, and `tool_name` vanished from the payload entirely
        because `JSON.stringify` drops function values.

        The ids come from the runtime's own `Object.prototype`, so this covers the class
        rather than the three names that happened to be tried.
        """
        for key in self._prototype_keys():
            with self.subTest(tool=key):
                res = self._before(key, {"command": "ls"})
                self.assertEqual([_hook_of(c["command"]) for c in res["calls"]],
                                 [opencode.DEFAULT_PRE_HOOK])
                # Forwarded under its own id, as any unknown tool is — and as a STRING, so
                # the handler's `tool_name` test is a comparison and not a coercion.
                self.assertEqual(res["calls"][0]["payload"]["tool_name"], key)

    def test_an_inherited_property_name_spawns_nothing_after_the_fact(self):
        """The after-block has no catch-all on purpose, and `POST_HOOKS["toString"]` walked
        straight past `if (!hook) return` — the single gate that decision rests on."""
        for key in self._prototype_keys():
            with self.subTest(tool=key):
                self.assertEqual(self._after(key, _ALLOW)["calls"], [])

    def test_a_tool_id_that_is_not_a_string_still_reaches_the_catch_all(self):
        """opencode types `tool` as a string; this is about what happens when something
        upstream is wrong rather than about a reachable attack. A number or an object used
        to be interpolated into the command line and into `tool_name` verbatim, which is a
        guard deciding about a value charter cannot compare."""
        for tool in (None, 42, {"toString": "x"}, ["read"]):
            with self.subTest(tool=tool):
                res = self._before(tool, {"command": "ls"})
                self.assertEqual([_hook_of(c["command"]) for c in res["calls"]],
                                 [opencode.DEFAULT_PRE_HOOK])
                self.assertEqual(res["calls"][0]["payload"]["tool_name"], "")
                self.assertEqual(self._after(tool, _ALLOW)["calls"], [])

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


@unittest.skipIf(_RUNTIME is None, "neither bun nor node is installed")
class OnlyAStringEverLeavesATable(unittest.TestCase):
    """`own()` hands back a table's value only when it IS a string, and that half of it
    cannot be reached by anything opencode sends: every value charter generates is one.

    It exists for the other direction — a table that is wrong — and that is not
    hypothetical, it is #433's second half exactly. The prototype walk did not produce a
    bad *decision*; it produced ``charter hook function Object() { [native code] }``,
    a non-zero exit, and a shim that failed open with the tool having reached no guard at
    all. Whatever a lookup returns ends up as a command-line word, so "it is a string"
    is the property, and `Object.hasOwn` is only one of the two ways it can be violated.

    The only honest way to exercise a branch that charter's own generation cannot trigger
    is to generate a table that triggers it. So this rewrites one VALUE in the generated
    source and leaves every line of code alone — deleting either half of `own()` turns
    one of these red, which is the point: a defensive line with no test is the thing this
    audit was called to remove, not something to add more of.
    """

    def setUp(self) -> None:
        self.dir = Path(tempfile.mkdtemp(prefix="charter-oc-nonstr-"))
        self.addCleanup(shutil.rmtree, self.dir, True)
        (self.dir / "drive.mjs").write_text(_DRIVER.read_text())

    def _with_entry(self, entry: str, replacement: str, scenario: dict) -> dict:
        src = opencode.SHIM
        self.assertIn(entry, src)          # a no-op edit would prove nothing
        (self.dir / "charter.mjs").write_text(src.replace(entry, replacement, 1))
        proc = subprocess.run([_RUNTIME, "drive.mjs"], input=json.dumps([scenario]),
                              capture_output=True, text=True, timeout=120, cwd=self.dir)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return json.loads(proc.stdout)[0]

    def test_a_routing_entry_that_is_not_a_string_falls_to_the_catch_all(self):
        """Not to `charter hook [object Object]`, which charter answers non-zero and this
        shim then reads as "guard unavailable" — allowing the tool."""
        res = self._with_entry('"read": "pretooluse-read"',
                               '"read": {"handler": "pretooluse-read"}',
                               {"event": "before", "tool": "read", "args": {},
                                "sessionID": "s1", "reply": _ALLOW})
        self.assertEqual([_hook_of(c["command"]) for c in res["calls"]],
                         [opencode.DEFAULT_PRE_HOOK])

    def test_a_tool_name_entry_that_is_not_a_string_forwards_the_raw_id(self):
        """`tool_name` reaches a Python `in` test. An object there is a guard comparing
        something it cannot compare, which decides nothing and looks wired."""
        res = self._with_entry('"read": "Read"', '"read": ["Read"]',
                               {"event": "before", "tool": "read", "args": {},
                                "sessionID": "s1", "reply": _ALLOW})
        self.assertEqual(res["calls"][0]["payload"]["tool_name"], "read")

    def test_a_post_entry_that_is_not_a_string_spawns_nothing(self):
        """The after-block's `if (!hook) return` is the only gate there is on that side."""
        res = self._with_entry('"write": "posttooluse"', '"write": {}',
                               {"event": "after", "tool": "write", "args": {},
                                "sessionID": "s1", "reply": _ALLOW, "output": "ran"})
        self.assertEqual(res["calls"], [])


if __name__ == "__main__":
    unittest.main()
