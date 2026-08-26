"""The opencode plugin charter generates, and the restraint it is written with.

ADR 0015: there is no marketplace and no published package for opencode — `charter init`
writing this file IS the install. That makes it the one artifact charter ships and never
executes, so what it contains is asserted here rather than trusted.
"""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from charter.harness import opencode


class EnsureShim(unittest.TestCase):
    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-oc-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.root, True))
        self.shim = self.root / opencode.SHIM_PATH

    def test_a_plane_with_no_opencode_dir_gets_the_plugin_written(self):
        self.assertEqual(opencode.ensure_shim(self.root), "created")
        self.assertTrue(self.shim.is_file())

    def test_a_second_call_leaves_an_edited_plugin_alone(self):
        """IF ABSENT, never repair — the same restraint `_load_settings` keeps for
        `.claude/settings.json`. Someone who edited the shim gets to keep their edit; a
        writer that silently reverts a deliberate change is the thing this refuses."""
        opencode.ensure_shim(self.root)
        self.shim.write_text("// mine now\n")
        self.assertEqual(opencode.ensure_shim(self.root), "present")
        self.assertEqual(self.shim.read_text(), "// mine now\n")

    def test_the_plugin_names_the_harness_through_shell_env(self):
        """The shim's whole job in this piece: every shell opencode spawns answers
        `harness.current()` with "opencode"."""
        opencode.ensure_shim(self.root)
        src = self.shim.read_text()
        self.assertIn('"shell.env"', src)
        self.assertIn("CHARTER_HARNESS", src)
        self.assertIn("opencode", src)

    def test_the_plugin_passes_the_session_id_through_per_invocation(self):
        """opencode 1.18.18 hands `shell.env` ``{cwd, sessionID, callID}`` — checked
        against the binary, because the published example shows `cwd` alone. Read from
        `input` on every call rather than cached in a module variable: one opencode server
        hosts many sessions, so a module-level "current session" has no correct value."""
        opencode.ensure_shim(self.root)
        src = self.shim.read_text()
        self.assertIn("CHARTER_SESSION_ID", src)
        self.assertIn("input.sessionID", src)


class GuardForwarding(unittest.TestCase):
    """The shim turns an opencode tool call into the hook payload charter already reads.

    Verified against opencode 1.18.18 by reading the bundled call site in the binary:
    `trigger("tool.execute.before", {tool, sessionID, callID}, {args})` is awaited BEFORE
    `u.execute(...)`, and `Plugin.trigger` wraps each hook in `Effect.promise` with no
    try/catch — so a throw prevents the tool from running.
    """

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-oc-g-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.root, True))
        opencode.ensure_shim(self.root)
        self.src = (self.root / opencode.SHIM_PATH).read_text()

    def test_it_hooks_the_event_that_runs_before_the_tool(self):
        self.assertIn('"tool.execute.before"', self.src)

    def test_opencode_tool_ids_are_mapped_to_the_names_charter_guards(self):
        """charter's guard matches `tool_name == "Bash"`; opencode calls it `bash`.
        The mapping lives in Python and is generated into the shim, so there is one
        source of truth rather than a table in each language that can drift."""
        self.assertEqual(opencode.TOOL_NAMES["bash"], "Bash")
        for oc_id, charter_name in opencode.TOOL_NAMES.items():
            with self.subTest(tool=oc_id):
                # The PAIR, as `PRE_HOOKS` below is already pinned — not the two halves
                # separately, which is what shipped and what `"write": "write"` satisfied
                # twice over. That one-word mutation passed the whole suite while
                # `posttooluse` bailed at `tool_name not in ("Write","Edit","MultiEdit")`
                # and the committed-secret scan stopped firing on this harness.
                self.assertIn(f'"{oc_id}": "{charter_name}"', self.src)
                # …and translating has to CHANGE the name. opencode's ids are lowercase
                # and charter's guards match Claude Code's CamelCase, so an entry that
                # returns its own key has translated nothing — it only looks like a table.
                self.assertNotEqual(charter_name, oc_id)
        # …and the table is READ, at both call sites. The same gap as #433 one field over:
        # a correct map that nothing indexes leaves `tool_name` as opencode's own id, and
        # every guard that matches on the Claude Code name silently stops matching.
        self.assertEqual(self.src.count("own(TOOL_NAMES, tool) ?? tool"), 2)

    def test_it_sends_the_payload_charter_reads_and_throws_on_deny(self):
        for token in ("hook_event_name", "PreToolUse", "session_id", "tool_name",
                      "tool_input", "charter hook ${hook}", "permissionDecision",
                      "throw"):
            with self.subTest(token=token):
                self.assertIn(token, self.src)

    def test_the_handler_is_chosen_by_TOOL_not_fixed_at_pretooluse(self):
        """This assertion used to read ``charter hook pretooluse`` — the literal — and it
        was the bug (#433) rather than the contract. `pretooluse` guards Bash by reading
        `tool_input.command` and never looks at `tool_name`, so sending opencode's `read`
        there meant the vault-read guard did not exist on this harness, while the Bash
        denial went on naming the vault path it had just refused.

        `pretooluse` is still in the shim — as the CATCH-ALL, which is the honest place
        for it. What the shim must not do again is send every tool there."""
        self.assertIn('const DEFAULT_PRE_HOOK = "pretooluse"', self.src)
        self.assertEqual(opencode.PRE_HOOKS["read"], "pretooluse-read")
        for tool, handler in opencode.PRE_HOOKS.items():
            with self.subTest(tool=tool):
                self.assertIn(f'"{tool}": "{handler}"', self.src)

    def test_the_call_site_reads_the_table_and_not_a_handler_it_spells_itself(self):
        """The table being right is not the fix. Every assertion above is satisfied by a
        shim whose call site says ``charter hook pretooluse`` with the table sitting three
        lines up, unread — which is the state that shipped for four releases, and which
        round one's fix was still mutable back into with the whole suite green.

        So pin the CALL SITE. `tests/test_opencode_shim_dispatches_at_runtime.py` pins the
        same thing by running it; this one holds when no JS runtime is installed."""
        self.assertEqual(self.src.count("charter hook ${hook}"), 2)
        self.assertIn("const hook = own(PRE_HOOKS, tool) ?? DEFAULT_PRE_HOOK", self.src)
        for handler in {*opencode.PRE_HOOKS.values(), *opencode.POST_HOOKS.values(),
                        opencode.DEFAULT_PRE_HOOK}:
            with self.subTest(handler=handler):
                self.assertNotIn(f"charter hook {handler} ", self.src)

    def test_no_table_is_indexed_by_a_tool_id_directly(self):
        """`TABLE[input?.tool]` does not ask the table, it asks the prototype chain.

        `PRE_HOOKS["constructor"]` is `Object` — a function, so `??` never fires — and the
        shim spawned `charter hook function Object() { [native code] }`, charter exited
        non-zero, the shim took its fail-open path, and the tool reached no guard at all.
        `POST_HOOKS["toString"]` walked past `if (!hook) return` the same way.

        `own()` asks the property instead: OWN key, string value. Pinned here as an
        absence because a fourth table added tomorrow must not reintroduce the class;
        `tests/test_opencode_shim_dispatches_at_runtime.py` proves the behaviour by
        running every name the runtime's own `Object.prototype` carries.
        """
        self.assertIn("Object.hasOwn(table, key)", self.src)
        # Comment lines dropped first: the shim explains this bug by quoting the spelling
        # it no longer uses, and an absence check that read the prose would be satisfied
        # by deleting the explanation instead of by keeping the fix.
        code = "\n".join(ln for ln in self.src.splitlines()
                         if not ln.lstrip().startswith("//"))
        for table in ("TOOL_NAMES", "PRE_HOOKS", "POST_HOOKS"):
            with self.subTest(table=table):
                self.assertNotIn(f"{table}[", code)


class MidSessionNudges(unittest.TestCase):
    """charter's governance text reaches an opencode session by riding a tool result.

    On Claude Code it arrives BESIDE the result, as `PostToolUse.additionalContext`.
    opencode has no such channel, so the shim appends to the result itself — verified
    possible by reading the binary: `trigger` hands hooks the result object and returns
    it, so a mutation propagates.
    """

    def setUp(self) -> None:
        self.root = Path(tempfile.mkdtemp(prefix="charter-oc-n-"))
        self.addCleanup(lambda: __import__("shutil").rmtree(self.root, True))
        opencode.ensure_shim(self.root)
        self.src = (self.root / opencode.SHIM_PATH).read_text()

    def test_it_hooks_the_event_that_runs_after_the_tool(self):
        """Same move as `test_the_handler_is_chosen_by_TOOL_not_fixed_at_pretooluse`: the
        literal `charter hook posttooluse` was one harness's answer for every tool, so
        `posttooluse-bash`, `posttooluse-skill` and `posttooluse-dispatch` — three of the
        five the manifest dispatches — never ran here."""
        self.assertIn('"tool.execute.after"', self.src)
        self.assertIn("charter hook ${hook}", self.src)
        self.assertIn("const hook = own(POST_HOOKS, tool)", self.src)
        for tool, handler in opencode.POST_HOOKS.items():
            with self.subTest(tool=tool):
                self.assertIn(f'"{tool}": "{handler}"', self.src)

    def test_only_effectful_tools_carry_it(self):
        """A `read` whose output has charter's text appended is a false record of that
        file — and the agent may write it back. Tools that RETURN CONTENT are never
        touched; only the ones whose output is a report of an action."""
        self.assertEqual(opencode.EFFECTFUL_TOOLS, ("bash", "edit", "write"))
        for tool in opencode.EFFECTFUL_TOOLS:
            with self.subTest(tool=tool):
                self.assertIn(f'"{tool}"', self.src)
        self.assertIn("EFFECTFUL", self.src)

    def test_running_a_post_handler_and_appending_its_answer_are_two_questions(self):
        """`skill` and `task` now get their handler run — that is how a tally is recorded —
        without their result being rewritten. Conflating the two gates is why neither was
        ever tallied on this harness."""
        self.assertIn("skill", opencode.POST_HOOKS)
        self.assertIn("task", opencode.POST_HOOKS)
        self.assertNotIn("skill", opencode.EFFECTFUL_TOOLS)
        self.assertNotIn("task", opencode.EFFECTFUL_TOOLS)
        self.assertIn("if (!note || !EFFECTFUL.includes(tool)) return", self.src)

    def test_the_appended_text_is_fenced_so_it_cannot_be_mistaken_for_output(self):
        self.assertIn("charter", self.src.lower())
        self.assertIn("additionalContext", self.src)


if __name__ == "__main__":
    unittest.main()
