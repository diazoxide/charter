"""A guard with no documented override is a guard people uninstall (#370).

A hook `deny` is the strongest thing charter does to a session: no permission mode lifts it,
`charter guard` cannot relax it, and it says so. Every denial named a remedy for the workflow
the operator was *supposed* to be doing, and not one named what to do when the guard is
simply wrong about this case. Nothing else named it either — no config key, no environment
variable, no per-guard switch anywhere in `charter/config.py`.

So the only route past a denial charter got wrong was to delete the hook from
`.claude/settings.json` or disable the plugin, which removes every guard, both injections and
all the tallies together. Nuclear, and undiscoverable at the moment it is needed. Every guard
is eventually wrong about something, and the response a design invites at that moment is the
response it gets: when the only move is nuclear, the guard that was wrong once is off for
ever, along with the ones that were not.

**The ruling this file pins.** The override is that you run the command yourself, outside
the agent — and there is deliberately no switch charter can read. That is not a dodge:

* charter's guards exist because committed data must not reach a credential or make
  something run. A key in `charter.toml` would be a key a committed file could flip, and an
  environment variable sits on a command line the agent writes. An override charter can read
  is an override the AGENT controls, which is exactly the party being bound.
* A `PreToolUse` hook governs the harness's tools. The operator's own shell was never on
  that side of the line, so running it there works around nothing.

**Appended in `_deny`, not at the five call sites**, which is the assertion with the most
value here: a sixth guard added tomorrow carries the override without anyone remembering to,
and the trace tally keys — computed from the reason BEFORE it reaches `_deny` — cannot drift.
"""

from __future__ import annotations

import ast
import re
import unittest
from pathlib import Path

from tests._isolation import PersonaIso, run_hook
from tests.test_hooks import InAControlPlane
from charter import hooks, trace

DOC = Path(__file__).resolve().parents[1] / "docs" / "hooks.md"
SECTION = "## When a guard is wrong"

#: Every denial charter can emit, driven end to end through the real handler.
DENIALS = {
    "secret-leak": (hooks.pretooluse,
                    {"tool_input": {"command": "charter secret get v K --reveal"}}),
    "vault-read": (hooks.pretooluse_read,
                   {"tool_name": "Read", "tool_input": {"file_path": ".charter/vaults/d.json"}}),
    "single-credential": (hooks.pretooluse,
                          {"tool_input": {"command": "git clone git@github.com:a/b.git"}}),
    "plane-root-branch": (hooks.pretooluse,
                          {"tool_input": {"command": "git checkout -b feature"}}),
    "release-floor": (hooks.pretooluse,
                      {"tool_input": {"command": "gh release create v9.9.9"},
                       "permission_mode": "bypassPermissions"}),
}


class DenialCase(InAControlPlane):
    def reason(self, name: str, sid: str = "s") -> str:
        handler, payload = DENIALS[name]
        r = run_hook(handler, {"cwd": str(self.tmp), "session_id": sid, **payload})
        self.assertIsNotNone(r, f"{name}: fixture never reached the guard")
        out = r["hookSpecificOutput"]
        self.assertEqual("deny", out["permissionDecision"],
                         f"{name}: precondition — this fixture must DENY, not {out}")
        return out["permissionDecisionReason"]


class TestEveryDenialNamesTheOverride(DenialCase):
    def test_all_five_of_them(self):
        """The precondition is the count: five guards deny, and all five must say it."""
        self.assertEqual(5, len(DENIALS))
        for name in DENIALS:
            with self.subTest(guard=name):
                self.assertIn(hooks._OVERRIDE_NOTE, self.reason(name))


class TestWhatTheOverrideActuallySays(DenialCase):
    """The wording is the deliverable. A pointer to a section that answers nothing, or a
    hint that reads as "ask charter nicely", would leave the operator exactly where #370
    found them."""

    def setUp(self) -> None:
        super().setUp()
        self.note = hooks._OVERRIDE_NOTE

    def test_it_says_there_is_no_switch(self):
        self.assertRegex(self.note, r"no .*(config|switch)")

    def test_it_says_why_there_is_no_switch(self):
        """Not a refusal — a reason. "A file could flip it" is the whole argument."""
        self.assertIn("committed", self.note)

    def test_it_names_the_move_that_works(self):
        self.assertIn("your own terminal", self.note)

    def test_it_points_at_the_section_that_explains_it(self):
        self.assertIn("docs/hooks.md", self.note)
        self.assertIn("When a guard is wrong", self.note)

    def test_it_does_not_read_as_a_bypass_the_agent_can_take(self):
        """An override an agent can act on is not an override, it is a hole. The note must
        not name a flag, variable or command that would lift the denial."""
        for bait in ("CHARTER_", "--force", "--no-verify", "charter guard allow"):
            self.assertNotIn(bait, self.note)


class TestTheTallyKeysAreUnchanged(DenialCase):
    """The trace `reason` is a tally key, and two guards derive it from the first 70
    characters of their prose. Appending in `_deny` keeps that prefix bit-identical;
    prepending would have silently restarted every series in every existing store."""

    KEYS = {"secret-leak": "would reveal a secret value into the conversation (--reveal)",
            "vault-read": "reads a vault/secret file directly (would print plaintext)",
            "single-credential": "single-credential",
            "plane-root-branch": "plane-root-branch",
            "release-floor": "release-floor"}

    def test_each_guard_still_traces_the_key_it_always_did(self):
        for name, key in self.KEYS.items():
            with self.subTest(guard=name):
                sid = f"s-{name}"
                self.reason(name, sid=sid)
                rows = [e for e in trace.read(sid) if e.get("event") == "deny"]
                self.assertEqual(1, len(rows), f"{name}: precondition — one deny row")
                self.assertTrue(rows[0]["reason"].startswith(key),
                                f"{name}: tally key moved to {rows[0]['reason']!r}")

    def test_no_tally_key_carries_the_override_text(self):
        for name in DENIALS:
            with self.subTest(guard=name):
                sid = f"k-{name}"
                self.reason(name, sid=sid)
                for row in trace.read(sid):
                    self.assertNotIn("your own terminal", row.get("reason", ""))


class TestANewGuardCannotForgetIt(PersonaIso):
    """Structural, because remembering is what fails. Every denial goes through `_deny`,
    so the override rides along with a guard nobody has written yet."""

    @staticmethod
    def _source() -> str:
        return Path(hooks.__file__).read_text()

    @staticmethod
    def _emits_a_denial(fn: ast.FunctionDef) -> bool:
        """An `_emit(...)` call whose literal payload carries `permissionDecision: deny`.
        Deliberately not "the word `deny` appears": `_trace("deny", …)` records a denial
        that `_deny` already emitted, and flagging those would make this test noise."""
        for node in ast.walk(fn):
            if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Name)
                    and node.func.id == "_emit"):
                continue
            for d in ast.walk(node):
                if not isinstance(d, ast.Dict):
                    continue
                for k, v in zip(d.keys, d.values):
                    if (isinstance(k, ast.Constant) and k.value == "permissionDecision"
                            and isinstance(v, ast.Constant) and v.value == "deny"):
                        return True
        return False

    def test_deny_is_the_only_thing_that_emits_a_denial(self):
        tree = ast.parse(self._source())
        fns = [n for n in ast.walk(tree) if isinstance(n, ast.FunctionDef)]
        emitters = [f.name for f in fns if self._emits_a_denial(f)]
        self.assertEqual(["_deny"], emitters,
                         "a denial is emitted outside `_deny`, so it carries no override")

    def test_every_deny_call_passes_prose_and_not_a_prebuilt_message(self):
        """Precondition for the test above: the call sites really do exist and route here."""
        calls = re.findall(r"_deny\(\s*\"PreToolUse\"", self._source())
        self.assertGreaterEqual(len(calls), 5, "precondition: the guards were not found")


class TestTheDocumentationExists(unittest.TestCase):
    """"Undocumented override" and "no override" are the same thing to the person hitting
    one, so the denial's pointer must land somewhere real."""

    def setUp(self) -> None:
        self.text = DOC.read_text()

    def test_hooks_md_has_the_section(self):
        self.assertIn(SECTION, self.text)

    def test_the_section_rules_out_a_config_key_and_says_why(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("charter.toml", body)
        self.assertIn("committed", body)

    def test_the_section_names_the_move_that_works(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("terminal", body)

    def test_the_section_names_the_narrower_moves_that_come_first(self):
        """Two guards have a real, narrower answer. Sending someone to a terminal when
        `--apply` or an attended re-run is the actual fix would be a worse doc than none."""
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("attended", body)
        self.assertIn("git-policy --apply", body)

    def test_the_section_names_the_nuclear_option_as_not_an_override(self):
        body = self.text.split(SECTION, 1)[1].split("\n## ", 1)[0]
        self.assertIn("uninstall", body)


if __name__ == "__main__":
    unittest.main()
