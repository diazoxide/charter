"""A workspace name must never reach a tmux command string.

`display-menu` takes commands tmux parses and runs, and workspace, repo, branch and
persona names all come from committed files or `.git/HEAD`. Charter shipped a fix for
this exact shape one release ago — a branch name reaching `gh -F` and making it read a
file — and its conclusion was that the fix is the mechanism, not the value. So menu items
carry opaque ids and charter resolves them in-process.
"""

from __future__ import annotations

import unittest

from charter.frame import menu, state

from tests._isolation import PersonaIso

#: The task brief's own hostile string. Deliberately picked so it ALSO contains the
#: literal substring "run-shell" — a menu label is drawn verbatim (never executed), so a
#: label containing that text is exactly as harmless as one that doesn't, and any test
#: that treated "the joined argv doesn't contain the substring run-shell" as the safety
#: property would be testing the wrong thing (see `OpaqueIds`'s own docstring below).
HOSTILE = 'x" ; run-shell "touch /tmp/pwned'


class OpaqueIds(PersonaIso, unittest.TestCase):
    def test_the_action_slot_never_carries_the_label(self):
        """The property that actually matters, and the one the brief's own draft test
        ('assertNotIn("run-shell", flat)') cannot check: `menu_argv`'s FIXED action
        template is `run-shell 'charter frame-action a<N>'`, which legitimately contains
        the substring "run-shell" for every entry, hostile label or not — asserting that
        substring is absent from the whole joined argv fails against ANY correct
        implementation the moment a label happens to echo it back, which this one does on
        purpose. What must hold is narrower and stronger: the per-item COMMAND slot
        (`display-menu`'s third element of each `label key command` triple) is always
        that exact fixed string, independent of what the label next to it says — proven
        here by checking the command slot in isolation, not the flattened text."""
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "ws", "use", HOSTILE])])
        argv = menu.menu_argv("f-1", "charter")
        label, key, command = argv[-3:]
        self.assertEqual(label, HOSTILE, "a label is drawn verbatim, never executed")
        self.assertEqual(key, "1")
        self.assertEqual(command, "run-shell 'charter frame-action a0'")
        self.assertNotIn(HOSTILE, command)
        self.assertNotIn("/tmp/pwned", command)
        self.assertNotIn(";", command)

    def test_the_menu_targets_this_frames_own_session_explicitly(self):
        """Without `-t fid`, `display-menu` defaults to "whichever client is most
        recently active" — right the instant a bind's own action runs it directly, but
        `charter frame-menu` (`commands_frame.cmd_menu`) is a SEPARATE process one hop
        removed from that bind, so the default is no longer guaranteed once two frames
        are attached in two terminals at once. `fid` is free to pass explicitly (it is
        the session's own name, already restricted to a safe alphabet by
        `state.frame_id` — see `charter/frame/state.py`), so there is no reason to lean
        on an implicit default here."""
        argv = menu.menu_argv("f-1", "charter")
        self.assertEqual(argv[:6], ["tmux", "-L", "charter", "display-menu", "-t", "f-1"])

    def test_the_id_resolves_back_to_the_real_command_in_process(self):
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "ws", "use", HOSTILE])])
        entries = menu.build("f-1")
        self.assertEqual(len(entries), 1)
        label, action_id = entries[0]
        self.assertEqual(label, HOSTILE)
        self.assertEqual(menu.resolve("f-1", action_id),
                         ["charter", "ws", "use", HOSTILE])

    def test_an_unknown_id_resolves_to_nothing(self):
        self.assertIsNone(menu.resolve("f-1", "not-an-id"))

    def test_an_unrecorded_frame_resolves_to_nothing(self):
        """No `record()` call for this fid at all — the ordinary case for a menu id
        arriving after `reap()` already removed the frame's own directory."""
        self.assertIsNone(menu.resolve("never-recorded", "a0"))
        self.assertEqual(menu.build("never-recorded"), [])

    def test_an_id_is_only_ever_id_shaped(self):
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "true"])])
        for _label, action_id in menu.build("f-1"):
            self.assertRegex(action_id, r"^a[0-9]+$")

    def test_ids_stay_in_recorded_order_past_nine_entries(self):
        """`build` must not sort the id STRINGS lexicographically — "a10" would then
        land before "a2", silently reordering the menu the moment a frame grows past
        nine entries. `record` mints ids by plain insertion order; `build` must return
        them the same way."""
        entries = [(f"item{i}", ["true"]) for i in range(12)]
        menu.record(fid="f-1", entries=entries)
        got = [label for label, _id in menu.build("f-1")]
        self.assertEqual(got, [f"item{i}" for i in range(12)])

    def test_a_hostile_workspace_named_fid_never_becomes_a_stray_directory(self):
        """`frame_dir` refuses a hostile id rather than sanitising it (see its own
        docstring) — `record`/`build`/`resolve` must all fall through to a no-op/`None`
        for exactly that id, never invent a path of their own to write to instead."""
        hostile_fid = "../../etc/passwd"
        menu.record(fid=hostile_fid, entries=[("x", ["true"])])
        self.assertIsNone(state.frame_dir(hostile_fid))
        self.assertEqual(menu.build(hostile_fid), [])
        self.assertIsNone(menu.resolve(hostile_fid, "a0"))

    def test_a_label_with_a_newline_cannot_break_the_menus_own_layout(self):
        menu.record(fid="f-1", entries=[("line one\nline two", ["true"])])
        label, _id = menu.build("f-1")[0]
        self.assertNotIn("\n", label)

    def test_a_very_long_label_is_bounded(self):
        menu.record(fid="f-1", entries=[("x" * 500, ["true"])])
        label, _id = menu.build("f-1")[0]
        self.assertLessEqual(len(label), 60)

    def test_reading_a_corrupt_table_answers_empty_not_a_crash(self):
        d = state.frame_dir("f-1", create=True)
        (d / "actions.json").write_text("{not json")
        self.assertEqual(menu.build("f-1"), [])
        self.assertIsNone(menu.resolve("f-1", "a0"))


if __name__ == "__main__":
    unittest.main()
