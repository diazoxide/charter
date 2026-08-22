"""A panel wakes because the agent did something, not because a timer went off.

The version file is a `stat` per tick, which is why the frame costs nothing at idle. A
FIFO was designed and rejected: opening one for write blocks until a reader exists, which
would put a hang inside the hook path.

Width is `slots.py`'s own responsibility, already pinned end to end in
`test_frame_slots.py`; `RendersToItsOwnPaneWidth` below only re-proves it holds THROUGH
`panel.run`, not that `slots._width` itself is correct. Height — the line COUNT painted,
not just each line's width — is this module's own job: `render()`'s contract is a single
string, so nothing downstream of it knows how tall the pane it is about to overwrite
actually is. `PaintClampsToRealHeight` pins that directly against a fake multi-line slot,
since every real slot today (`top`/`bottom`) only ever renders one line.
"""

from __future__ import annotations

import io
import os
import signal
import sys
import unittest
from contextlib import redirect_stdout
from unittest import mock

from charter import tui
from charter.frame import panel, slots, state

from tests._isolation import PersonaIso


class Draw(PersonaIso, unittest.TestCase):
    def test_one_pass_writes_the_slot_and_returns(self):
        """`rc == 0` alone survives `_paint` becoming a no-op — the "writes the slot"
        half of this test's own name needs the actual painted content checked, not just
        that `run` returned cleanly."""
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = panel.run("bottom", "f-1", once=True)
        self.assertEqual(rc, 0)
        self.assertIn("todo", buf.getvalue())

    def test_an_unknown_slot_is_refused_rather_than_drawn_blank(self):
        rc = panel.run("sideways", "f-1", once=True)
        self.assertNotEqual(rc, 0)


class Height(unittest.TestCase):
    """Mirrors `test_frame_slots.py`'s own `Width` class: `_rows` must measure the
    pane's own tty, not assume one — `left`/`right` panels are full height while
    `top`/`bottom` are one row (`layout.SLOT_SIZE`), and this is the one function that
    has to be right for both."""

    def test_rows_measures_the_panes_own_tty(self):
        with mock.patch("os.get_terminal_size", return_value=os.terminal_size((80, 7))):
            self.assertEqual(panel._rows(), 7)

    def test_rows_falls_back_to_a_default_when_no_tty_is_available(self):
        with mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
            self.assertEqual(panel._rows(), panel._DEFAULT_ROWS)


class PaintClampsToRealHeight(PersonaIso, unittest.TestCase):
    """`render()`'s contract is a single string with no notion of height — a future
    `left`/`right` slot could return more lines than a real pane holds, and painting all
    of them would scroll that PANEL'S OWN rows (each tmux pane keeps its own scroll
    region — a sibling pane is untouched), the same shape of corruption a stale-width
    repaint causes. Every real slot today (`top`/`bottom`) only ever emits one line, so
    this stands a fake multi-line slot in for a future renderer the same way
    `test_frame_slots.py`'s own `boom` fixture stands in for a failing one — restored via
    `finally`, not `addCleanup`, to match that file's own convention.
    """

    def test_a_taller_than_the_pane_render_is_clamped_to_the_panes_rows(self):
        slots.SLOTS["tall"] = lambda fid: "\n".join(f"line {i}" for i in range(10))
        try:
            buf = io.StringIO()
            with redirect_stdout(buf):
                with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
                     mock.patch("os.get_terminal_size", return_value=os.terminal_size((40, 3))):
                    rc = panel.run("tall", "f-1", once=True)
        finally:
            del slots.SLOTS["tall"]
        self.assertEqual(rc, 0)
        painted = buf.getvalue().split("\x1b[2J", 1)[1]
        self.assertEqual(len(painted.split("\n")), 3, painted)


class RendersToItsOwnPaneWidth(PersonaIso, unittest.TestCase):
    """The coordinator correction this task shipped under: a panel computes its width
    from its own tty, never from `$COLUMNS`, which it inherits WHOLE from the launching
    shell (measured: a 22-column pane whose launcher had exported `COLUMNS=200` saw
    `COLUMNS='200'`). `test_frame_slots.py` already pins this against `slots._width`/
    `slots.render` directly; this pins it end to end THROUGH `panel.run`, so a future
    change to `panel.py` itself — capturing stdout before `slots.render` ever sees the
    real fd, say — cannot quietly reopen it.
    """

    def test_panel_run_follows_the_pane_not_the_wider_columns_value(self):
        buf = io.StringIO()
        with mock.patch.dict(os.environ, {"COLUMNS": "500"}), redirect_stdout(buf):
            with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
                 mock.patch("os.get_terminal_size", return_value=os.terminal_size((18, 4))):
                rc = panel.run("top", "f-1", once=True)
        self.assertEqual(rc, 0)
        painted = buf.getvalue().split("\x1b[2J", 1)[1]
        for line in painted.split("\n"):
            self.assertLessEqual(tui.width(line), 18)


class Tick(PersonaIso, unittest.TestCase):
    """`_tick` is `run`'s decision-and-effect for ONE iteration, split out so the
    decision can be exercised without also exercising `run`'s `while True`/
    `time.sleep`. A resize must win even when the frame's own version has not moved —
    only charter's own hooks call `state.bump`; an operator resizing their terminal does
    not, so a version comparison alone would leave a pane painted for a size that no
    longer exists until unrelated agent activity happened to bump the version next.
    """

    def test_a_resize_paints_even_when_the_version_is_unchanged(self):
        seen = state.version("f-1")
        resized = {"flag": True}
        with mock.patch("charter.frame.panel._paint") as paint:
            panel._tick(resized, seen, "bottom", "f-1")
        paint.assert_called_once_with("bottom", "f-1")
        self.assertFalse(resized["flag"])

    def test_no_resize_and_no_version_change_does_not_paint(self):
        seen = state.version("f-1")
        resized = {"flag": False}
        with mock.patch("charter.frame.panel._paint") as paint:
            result = panel._tick(resized, seen, "bottom", "f-1")
        paint.assert_not_called()
        self.assertEqual(result, seen)

    def test_a_version_bump_paints_and_remembers_the_new_version(self):
        seen = state.version("f-1")
        state.bump("f-1")
        resized = {"flag": False}
        with mock.patch("charter.frame.panel._paint") as paint:
            result = panel._tick(resized, seen, "bottom", "f-1")
        paint.assert_called_once_with("bottom", "f-1")
        self.assertEqual(result, state.version("f-1"))
        self.assertNotEqual(result, seen)

    def test_a_bump_landing_during_the_paint_is_not_marked_seen(self):
        """Pins the read-before-paint ordering `_tick`'s own docstring argues for,
        deterministically rather than by argument alone: `_paint` is mocked to itself
        call `state.bump` — standing in for a hook firing WHILE this tick's paint is in
        flight — so the version has already moved again by the time `_paint` returns.
        `_tick` must not report the version it read at the START of this tick as "seen"
        in a way that satisfies "has the frame changed" for the version that landed
        during it; red on the one mutation (of thirteen) that swapped the read and the
        paint. Asserts against `state.version` directly — `panel.should_redraw`, the
        predicate this used to ask, was deleted as dead (no production caller ever
        reached it), and the question it answered is one `!=` either way."""
        fid = "f-race"
        state.bump(fid)
        seen = state.version(fid)
        state.bump(fid)
        with mock.patch("charter.frame.panel._paint",
                        side_effect=lambda slot, f: state.bump(f)):
            result = panel._tick({"flag": False}, seen, "bottom", fid)
        self.assertNotEqual(result, state.version(fid))


class Sigwinch(unittest.TestCase):
    """A resize does not bump the frame's version — only charter's own hooks call
    `state.bump` — so without this handler a pane would sit painted at the OLD size
    until unrelated activity happened to bump the version next."""

    def test_the_installed_handler_marks_the_resize_flag(self):
        resized = {"flag": False}
        old = panel._install_sigwinch(resized)
        try:
            signal.getsignal(signal.SIGWINCH)(signal.SIGWINCH, None)
            self.assertTrue(resized["flag"])
        finally:
            signal.signal(signal.SIGWINCH, old)


class RunOnceLoop(PersonaIso, unittest.TestCase):
    """`run(once=True)` is called IN-PROCESS by every test above, not only spawned as a
    subprocess's whole life — so it must leave nothing behind, and it must never so much
    as touch the loop's own throttle."""

    def test_once_true_restores_the_previous_sigwinch_handler(self):
        original = signal.getsignal(signal.SIGWINCH)
        panel.run("bottom", "f-1", once=True)
        self.assertEqual(signal.getsignal(signal.SIGWINCH), original)

    def test_once_true_returns_without_ever_sleeping(self):
        with mock.patch("charter.frame.panel.time.sleep",
                        side_effect=AssertionError("once=True must not sleep")):
            rc = panel.run("bottom", "f-1", once=True)
        self.assertEqual(rc, 0)


class CliWiring(unittest.TestCase):
    """`layout.panel_argvs` (Task 6, already merged and already spawning panel panes)
    emits exactly `*charter_argv, "panel", slot, "--session", session` — a CLI that does
    not accept that shape means every panel pane fails at startup and the frame comes up
    empty."""

    def test_the_launchers_argv_shape_parses(self):
        from charter.cli import build_parser
        ns = build_parser().parse_args(["panel", "bottom", "--session", "f-1"])
        self.assertEqual(ns.slot, "bottom")
        self.assertEqual(ns.session, "f-1")

    def test_the_parsed_args_dispatch_to_panel_run(self):
        from charter.cli import build_parser
        ns = build_parser().parse_args(["panel", "top", "--session", "f-2"])
        with mock.patch("charter.frame.panel.run", return_value=0) as run:
            rc = ns.func(ns)
        run.assert_called_once_with("top", "f-2")
        self.assertEqual(rc, 0)

    def test_session_is_required(self):
        from charter.cli import build_parser
        with self.assertRaises(SystemExit):
            build_parser().parse_args(["panel", "bottom"])


if __name__ == "__main__":
    unittest.main()
