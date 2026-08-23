"""A workspace name must never reach a tmux command string — and a label is not inert.

`display-menu` takes commands tmux parses and runs, and workspace, repo, branch and
persona names all come from committed files or `.git/HEAD`. Charter shipped a fix for
this exact shape one release ago — a branch name reaching `gh -F` and making it read a
file — and its conclusion was that the fix is the mechanism, not the value. So menu items
carry opaque ids and charter resolves them in-process.

But a menu item's own NAME is also not inert: `display-menu`'s own docs say "The name and
command are formats" — `#(shell command)` runs the moment tmux draws the menu, no
selection needed, and `#{variable}` substitutes a value. This module's own first version
claimed "a label is drawn, never executed" and pinned that with a test; both were wrong,
confirmed by hand in a real frame (an unescaped `#(touch CANARY)` label created CANARY the
instant the menu was drawn, and the hostile row was invisible — nothing about the RENDERED
menu looked wrong). `OpaqueIds` below tests the id/argv boundary at the unit level;
`tests/test_frame_tmux_integration.py`'s `MenuFormatIntegration` proves the label boundary
against a real, rendered `display-menu`, with a canary — an argv-shape assertion alone
cannot prove tmux's own format parser was actually defeated.
"""

from __future__ import annotations

import json
import sys
import unittest

from charter.frame import menu, state

from tests._isolation import PersonaIso

#: The task brief's own hostile string, for the id/argv boundary below. Contains no `#`,
#: no leading `-`, and does not end in `#` — none of `_safe_label`'s own transforms
#: apply to it, so it round-trips through `menu_argv` unchanged, which is exactly what
#: lets `test_the_action_slot_never_carries_the_label` assert on it byte for byte.
HOSTILE = 'x" ; run-shell "touch /tmp/pwned'


class OpaqueIds(PersonaIso, unittest.TestCase):
    def test_the_action_slot_never_carries_the_label(self):
        """The property that actually matters, and the one the brief's own draft test
        ('assertNotIn("run-shell", flat)') cannot check: `menu_argv`'s FIXED action
        template is `run-shell '"$CHARTER_PY" -m charter frame-action a<N>'`, which
        legitimately contains
        the substring "run-shell" for every entry, hostile label or not — asserting that
        substring is absent from the whole joined argv fails against ANY correct
        implementation the moment a label happens to echo it back, which this one does on
        purpose. What must hold is narrower and stronger: the per-item COMMAND slot
        (`display-menu`'s third element of each `label key command` triple) is always
        that exact fixed string, independent of what the label next to it says — proven
        here by checking the command slot in isolation, not the flattened text."""
        menu.record(fid="f-1", entries=[(HOSTILE, ["charter", "ws", "use", HOSTILE])])
        argv = menu.menu_argv("f-1", "charter", client="/dev/ttys0")
        label, key, command = argv[-3:]
        # NOT "a label is drawn verbatim, never executed" — that was this module's own
        # first, wrong claim (see the module docstring). HOSTILE happens to round-trip
        # unchanged only because it triggers none of `_safe_label`'s own transforms (no
        # `#`, no leading `-`, no trailing `#`); see `LabelSafety` below for labels that
        # DO trigger them, and `MenuFormatIntegration` for the property that actually
        # matters, proven against a real, rendered menu.
        self.assertEqual(label, HOSTILE)
        self.assertEqual(key, "1")
        self.assertEqual(command,
                         'run-shell \'"$CHARTER_PY" -m charter frame-action a0\'')
        self.assertNotIn(HOSTILE, command)
        self.assertNotIn("/tmp/pwned", command)
        self.assertNotIn(";", command)

    def test_the_action_runs_charter_through_a_named_interpreter_not_off_the_path(self):
        """Same defect as the hotkey bind's own (`test_frame_launcher.Conf`), one layer
        down: every MENU ITEM ran a bare `charter` too. With charter not on the tmux
        server's `$PATH`, selecting "Detach" makes `run-shell` print
        `'charter frame-action a0' returned 127` INTO THE HARNESS PANE and drop it into
        copy-mode — charter drawing in the one rectangle ADR 0018 says it never draws.

        The interpreter is a VARIABLE the session carries, never `sys.executable` baked
        in here: an absolute path re-embedded inside this nested tmux-quote layer is the
        construction `commands_frame`'s module docstring bans for `status_path`."""
        menu.record(fid="f-1", entries=[("Detach", ["true"])])
        command = menu.menu_argv("f-1", "charter", client="/dev/ttys0")[-1]
        self.assertNotIn("run-shell 'charter", command)
        self.assertIn('"$CHARTER_PY" -m charter frame-action', command)
        self.assertNotIn(sys.executable, command)

    def test_the_menu_targets_this_frames_own_client_and_session(self):
        """`-c client` is what selects WHICH ATTACHED TERMINAL sees the menu —
        `display-menu`'s own docs: "Display a menu on target-client." `-t fid` alone
        does NOT choose it (verified by hand against tmux 3.7c with two frames attached
        in two terminals: `-t fid` rendered the wrong frame's menu on the wrong screen) —
        it only scopes format evaluation for the item's own command text, which is why
        both flags are asserted here rather than either alone."""
        argv = menu.menu_argv("f-1", "charter", client="/dev/ttys3")
        self.assertEqual(argv[:9],
                         ["tmux", "-L", "charter", "display-menu", "-t", "f-1",
                          "-c", "/dev/ttys3", "-T"])

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

    def test_a_key_not_shaped_a_n_is_refused_at_the_point_of_use(self):
        """Only `record` mints ids today, so this is defence in depth rather than a live
        hole — but `menu_argv` interpolates whatever `build` hands it directly into text
        tmux re-parses (`run-shell '"$CHARTER_PY" -m charter frame-action <id>'`), so a
        corrupted or hand-edited table with a key like `a0'; run-shell "touch x` must
        never reach that f-string. Written directly to the table `record` itself would
        write to, bypassing `record`'s own minting, to prove the guard is at `build`,
        not only at the one writer that happens to behave today."""
        d = state.frame_dir("f-1", create=True)
        (d / "actions.json").write_text(json.dumps({
            "a0": {"label": "fine", "argv": ["true"]},
            "a0'; run-shell \"touch /tmp/pwned": {"label": "hostile key",
                                                  "argv": ["touch", "/tmp/pwned"]},
        }))
        entries = menu.build("f-1")
        self.assertEqual([label for label, _id in entries], ["fine"])
        argv = menu.menu_argv("f-1", "charter", client="/dev/ttys0")
        self.assertNotIn("pwned", " ".join(argv))

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

    def test_an_empty_label_does_not_desync_the_menus_own_triples(self):
        """`display-menu` treats an empty NAME as a separator line — both the key and
        command are meant to be omitted for one, per its own docs. `record` still writes
        a `label key command` triple regardless, so an empty label would silently
        desync every triple after it and `display-menu` would fail outright with "not
        enough arguments" — the hotkey doing nothing, with nothing anywhere saying why."""
        menu.record(fid="f-1", entries=[("", ["true"]), ("second", ["true"])])
        labels = [label for label, _id in menu.build("f-1")]
        self.assertTrue(labels[0], "an empty label must become a non-empty placeholder")
        self.assertEqual(labels[1], "second")

    def test_a_label_of_only_newlines_also_gets_a_placeholder(self):
        """Newline-stripping alone can turn a non-empty label into an all-space one but
        never into an EMPTY one (`\\n` -> ` `, never -> `''`) — this pins the one input
        shape that actually reaches record's own empty check: a literally empty string."""
        menu.record(fid="f-1", entries=[("", ["true"])])
        label, _id = menu.build("f-1")[0]
        self.assertNotEqual(label, "")

    def test_reading_a_corrupt_table_answers_empty_not_a_crash(self):
        d = state.frame_dir("f-1", create=True)
        (d / "actions.json").write_text("{not json")
        self.assertEqual(menu.build("f-1"), [])
        self.assertIsNone(menu.resolve("f-1", "a0"))

    def test_a_non_string_label_in_a_corrupted_table_does_not_crash_the_hotkey(self):
        """`build` used to return `v.get("label", "")` unvalidated — a value that
        EXISTS but is the wrong type (`{"label": 123}`) is not caught by that default
        (the default only ever applies when the KEY is missing), and `123.replace(...)`
        inside `_safe_label` raised `AttributeError` the moment `menu_argv` tried to
        escape it. Only `record` writes a `str` today, so this is defence in depth —
        the same class of guard `_ACTION_ID_RE` already applies to the key next to it,
        written directly to bypass `record`'s own minting, the same way that test
        does."""
        d = state.frame_dir("f-1", create=True)
        (d / "actions.json").write_text(json.dumps({"a0": {"label": 123, "argv": ["true"]}}))
        self.assertEqual(menu.build("f-1"), [("(untitled)", "a0")])
        # Must not raise, and must not embed the id where the label was expected.
        argv = menu.menu_argv("f-1", "charter", client="/dev/ttys0")
        self.assertEqual(argv[-3], "(untitled)")

    def test_a_missing_label_key_also_gets_the_placeholder(self):
        d = state.frame_dir("f-1", create=True)
        (d / "actions.json").write_text(json.dumps({"a0": {"argv": ["true"]}}))
        self.assertEqual(menu.build("f-1"), [("(untitled)", "a0")])


class LabelSafety(PersonaIso, unittest.TestCase):
    """`_safe_label` and its use inside `menu_argv` — the fix for the CRITICAL finding:
    a `display-menu` item name is a tmux FORMAT, not inert text. Argv-level assertions
    only; the render-time proof (a real `#(...)` label creating no canary against a real,
    attached client) lives in `test_frame_tmux_integration.py`'s `MenuFormatIntegration`."""

    def _label_argv(self, label: str) -> str:
        menu.record(fid="f-1", entries=[(label, ["true"])])
        return menu.menu_argv("f-1", "charter", client="/dev/ttys0")[-3]

    def test_a_shell_job_label_is_escaped_hash_by_hash(self):
        """`#(cmd)` runs `cmd` the instant tmux draws the menu (confirmed by hand — see
        the module docstring). `##` is tmux's own escape for a literal `#`; doubling
        every occurrence is what closes it — checked here as an exact string match, not
        merely "the substring #( is gone", since a partial escape (only the first #, say)
        would still leave a working `#(...)` job one character later."""
        rendered = self._label_argv("#(touch /tmp/pwned)")
        self.assertEqual(rendered, "##(touch /tmp/pwned)")

    def test_a_format_variable_label_is_escaped_too(self):
        """`#{session_name}` substitutes a value rather than running a job, but it is the
        SAME construction (an unescaped `#`) and the SAME fix closes it — no separate
        mechanism needed for the two forms tmux's FORMATS section documents."""
        rendered = self._label_argv("#{session_name}")
        self.assertEqual(rendered, "##{session_name}")

    def test_every_hash_is_doubled_not_only_the_first(self):
        rendered = self._label_argv("##(a)##(b)#")
        self.assertEqual(rendered.count("#"), 2 * "##(a)##(b)#".count("#"))

    def test_a_leading_hyphen_no_longer_disables_the_row(self):
        """`display-menu`'s own docs: a name starting with `-` is "shown dim and may not
        be chosen" — the whole item becomes unselectable, not merely oddly labelled.
        Exactly correction 4's "truncated into something misleading" failure, reached
        through a menu row instead of a status line."""
        rendered = self._label_argv("-my-branch")
        self.assertFalse(rendered.startswith("-"))
        self.assertIn("my-branch", rendered)

    def test_a_trailing_hash_gets_a_trailing_space(self):
        """Cosmetic, not a safety hole (the escape above already makes it inert either
        way) — but a label doubled from a single trailing `#` collides with a style-reset
        sequence tmux appends after every item's own name, rendering literal
        `label#[default]` garbage (confirmed by hand; a label with no trailing hash never
        showed it). A trailing space breaks the adjacency."""
        rendered = self._label_argv("trailing#")
        self.assertEqual(rendered, "trailing## ")

    def test_a_label_with_none_of_the_special_shapes_is_unchanged(self):
        rendered = self._label_argv("ordinary text")
        self.assertEqual(rendered, "ordinary text")


if __name__ == "__main__":
    unittest.main()
