"""Every handler charter ships has to be REACHABLE on opencode, not merely to exist (#433).

`hooks/hooks.json` is the manifest for Claude Code, and Codex installs that same plugin.
opencode has no manifest — the routing lives in the plugin charter generates — and for four
releases it had no routing at all: `tool.execute.before` forwarded every tool to `charter
hook pretooluse`, which guards Bash by reading ``tool_input["command"]`` and never looks at
`tool_name`.

So the vault-read guard was ABSENT on that harness, while the Bash denial went on naming the
vault path it had just refused — #90 verbatim, one harness over. `charter harness list` did
not print it either, because `deficits` never claimed it.

`tests/test_vault_read_guard.py::TestItIsActuallyWired` is the test that should have caught
it and could not: it reads `hooks/hooks.json`, which is one harness's answer. This one reads
the OTHER harness's wiring — the generated shim — and diffs the two, so a handler added to
`hooks/hooks.json` tomorrow fails here until opencode routes it or a reason is written down
for why it cannot.

Reads it from the CALL SITES, not from handler-shaped strings anywhere in the file. The
first version of `_shim_routes` did the latter and an adversarial review broke it in one
line: spell `charter hook pretooluse` at the call site, leave `PRE_HOOKS` untouched three
lines above, and the suite stayed green while the shim was back to #433 exactly. A table
nothing dispatches through is a table that does not run, so it does not count here.
`tests/test_opencode_shim_dispatches_at_runtime.py` asks the same question by executing the
shim, which is the only way to ask it that does not involve reading JavaScript with regexes.

The opencode tool schemas quoted below were read off opencode 1.18.21 itself — the running
server's `GET /experimental/tool` — rather than off its docs, because the argument NAMES are
the half that bit: opencode's `read` takes `filePath`, not `file_path`.
"""

from __future__ import annotations

import json
import re
import unittest
from pathlib import Path

from charter import config, hooks
from charter.harness import opencode
from tests._isolation import PersonaIso, PlaneIso, run_hook

ROOT = Path(__file__).resolve().parents[1]

#: What `hooks/hooks.json` dispatches for each PreToolUse/PostToolUse matcher, transcribed
#: as the pair `(claude-code tool name, handler)`. Not read from the file: this is the
#: EXPECTATION the diff below compares the file against, so it has to be written by hand.
#: `_manifest_routes` reads the file, and the two disagreeing is the test.
_TOOLS_BY_HARNESS = {
    # opencode tool id -> the Claude Code tool it is the same tool as
    "read": "Read",
    "grep": "Grep",
    "write": "Write",
    "edit": "Edit",
    "task": "Task",
    "bash": "Bash",
    "skill": "Skill",
}

#: Handlers `hooks/hooks.json` dispatches that opencode genuinely cannot reach, each with
#: the reason. An excuse is a claim, so each one names what was checked.
_UNREACHABLE = {
    # `charter hook sessionstart` has no opencode counterpart: there is no SessionStart
    # event. charter substitutes a generated context file named in `instructions`
    # (`opencode.CONTEXT_PATH`), which is why this is not a deficit either.
    "sessionstart": "no SessionStart event; charter writes CONTEXT_PATH instead",
    # The declared `prompt-hook` deficit, in code.
    "userpromptsubmit": "no per-turn prompt hook — declared as the `prompt-hook` deficit",
    # opencode has no agent-messaging tool at all. Its full tool id list at 1.18.21 is
    # invalid, question, bash, read, glob, grep, edit, write, task, webfetch, todowrite,
    # skill — no `SendMessage`, so there is nothing to dispatch on.
    "posttooluse-message": "opencode 1.18.21 has no SendMessage tool",
}


def _manifest_routes() -> set[str]:
    """Every `charter hook <name>` the plugin manifest dispatches."""
    doc = json.loads((ROOT / "hooks" / "hooks.json").read_text())["hooks"]
    out = set()
    for entries in doc.values():
        for e in entries:
            for h in e["hooks"]:
                c = h["command"]
                if "charter hook " in c:
                    out.add(c.split("charter hook ")[1].split()[0])
    return out


#: The two blocks in the shim from which a handler can actually be spawned, in the order
#: they appear. A `charter hook` outside both is not reachable from a tool call.
_DISPATCH_BLOCKS = ('"tool.execute.before"', '"tool.execute.after"')


def _shim_const(name: str) -> object | None:
    """The value of a top-level ``const NAME = …`` in the shim, or ``None``.

    Everything the template interpolates is rendered by `json.dumps`, so the text that
    follows the ``=`` is JSON and `raw_decode` reads exactly it.
    """
    m = re.search(rf"^const {re.escape(name)} = ", opencode.SHIM, re.M)
    if m is None:
        return None
    try:
        value, _ = json.JSONDecoder().raw_decode(opencode.SHIM, m.end())
    except ValueError:
        return None
    return value


def _handler_names(value: object) -> set[str]:
    if isinstance(value, str):
        return {value}
    if isinstance(value, dict):
        return {v for v in value.values() if isinstance(v, str)}
    if isinstance(value, list):
        return {v for v in value if isinstance(v, str)}
    return set()


def _shim_routes() -> set[str]:
    """Every handler the GENERATED shim can actually spawn, derived from its CALL SITES.

    The first version of this read handler-shaped strings out of the whole file, and that
    is the bug one level up: #433 shipped for four releases with the routing right there
    and no call site reading it, and round one's fix was still mutable back to a literal
    ``charter hook pretooluse`` at either call site with the entire suite green. A table
    nothing dispatches through is a table that does not run.

    So: find each ``charter hook`` inside `tool.execute.before` / `tool.execute.after`,
    take the expression it interpolates, and resolve it through the constants that
    expression NAMES. A dead table resolves to nothing and stops satisfying anything; a
    literal call site contributes that one literal and nothing else.
    """
    src = opencode.SHIM
    bounds = [src.index(b) for b in _DISPATCH_BLOCKS] + [len(src)]
    named: set[str] = set()
    for start, end in zip(bounds, bounds[1:]):
        block = src[start:end]
        for m in re.finditer(r"charter hook (\$\{(\w+)\}|[a-z][\w-]*)", block):
            if m.group(2) is None:  # spelled out at the call site
                named.add(m.group(1))
                continue
            # An interpolated variable: whatever that name is bound to IN THIS BLOCK.
            bind = re.search(rf"\b(?:const|let|var) {m.group(2)} = (.+)", block)
            if bind is None:
                continue
            expr = bind.group(1)
            if expr.startswith('"'):  # bound to a literal, not to a table
                named |= _handler_names(json.JSONDecoder().raw_decode(expr)[0])
            for ident in re.findall(r"\b[A-Z][A-Z0-9_]*\b", expr):
                named |= _handler_names(_shim_const(ident))
    return named


class TheGeneratedShimReachesEveryHandler(unittest.TestCase):
    def test_every_handler_the_manifest_dispatches_is_reachable_from_opencode(self):
        """The whole issue, as one assertion.

        Anything genuinely unreachable belongs in `_UNREACHABLE` with the reason — which is
        a decision someone has to write down, rather than a gap that simply is."""
        missing = _manifest_routes() - _shim_routes() - set(_UNREACHABLE)
        self.assertEqual(missing, set(),
                         f"opencode never dispatches {sorted(missing)} — the guard exists "
                         f"and does not run on that harness")

    def test_the_vault_read_guard_specifically(self):
        """Named on its own so a failure says which invariant went missing, not just that
        a set differs. This is the one #433 is about."""
        self.assertIn("pretooluse-read", _shim_routes())

    def test_every_excuse_is_still_a_handler_charter_ships(self):
        """An excuse for a handler that no longer exists is dead weight that hides the next
        one. Fails the day a name in `_UNREACHABLE` is renamed or deleted."""
        self.assertLessEqual(set(_UNREACHABLE), set(hooks._HANDLERS))

    def test_nothing_is_routed_to_a_handler_the_engine_does_not_have(self):
        """The other direction: a typo in the routing table would spawn `charter hook
        pretooluse-raed` on every read, which exits 1 and — because the shim fails OPEN —
        looks exactly like the guard passing."""
        self.assertLessEqual(_shim_routes(), set(hooks._HANDLERS))

    def test_the_routing_tables_only_name_handlers_that_exist(self):
        for table in (opencode.PRE_HOOKS, opencode.POST_HOOKS):
            for tool, handler in table.items():
                with self.subTest(tool=tool):
                    self.assertIn(handler, hooks._HANDLERS)
        self.assertIn(opencode.DEFAULT_PRE_HOOK, hooks._HANDLERS)


class ItRoutesTheSameWayTheManifestDoes(unittest.TestCase):
    """A tool must reach the handler `hooks/hooks.json` names for its Claude Code twin.

    Reading the manifest rather than restating it: the two are one decision and the point
    of this file is that they stop being able to drift apart quietly.
    """

    def _manifest_handler(self, event: str, tool: str) -> str | None:
        doc = json.loads((ROOT / "hooks" / "hooks.json").read_text())["hooks"]
        for e in doc[event]:
            if tool in (e.get("matcher") or "").split("|"):
                c = e["hooks"][0]["command"]
                return c.split("charter hook ")[1].split()[0]
        return None

    def test_pretooluse_routing_matches_the_manifest(self):
        for oc_id, cc_name in _TOOLS_BY_HARNESS.items():
            want = self._manifest_handler("PreToolUse", cc_name)
            got = opencode.PRE_HOOKS.get(oc_id, opencode.DEFAULT_PRE_HOOK)
            with self.subTest(tool=oc_id):
                # A tool the manifest registers no PreToolUse matcher for (`skill`) is
                # allowed to fall to the catch-all; it must never reach a DIFFERENT
                # handler than the one Claude Code uses.
                self.assertEqual(got, want or opencode.DEFAULT_PRE_HOOK)

    def test_posttooluse_routing_matches_the_manifest(self):
        for oc_id, cc_name in _TOOLS_BY_HARNESS.items():
            want = self._manifest_handler("PostToolUse", cc_name)
            with self.subTest(tool=oc_id):
                self.assertEqual(opencode.POST_HOOKS.get(oc_id), want)

    def _manifest_names(self, event: str, handler: str) -> set[str]:
        """Every Claude Code tool name `hooks/hooks.json` registers against *handler*."""
        doc = json.loads((ROOT / "hooks" / "hooks.json").read_text())["hooks"]
        names: set[str] = set()
        for e in doc[event]:
            for h in e["hooks"]:
                c = h["command"]
                if ("charter hook " in c
                        and c.split("charter hook ")[1].split()[0] == handler):
                    names |= set((e.get("matcher") or "").split("|")) - {""}
        return names

    def test_a_routed_tool_arrives_under_a_name_that_handler_answers_to(self):
        """`TOOL_NAMES`, `PRE_HOOKS` and `POST_HOOKS` are three tables, and all three
        agreeing with each other is not the same as agreeing with the handler.

        Every one of these handlers opens by testing `tool_name` and returning 0 when it
        does not recognise it. So a routed tool whose translated name that handler is not
        registered for produces a hook that runs, decides nothing, and looks wired — #433's
        exact shape, reachable without touching the routing at all. `TOOL_NAMES["write"] =
        "write"` is the one-word version a review shipped past the whole suite; adding a
        tool to `POST_HOOKS` under a handler whose matcher does not name it is the version
        nobody has written yet.

        Read out of the manifest rather than restated, so the two harnesses cannot drift.
        Tools with no entry are exempt and deliberately so: they fall to
        :data:`DEFAULT_PRE_HOOK`, which guards Bash by reading ``tool_input["command"]``
        and never looks at `tool_name` at all.
        """
        for event, table in (("PreToolUse", opencode.PRE_HOOKS),
                             ("PostToolUse", opencode.POST_HOOKS)):
            for oc_id, handler in table.items():
                with self.subTest(event=event, tool=oc_id):
                    self.assertIn(opencode.TOOL_NAMES[oc_id],
                                  self._manifest_names(event, handler))

    def test_charters_tool_names_cover_every_tool_that_is_routed(self):
        """A routed tool whose id charter never translates arrives with `tool_name` set to
        opencode's lowercase id, and every handler's `tool_name` test then fails — a hook
        that runs and decides nothing, which is the shape of this whole issue."""
        for tool in set(opencode.PRE_HOOKS) | set(opencode.POST_HOOKS):
            with self.subTest(tool=tool):
                self.assertIn(tool, opencode.TOOL_NAMES)


#: ``opencode tool id -> (its parameter names)``, opencode 1.18.21, read off the running
#: server's `GET /experimental/tool`. Quoted here because the ARGUMENT NAMES are the half
#: that bit: routing `read` to `pretooluse_read` fixes nothing while the handler looks for
#: `file_path` and opencode sends `filePath`.
_OPENCODE_ARGS = {
    "bash": ("command", "timeout", "workdir"),
    "read": ("filePath", "offset", "limit"),
    "grep": ("pattern", "path", "include"),
    "edit": ("filePath", "oldString", "newString", "replaceAll"),
    "write": ("content", "filePath"),
    "task": ("description", "prompt", "subagent_type", "task_id", "command"),
    "skill": ("name",),
}


class TheHandlersReadTheArgumentNamesOpencodeSENDS(PlaneIso):
    """Routing is half the fix; reading the right KEY is the other half.

    Every case here drives the REAL handler with the payload the shim will really build —
    opencode's tool id translated through `TOOL_NAMES`, and opencode's own argument names
    verbatim. Asserting against `_OPENCODE_ARGS` alone would be a fixture agreeing with
    itself; these assert on what the handler DID.
    """

    def _payload(self, tool: str, args: dict) -> dict:
        """Exactly what `_SHIM_TEMPLATE` puts on stdin for opencode's *tool*."""
        for k in args:
            self.assertIn(k, _OPENCODE_ARGS[tool],
                          f"opencode's `{tool}` has no argument `{k}`")
        return {"hook_event_name": "PreToolUse", "session_id": "s", "cwd": str(self.tmp),
                "tool_name": opencode.TOOL_NAMES[tool], "tool_input": args}

    def test_a_read_of_a_vault_is_denied_with_opencodes_own_key(self):
        r = run_hook(hooks.pretooluse_read,
                     self._payload("read", {"filePath": ".charter/vaults/devops.json"}))
        self.assertEqual(
            (r or {}).get("hookSpecificOutput", {}).get("permissionDecision"), "deny")

    def test_a_grep_into_the_vault_directory_is_denied(self):
        r = run_hook(hooks.pretooluse_read,
                     self._payload("grep", {"path": ".charter/vaults", "pattern": "token"}))
        self.assertEqual(
            (r or {}).get("hookSpecificOutput", {}).get("permissionDecision"), "deny")

    def test_a_secret_written_into_committed_memory_is_still_caught(self):
        """`posttooluse` read only `file_path`, so on opencode every branch below its path
        lookup — the secret scan included — was a no-op that looked wired.

        `tool_name` comes from `TOOL_NAMES`, never spelled here. It used to read `"Write"`
        literally, and that literal is why `TOOL_NAMES["write"] = "write"` — a case change
        — passed the whole suite: the one test that drove this handler agreed with itself
        about the name while the shim had stopped sending it. Both tools that route here,
        because `write` and `edit` are two independent one-word mutations.
        """
        for tool in ("write", "edit"):
            with self.subTest(tool=tool):
                name = self.make_persona(f"scanned-{tool}")
                p = config.PERSONAS_DIR / name / "memory" / "leak.md"
                p.parent.mkdir(parents=True, exist_ok=True)
                # Not a real credential: the prefix and the length are all the scanner
                # matches on, and this string authenticates to nothing.
                body = "token: ghp_" + "a" * 36
                p.write_text(body)
                r = run_hook(hooks.posttooluse,
                             {"hook_event_name": "PostToolUse", "session_id": "s",
                              "cwd": str(self.tmp),
                              "tool_name": opencode.TOOL_NAMES[tool],
                              "tool_input": {"filePath": str(p), "content": body}})
                note = (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")
                self.assertIn("SECURITY", note)

    def test_a_skill_invocation_is_tallied_under_opencodes_own_key(self):
        from charter import skilluse

        name = self.make_persona("tallied")
        config.ACTIVE_PERSONA_FILE.parent.mkdir(parents=True, exist_ok=True)
        config.ACTIVE_PERSONA_FILE.write_text(name)
        run_hook(hooks.posttooluse_skill,
                 {"hook_event_name": "PostToolUse", "session_id": "s", "cwd": str(self.tmp),
                  "tool_name": opencode.TOOL_NAMES["skill"],
                  "tool_input": {"name": "browser"}})
        self.assertEqual(skilluse.by_persona(name).get("browser"), 1)

    def test_claude_codes_spelling_still_works(self):
        """The camelCase keys are ADDITIVE. A fix that moved the guard from one harness's
        spelling to the other's would be the same bug facing the other way."""
        r = run_hook(hooks.pretooluse_read,
                     {"tool_name": "Read", "session_id": "s", "cwd": str(self.tmp),
                      "tool_input": {"file_path": ".charter/vaults/devops.json"}})
        self.assertEqual(
            (r or {}).get("hookSpecificOutput", {}).get("permissionDecision"), "deny")

    def test_the_dispatch_tally_reads_the_key_opencodes_task_carries(self):
        from charter import dispatch

        name = self.make_persona("dispatched")
        run_hook(hooks.posttooluse_dispatch,
                 {"hook_event_name": "PostToolUse", "session_id": "s", "cwd": str(self.tmp),
                  "tool_name": opencode.TOOL_NAMES["task"],
                  "tool_input": {"subagent_type": name, "prompt": "go", "description": "d"},
                  "tool_response": ""})
        self.assertEqual(dispatch.tally().get(name), 1)


if __name__ == "__main__":
    unittest.main()
