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

import sys
import unittest

from charter import util
from charter.frame import layout


SESSION = dict(session="charter-demo-1234", conf="/tmp/f/tmux.conf",
               socket="charter", cols=200, rows=50,
               harness_argv=["claude", "--resume", "a;b"])

PANELS = dict(session="charter-demo-1234", socket="charter", harness_pane="%0")


def _direction(cmd: list[str]) -> str:
    """`-v` (splits along rows) or `-h` (splits along columns) — whichever is present."""
    return "-v" if "-v" in cmd else "-h"


def _size(cmd: list[str]) -> str:
    """The value passed to `-l`, as the literal string tmux would see on argv."""
    return cmd[cmd.index("-l") + 1]


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

    def test_it_asks_tmux_to_print_each_panels_pane_id(self):
        """Mirrors `SessionArgv.test_it_asks_tmux_to_print_the_pane_id`: a caller needs
        each panel's own pane id to re-assert its fixed size after tmux's own layout
        engine redistributes every pane proportionally on a resize (measured against
        tmux 3.7c — see this function's own docstring). `-P`/`-F` must land BEFORE the
        `--` separator (tmux's own option, never part of the `charter panel …` argv
        after it) — the same placement `session_argv` already uses."""
        for cmd in layout.panel_argvs(slots=["top", "bottom"], **PANELS):
            self.assertIn("-P", cmd)
            i = cmd.index("-P")
            self.assertEqual(cmd[i + 1:i + 3], ["-F", "#{pane_id}"])
            self.assertLess(i, cmd.index("--"),
                            "-P/-F must be split-window's own options, before --")

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


class PanelCommand(unittest.TestCase):
    """One definition of what a panel pane runs, because two modules start one.

    The launcher splits a pane for it; `commands_frame.cmd_respawn` brings the same
    panel back with `respawn-pane` after its `pane-died` hook fires (#382). Two
    hand-written copies of this argv drift, and the drift only ever shows up after
    something has already died once — which is the worst possible moment to discover
    the respawned panel is running a slightly different command.
    """

    def test_the_split_runs_exactly_what_a_respawn_would_run(self):
        split = layout.panel_argvs(slots=["bottom"], session="f-1", socket="charter",
                                   harness_pane="%0")[0]
        self.assertEqual(split[split.index("--") + 1:],
                         layout.panel_command(slot="bottom", session="f-1"))

    def test_it_carries_the_slot_and_the_session_the_cli_requires(self):
        """`cli.build_parser`'s `panel` parser takes `<slot> --session <fid>` and makes
        `--session` required — a command missing either is a pane that fails at startup,
        which is the hole #382's first half exists to make legible rather than to
        create a second source of."""
        self.assertEqual(
            layout.panel_command(slot="top", session="f-9"),
            [*util.self_relaunch_argv(), "panel", "top", "--session", "f-9"])

    def test_the_interpreter_half_is_the_shared_helpers_and_carries_dash_p(self):
        """#390, and the reason this function stopped taking a *charter_argv* at all.

        A `charter_argv` PARAMETER is what let the launcher and `cmd_respawn` disagree:
        the launcher moved to `util.self_relaunch_argv()` and the respawn kept a
        hand-built `[sys.executable, "-m", "charter"]`, so a respawned panel would have
        imported whatever `charter/` package sat in the pane's own cwd — a charter
        checkout, for anyone dogfooding. Asserted against the LITERAL prefix as well as
        against the helper, so a helper that itself lost `-P` cannot make this test
        agree with a broken production path (the helper's own shape is pinned in
        `tests/test_self_relaunch_argv.py`, but a test that only compares two things
        that move together proves neither)."""
        cmd = layout.panel_command(slot="bottom", session="f-1")
        self.assertEqual(cmd[:4], [sys.executable, "-P", "-m", "charter"])


class PanelGeometry(unittest.TestCase):
    """Pins the one property nothing else in this file checks: each slot's actual shape.

    `visible_slots` decides WHICH slots appear and `panel_argvs`' targeting tests pin
    WHERE the split lands, but nothing until now pinned the split itself — direction,
    `-b`, and `-l <size>` are three independent pieces of code (a membership check, a
    second membership check, and a dict lookup) that happen to agree with the intended
    frame only because each was written correctly, not because anything forces them to
    agree. Swap two `SLOT_SIZE` values, or transpose the two membership checks, and every
    other test in this file stays green while the frame ships sideways.
    """

    def test_horizontal_edges_split_vertically_and_top_goes_before_the_harness(self):
        """`top` and `bottom` are full-width, one-row strips, so both are cut with a
        VERTICAL split (`-v` divides the terminal along its rows — the axis that makes a
        one-row strip; `-h`, used by `left`/`right` below, divides it into side-by-side
        columns instead, which is the wrong shape for either of these). `top` is placed
        BEFORE the harness pane (`-b`); `bottom`, asserted right alongside it for
        contrast, goes after (no `-b`) — that contrast is what makes this one test with
        `bottom` in it rather than an isolated claim about `top`. Both are exactly
        `SLOT_SIZE["top"] == SLOT_SIZE["bottom"] == 1` row, asserted as the literal
        `"1"` rather than read back through `layout.SLOT_SIZE`: reading it back would
        still pass after `SLOT_SIZE` is swapped to `{"top": 22, ...}`, since the emitted
        `-l` always equals whatever the dict currently says — the literal is what makes
        the swap visible.

        Catches (verified by hand, see fix-round section of the task report): mutation 1
        (direction inverted and `-b` membership swapped) — `top`'s direction flips to
        `-h` and its `-b` disappears, both asserted here. Catches mutation 2 (`SLOT_SIZE`
        rows/cols swapped to 22/1) via the literal `"1"`.
        """
        top, bottom = layout.panel_argvs(slots=["top", "bottom"], **PANELS)
        self.assertEqual(_direction(top), "-v")
        self.assertEqual(_direction(bottom), "-v")
        self.assertIn("-b", top)
        self.assertNotIn("-b", bottom)
        self.assertEqual(_size(top), "1")
        self.assertEqual(_size(bottom), "1")

    def test_vertical_edges_split_horizontally_and_left_goes_before_the_harness(self):
        """Mirror of the test above, for the other axis. `left` and `right` are the side
        columns, so both are cut with a HORIZONTAL split (`-h` — the axis that makes a
        column; `-v`, used by `top`/`bottom` above, would instead slice off a row).
        `left` goes BEFORE the harness (`-b`); `right`, alongside it for the same
        contrast, goes after (no `-b`). Both are `SLOT_SIZE["left"] == SLOT_SIZE["right"]
        == 22` columns, again the literal `"22"` rather than read back through
        `layout.SLOT_SIZE`, for the reason given above.

        Catches: mutation 1 on `left` (direction flips to `-v`, `-b` disappears — both
        asserted here) and, together with the test above, rules out a fix that inverts
        direction correctly on one axis but not the other. Catches mutation 2 via the
        literal `"22"`.
        """
        left, right = layout.panel_argvs(slots=["left", "right"], **PANELS)
        self.assertEqual(_direction(left), "-h")
        self.assertEqual(_direction(right), "-h")
        self.assertIn("-b", left)
        self.assertNotIn("-b", right)
        self.assertEqual(_size(left), "22")
        self.assertEqual(_size(right), "22")


class WindowInTheOperatorsServer(unittest.TestCase):
    """`new-window` and `respawn-pane` — the frame built INSIDE a tmux the operator is
    already in, instead of a private server nested in their pane.

    Two commands rather than one for a measured reason (tmux 3.7c). `remain-on-exit` is
    what keeps a dead harness pane askable long enough for its exit status to be read,
    and there is no way to set it ON a pane that does not exist yet — every option tmux
    would otherwise inherit it from is global or session-scoped, and writing either on
    somebody else's server is exactly what this path exists not to do. So the window is
    created running a placeholder that never exits, `remain-on-exit` is set on that
    pane, and only THEN is the harness respawned into the same pane (`respawn-pane -k`
    keeps the pane's `%N` id, verified against 3.7c). The race the private-server path
    closes with `_PLACEHOLDER_CONF` is not narrowed here — it is removed.
    """

    def test_the_window_is_created_in_the_operators_own_session(self):
        cmd = layout.window_argv(socket="/private/tmp/tmux-502/default", session="$1",
                                 window="charter-demo-1234", cwd="/work/repo")
        self.assertEqual(cmd[:3], ["tmux", "-S", "/private/tmp/tmux-502/default"])
        self.assertIn("new-window", cmd)
        self.assertEqual(cmd[cmd.index("-t") + 1], "$1")
        self.assertEqual(cmd[cmd.index("-n") + 1], "charter-demo-1234")

    def test_the_window_is_created_in_the_background(self):
        """`-d`: the operator is switched to the frame once it is BUILT, never onto a
        half-drawn window with a placeholder running in it."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertIn("-d", cmd)

    def test_both_the_window_and_the_pane_ids_are_asked_for(self):
        """The pane id scopes every split and the exit-status query; the window id is
        what `kill-window` targets at the end. Indices are useless for both — tmux
        renumbers windows and panes (see this module's own docstring)."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertEqual(cmd[cmd.index("-F") + 1], "#{window_id} #{pane_id}")
        self.assertIn("-P", cmd)

    def test_the_placeholder_is_what_the_window_is_created_running(self):
        """After the `--`, so tmux runs it as a program rather than reading it as one
        of `new-window`'s own options — the same placement `session_argv` and
        `panel_argvs` already use for the harness and the panels.

        That the placeholder never exits on its own cannot be asserted from an argv;
        `tests/test_frame_tmux_integration.py` runs this exact command against a real
        server and reads back a pane still alive."""
        cmd = layout.window_argv(socket="/s", session="$1", window="f", cwd="/work/repo")
        self.assertTrue(layout.PLACEHOLDER, "a window has to be created running something")
        self.assertEqual(cmd[cmd.index("--") + 1:], layout.PLACEHOLDER)
        self.assertEqual(cmd.count("--"), 1)

    def test_the_harness_replaces_the_placeholder_in_the_same_pane(self):
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", env={}, cwd="/work/repo",
                                  harness_argv=["claude", "--resume", "a;b"])
        self.assertIn("respawn-pane", cmd)
        self.assertIn("-k", cmd)
        self.assertEqual(cmd[cmd.index("-t") + 1], "%7")
        self.assertEqual(cmd[cmd.index("--") + 1:], ["claude", "--resume", "a;b"])

    def test_the_harness_argv_is_never_joined(self):
        """The same rule the rest of this module pins: `a;b` reaching tmux as one
        element is inert, and as part of a joined string is a command separator."""
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", env={}, cwd="/work/repo",
                                  harness_argv=["claude", "-p", "hi; touch INJ"])
        self.assertEqual(cmd[-1], "hi; touch INJ")

    def test_charters_environment_rides_on_the_respawn_not_the_session(self):
        """`-e` puts a variable on THIS pane's own process and nowhere else. The
        private-server path carries `CHARTER_SESSION_ID` in the environment
        `new-session` inherits, which is not available here — the server is already
        running, and it is the operator's. `set-environment -t <their session>` would
        reach the harness, and would also hand every new shell they open a frame id
        that is not theirs.

        Measured against tmux 3.7c: `respawn-pane -e` REPLACES the pane's environment
        rather than adding to what `new-window -e` set, so everything the harness needs
        has to be on this call, not the one that made the window."""
        cmd = layout.respawn_argv(socket="/s", harness_pane="%7", cwd="/work/repo",
                                  env={"CHARTER_SESSION_ID": "demo-1",
                                       "CHARTER_HARNESS": "claude-code"},
                                  harness_argv=["claude"])
        self.assertIn("-e", cmd)
        pairs = [cmd[i + 1] for i, a in enumerate(cmd) if a == "-e"]
        self.assertEqual(sorted(pairs),
                         ["CHARTER_HARNESS=claude-code", "CHARTER_SESSION_ID=demo-1"])
        # ...and before the `--`, so they are `respawn-pane`'s own options and never
        # get grafted onto the harness's argv.
        self.assertTrue(all(cmd.index(p) < cmd.index("--") for p in pairs))


    def test_the_harness_starts_where_charter_was_typed(self):
        """A pane in a server charter did not start inherits the SESSION's directory —
        wherever the operator was when they first ran `tmux`, which is not where they
        typed `charter claude`. The panels split off this pane inherit its directory in
        turn, and `workspace.resolve()` reads exactly that."""
        for cmd in (layout.window_argv(socket="/s", session="$1", window="f",
                                       cwd="/work/repo"),
                    layout.respawn_argv(socket="/s", harness_pane="%7", env={},
                                        cwd="/work/repo", harness_argv=["claude"])):
            with self.subTest(cmd=cmd[3]):
                self.assertEqual(cmd[cmd.index("-c") + 1], "/work/repo")


    def test_a_panel_can_be_handed_an_environment_of_its_own(self):
        """A pane created on a server charter did not start inherits THAT SERVER's
        environment — whatever the operator's shell had when they first ran `tmux`, days
        ago. On charter's own server the panels inherit the launcher's environment
        because `new-session` is what starts the server; there is no such moment here, so
        the same values ride on `split-window -e`, exactly as the harness's do on
        `respawn-pane -e`.

        Omitted entirely when there is nothing to carry, so the private-server path's
        command is byte-for-byte what it always was."""
        with_env = layout.panel_argvs(slots=["top"], session="f", socket="/s",
                                      harness_pane="%3",
                                      env={"CHARTER_ROOT": "/plane"})[0]
        self.assertEqual(with_env[with_env.index("-e") + 1], "CHARTER_ROOT=/plane")
        self.assertLess(with_env.index("-e"), with_env.index("--"),
                        "`-e` is `split-window`'s own option, never part of the panel's "
                        "argv")
        without = layout.panel_argvs(slots=["top"], session="f", socket="/s",
                                     harness_pane="%3")[0]
        self.assertNotIn("-e", without)


class ServerSelection(unittest.TestCase):
    def test_a_split_can_be_aimed_at_the_operators_server_too(self):
        """`panel_argvs` and `session_argv` grew no new parameter for this: the socket
        they already take is now either charter's own server NAME or a socket PATH, and
        `tmuxctl.server_argv` is the one place that difference turns into `-L` or `-S`."""
        cmds = layout.panel_argvs(slots=["top"], session="f", socket="/tmp/tmux-1/default",
                                  harness_pane="%3")
        self.assertEqual(cmds[0][:3], ["tmux", "-S", "/tmp/tmux-1/default"])


if __name__ == "__main__":
    unittest.main()
