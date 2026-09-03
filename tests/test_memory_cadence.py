"""Record-memory cadence nudge: recording durable memory is a standing part of the flow,
but its salience fades on long sessions (context growth + compaction). A hook-fresh counter
re-surfaces the habit every _MEM_NUDGE_EVERY file-changes that produced no memory, and any
recorded memory (direct write OR the `edm … remember` CLI) resets it."""

from __future__ import annotations

import unittest

from charter import hooks
from tests._isolation import PersonaIso, PlaneIso, run_hook


class MemoryCadenceCase(PlaneIso):
    SID = "sess-cadence"

    def _edit(self, path):
        return run_hook(hooks.posttooluse,
                        {"tool_name": "Edit", "tool_input": {"file_path": path}, "session_id": self.SID})

    def _msg(self, r):
        return (r or {}).get("hookSpecificOutput", {}).get("additionalContext", "")

    def test_silent_until_threshold_then_nudges(self):
        fired = [i for i in range(1, hooks._MEM_NUDGE_EVERY + 1)
                 if self._edit(f"/x/edm/f{i}.py")]
        self.assertEqual(fired, [hooks._MEM_NUDGE_EVERY])  # only the Nth edit nudges
        self.assertEqual(hooks._memnudge_get(self.SID), hooks._MEM_NUDGE_EVERY)

    def test_nudge_recurs_every_interval(self):
        nudges = [i for i in range(1, hooks._MEM_NUDGE_EVERY * 2 + 1)
                  if self._edit(f"/x/edm/f{i}.py")]
        self.assertEqual(nudges, [hooks._MEM_NUDGE_EVERY, hooks._MEM_NUDGE_EVERY * 2])

    def test_direct_memory_write_resets(self):
        for i in range(5):
            self._edit(f"/x/edm/f{i}.py")
        self.assertEqual(hooks._memnudge_get(self.SID), 5)
        run_hook(hooks.posttooluse, {"tool_name": "Write", "session_id": self.SID,
                                     "tool_input": {"file_path": "/x/personas/dev/memory/f.md",
                                                    "content": "a fact"}})
        self.assertEqual(hooks._memnudge_get(self.SID), 0)

    def test_cli_remember_resets_via_pretooluse(self):
        for i in range(5):
            self._edit(f"/x/edm/f{i}.py")
        run_hook(hooks.pretooluse, {"session_id": self.SID,
                                    "tool_input": {"command": 'edm workspace remember "x"'}})
        self.assertEqual(hooks._memnudge_get(self.SID), 0)

    def test_persona_note_also_resets(self):
        for i in range(5):
            self._edit(f"/x/edm/f{i}.py")
        run_hook(hooks.pretooluse, {"session_id": self.SID,
                                    "tool_input": {"command": "python3 -m edm persona note dev 'y'"}})
        self.assertEqual(hooks._memnudge_get(self.SID), 0)

    def test_unrelated_bash_does_not_reset(self):
        for i in range(5):
            self._edit(f"/x/edm/f{i}.py")
        run_hook(hooks.pretooluse, {"session_id": self.SID,
                                    "tool_input": {"command": "git status"}})
        self.assertEqual(hooks._memnudge_get(self.SID), 5)

    def test_no_session_id_is_silent_and_safe(self):
        r = run_hook(hooks.posttooluse,
                     {"tool_name": "Edit", "tool_input": {"file_path": "/x/edm/f.py"}})
        self.assertIsNone(r)

    def test_nudge_message_mentions_recording(self):
        r = None
        for i in range(hooks._MEM_NUDGE_EVERY):
            r = self._edit(f"/x/edm/f{i}.py")
        msg = self._msg(r)
        self.assertIn("Memory check", msg)
        self.assertIn("remember", msg)  # names the recording command

    def test_memory_write_does_not_count_as_work(self):
        # a memory write should reset, never bump toward a nudge
        run_hook(hooks.posttooluse, {"tool_name": "Write", "session_id": self.SID,
                                     "tool_input": {"file_path": "/x/workspaces/w/memory/a.md",
                                                    "content": "note"}})
        self.assertEqual(hooks._memnudge_get(self.SID), 0)


if __name__ == "__main__":
    unittest.main()
