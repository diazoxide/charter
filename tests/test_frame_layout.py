"""The frame's whole shape, decided without tmux.

Layout is pure so the feature is testable on a machine that has never installed tmux, and
so the argv rule is enforced by a unit test rather than by review: every element of every
command is a separate string, because tmux shell-interprets a joined one (pinned against
3.7c) and workspace, repo and branch names all reach here from committed files.

`session_argv` and `panel_argvs` are two functions rather than one `plan()` because the
launcher must run the first, read the harness's pane id off its stdout, and only then
build the splits — see `charter/frame/layout.py`'s module docstring for the measured tmux
3.7c failure (pane-index renumbering) a single up-front plan cannot avoid.
"""

from __future__ import annotations

import unittest

from charter.frame import layout


SESSION = dict(session="charter-demo-1234", conf="/tmp/f/tmux.conf",
               socket="charter", cols=200, rows=50,
               harness_argv=["claude", "--resume", "a;b"])

PANELS = dict(session="charter-demo-1234", socket="charter",
              charter_argv=["charter"], harness_pane="%0")


class VisibleSlots(unittest.TestCase):
    def test_a_wide_tall_terminal_keeps_every_slot(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 200, 50, 100, 20),
            ["top", "bottom", "left", "right"])

    def test_side_panels_go_first_when_the_terminal_is_narrow(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 80, 50, 100, 20),
            ["top", "bottom"])

    def test_the_top_goes_next_when_the_terminal_is_short(self):
        self.assertEqual(
            layout.visible_slots(["top", "bottom", "left", "right"], 200, 12, 100, 20),
            ["bottom"])

    def test_a_tiny_terminal_keeps_nothing(self):
        """Below the floor the harness gets the whole terminal. Degrading to a bare
        harness is the same move `statusline.render` makes when it runs out of width."""
        self.assertEqual(layout.visible_slots(["top", "bottom"], 40, 8, 100, 20), [])


class SessionArgv(unittest.TestCase):
    def test_the_harness_argv_survives_as_separate_elements_after_the_separator(self):
        """The security property: tmux does not shell-interpret separate argv elements
        but does interpret a joined string (pinned against 3.7c), and harness arguments
        come from the operator's own command line. Checking the exact tail — not just
        membership of `"--resume"` and `"a;b"` — also catches a `--` dropped or moved,
        and the harness argv reordered or truncated, none of which the brief's weaker
        membership check would have caught."""
        cmd = layout.session_argv(**SESSION)
        joined = [part for part in cmd if "claude --resume" in part]
        self.assertEqual(joined, [], "harness argv was joined into one string")
        self.assertEqual(cmd[cmd.index("--") + 1:], SESSION["harness_argv"])

    def test_the_command_is_a_list_of_separate_strings(self):
        cmd = layout.session_argv(**SESSION)
        self.assertIsInstance(cmd, list)
        for part in cmd:
            self.assertIsInstance(part, str)

    def test_the_socket_is_named(self):
        """One private server, never the operator's. Every command carries `-L`."""
        self.assertEqual(layout.session_argv(**SESSION)[:3], ["tmux", "-L", "charter"])

    def test_it_asks_tmux_to_print_the_pane_id(self):
        """Pins that `session_argv` actually requests the pane id, not merely `-P` in
        isolation: the whole two-function split depends on the launcher being able to
        read a real pane id off stdout, and `-P` with the wrong (or missing) `-F` prints
        something else just as silently as omitting the flag. Catches `-P` present but
        `-F`/`'#{pane_id}'` dropped or misspelled."""
        cmd = layout.session_argv(**SESSION)
        self.assertIn("-P", cmd)
        i = cmd.index("-P")
        self.assertEqual(cmd[i + 1:i + 3], ["-F", "#{pane_id}"])

    def test_the_session_is_created_detached(self):
        """`-d`: launched from a script with no tty to hand tmux. Without it tmux
        attaches and the call never returns, which a test would see as a hang rather
        than a clean failure — so this is worth pinning explicitly."""
        self.assertIn("-d", layout.session_argv(**SESSION))


class PanelArgvs(unittest.TestCase):
    def test_every_command_is_a_list_of_separate_strings(self):
        for cmd in layout.panel_argvs(slots=["top", "bottom"], **PANELS):
            self.assertIsInstance(cmd, list)
            for part in cmd:
                self.assertIsInstance(part, str)

    def test_each_visible_slot_gets_one_panel_command(self):
        cmds = layout.panel_argvs(slots=["top", "bottom"], **PANELS)
        panels = [c for c in cmds if "panel" in c]
        self.assertEqual(len(panels), 2)
        slots = {c[c.index("panel") + 1] for c in panels}
        self.assertEqual(slots, {"top", "bottom"})

    def test_the_socket_is_named_on_every_command(self):
        for cmd in layout.panel_argvs(slots=["top"], **PANELS):
            self.assertEqual(cmd[:3], ["tmux", "-L", "charter"])

    def test_every_split_targets_the_harness_pane_id_not_a_session_index(self):
        """The bug this two-function design exists to prevent (measured against tmux
        3.7c — see the module docstring): tmux renumbers pane INDICES on every split, so
        a target like `f"{session}:0.0"` stops naming the harness after the first split
        ever runs, and the next split divides the previous split's own panel instead —
        which eventually fails outright once that panel is one row tall. Fails if any
        split falls back to a `session:0.0`-style target instead of the pane id it was
        given."""
        cmds = layout.panel_argvs(slots=["top", "bottom", "left", "right"], **PANELS)
        for cmd in cmds:
            self.assertEqual(cmd[cmd.index("-t") + 1], "%0")
            self.assertNotIn(f"{PANELS['session']}:0.0", cmd)

    def test_every_slot_targets_the_same_pane_even_after_earlier_splits(self):
        """Companion to the test above, guarding against a plausible half-fix: only the
        FIRST split corrected to use the pane id, with later ones drifting back to some
        target derived from loop position (e.g. incrementing an index, or chaining off
        the pane an earlier split in this same list would have created). Every split must
        name the one id `panel_argvs` was handed, regardless of its position in *slots*."""
        cmds = layout.panel_argvs(slots=["top", "bottom", "left", "right"], **PANELS)
        targets = {cmd[cmd.index("-t") + 1] for cmd in cmds}
        self.assertEqual(targets, {"%0"})


if __name__ == "__main__":
    unittest.main()
