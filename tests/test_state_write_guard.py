"""A session may not hand-write the files that decide its own permissions (#432).

The vault-read guard denies READING `.charter/`. Writing it was never guarded at all, and
three of the files there are read back by `toolgate.decide` on the next Bash call: the
active-persona pointer, the per-session persona pointer, and the tool ceiling. So the
shape the assessment reproduced — rewrite the pointer, get another persona's declared
tools — needed no Bash at all: one `Write` did it.

`charter persona use` and every other CLI writer is untouched. They write the file
directly; this guard is on the harness's Write/Edit tools, which is where the *agent*
writes.
"""
from __future__ import annotations

import unittest
from pathlib import Path

from charter import config, hooks
from tests._isolation import PersonaIso, run_hook


class StateWriteCase(PersonaIso):
    def setUp(self):
        super().setUp()
        # `_state_write_reason` is gated on there being a control plane, for the reason A2
        # states — this handler runs in every repo on the machine.
        (self.tmp / "charter.toml").write_text("[plane]\n")
        self._orig_plane = config.HAS_CONTROL_PLANE
        config.HAS_CONTROL_PLANE = True
        self.addCleanup(setattr, config, "HAS_CONTROL_PLANE", self._orig_plane)

    def write(self, path, tool="Write", key="file_path"):
        return run_hook(hooks.pretooluse_edit, {
            "hook_event_name": "PreToolUse", "tool_name": tool,
            "session_id": "sess-state-write", "cwd": str(self.tmp),
            "tool_input": {key: str(path), "content": "x"},
        })

    def assertDenied(self, out):
        self.assertIsNotNone(out, "no decision emitted at all")
        d = (out.get("hookSpecificOutput") or {})
        self.assertEqual(d.get("permissionDecision"), "deny", d)
        return d.get("permissionDecisionReason", "")

    def assertNotDenied(self, out):
        d = ((out or {}).get("hookSpecificOutput") or {})
        self.assertNotEqual(d.get("permissionDecision"), "deny", d)


class TestCharterOwnState(StateWriteCase):
    def test_the_active_persona_pointer_cannot_be_written(self):
        reason = self.assertDenied(self.write(config.ACTIVE_PERSONA_FILE))
        self.assertIn("charter persona use", reason)

    def test_every_file_the_gate_reads_back_is_covered(self):
        for p in (config.ACTIVE_PERSONA_FILE,
                  config.SESSIONS_DIR / "sess-state-write.persona",
                  config.SESSIONS_DIR / "sess-state-write.tools",
                  config.VAULTS_REGISTRY,
                  config.VAULTS_DIR / "devops.json",
                  config.STATE_DIR / "terminals" / "t.persona"):
            with self.subTest(path=p):
                self.assertDenied(self.write(p))

    def test_a_relative_path_is_the_same_answer(self):
        """The hook is handed whatever the agent typed, and `cwd` with it."""
        rel = Path(config.ACTIVE_PERSONA_FILE).relative_to(self.tmp)
        self.assertDenied(self.write(rel))

    def test_a_symlink_into_the_state_dir_is_the_same_answer(self):
        """The guard is about the file that gets written, not its spelling — otherwise
        one `ln -s` restores the hole."""
        link = self.tmp / "shortcut"
        config.STATE_DIR.mkdir(parents=True, exist_ok=True)
        link.symlink_to(config.STATE_DIR)
        self.assertDenied(self.write(link / "active-persona"))

    def test_edit_and_multiedit_are_the_same_answer(self):
        for tool in ("Edit", "MultiEdit"):
            with self.subTest(tool=tool):
                self.assertDenied(self.write(config.ACTIVE_PERSONA_FILE, tool=tool))

    def test_an_ordinary_file_is_untouched(self):
        self.assertNotDenied(self.write(self.tmp / "docs" / "notes.md"))

    def test_a_persona_definition_is_untouched(self):
        """Editing a persona charter on request is ordinary work. What made it dangerous
        was the tool-gate re-reading it mid-session, which `toolgate.frozen_tools` fixes at the
        reading end. Denying the edit as well would break the authoring flow to close a
        hole that is already closed."""
        self.make_persona("dev", role="dev", vault="none", tools="ls")
        self.assertNotDenied(self.write(config.PERSONAS_DIR / "dev" / "persona.md"))

    def test_outside_a_control_plane_nothing_is_denied(self):
        """This handler runs in every repo on the machine. A denial where no plane exists
        explains a control plane that is not there — the complaint A2 already answered."""
        config.HAS_CONTROL_PLANE = False
        self.assertNotDenied(self.write(config.ACTIVE_PERSONA_FILE))


if __name__ == "__main__":
    unittest.main()
