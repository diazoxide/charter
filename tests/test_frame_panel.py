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
from contextlib import redirect_stderr, redirect_stdout
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


class FailureIsVisibleInThePane(PersonaIso, unittest.TestCase):
    """A panel that cannot do its job must PAINT the reason and stay alive, because a
    panel that exits leaves nothing an operator can read.

    Measured against real tmux 3.7c, which is the whole reason this class exists.
    `remain-on-exit on` keeps a dead pane, and tmux then writes `Pane is dead (status
    N, <date>)` into it — but it writes that message by moving the cursor to the LAST
    row and issuing a linefeed first, which scrolls the pane up by exactly one line. In
    a six-row pane that costs the first of three stderr lines (`L1` gone, `L2`/`L3`
    still readable); in the ONE-ROW panes charter actually uses for `top` and `bottom`
    (`layout.SLOT_SIZE`) that one scrolled line is the entire pane, so a panel's stderr
    is provably unreadable there and `Pane is dead (status 2)` is all that is left. A
    pane whose process is still ALIVE keeps whatever it painted (measured the same way).

    So the fix is not "write the error somewhere better" — it is "do not exit". This is
    `slots.render`'s never-raise promise (`charter/frame/slots.py`) extended to the
    failures that happen BEFORE any renderer is reached, which is the exact gap it
    cannot cover from where it sits.
    """

    def test_an_unknown_slot_paints_the_reason_into_the_pane(self):
        """`Draw.test_an_unknown_slot_is_refused_rather_than_drawn_blank` above already
        pins the refusal; it passes just as well when the only trace is a stderr line
        tmux is about to scroll away. This pins the half that survives: the reason is on
        the pane's own SCREEN — stdout, the same file descriptor `_paint` writes to —
        naming the slot that was refused."""
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            rc = panel.run("sideways", "f-1", once=True)
        self.assertNotEqual(rc, 0)
        self.assertIn("sideways", out.getvalue(),
                      f"the refusal never reached the pane's own screen: {out.getvalue()!r}")

    def test_a_crash_before_render_is_painted_rather_than_ending_the_pane(self):
        """`slots.render` catches whatever a RENDERER raises, so the hole it guards is
        the one it cannot see from inside itself: anything raising on the way to it.
        Stood up here by making `render` itself raise — the one mock that reproduces
        every such failure at once (a `tui` helper blowing up, a `state` read this
        module calls directly, an fd that has gone away) without pretending to know
        which of them it will be."""
        out, err = io.StringIO(), io.StringIO()
        with mock.patch("charter.frame.panel.slots.render",
                        side_effect=RuntimeError("boom")):
            with redirect_stdout(out), redirect_stderr(err):
                rc = panel.run("bottom", "f-1", once=True)
        self.assertNotEqual(rc, 0)
        self.assertIn("RuntimeError", out.getvalue(),
                      f"the crash never reached the pane's own screen: {out.getvalue()!r}")

    def test_a_failed_panel_holds_the_pane_open_instead_of_exiting(self):
        """The half the painted text depends on. Painting the reason and then RETURNING
        is worth nothing: the process exits, tmux writes `Pane is dead (...)` over the
        one row this panel owns, and the reason is gone. `once=True` — the only way
        every other test in this file can call `run` at all — is exactly the mode that
        cannot show this, so this one runs the real loop and interrupts it from inside
        `time.sleep`. `KeyboardInterrupt` deliberately, not a plain exception: it is a
        `BaseException`, so it proves the hold is not merely being swallowed by the
        same `except Exception` that put us here."""
        slept = []

        def _interrupt(delay):
            slept.append(delay)
            raise KeyboardInterrupt

        with mock.patch("charter.frame.panel.time.sleep", side_effect=_interrupt):
            with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()):
                with self.assertRaises(KeyboardInterrupt):
                    panel.run("sideways", "f-1")
        self.assertTrue(slept, "a refused panel exited instead of holding its pane open")

    def test_the_painted_reason_is_clamped_to_the_panes_own_width(self):
        """The failure paint shares nothing with `slots.render` — deliberately, so a
        failure INSIDE the render path cannot take the message about it down too — which
        means it also does not inherit `render`'s own truncation. A message wider than a
        22-column `left` pane would wrap and scroll itself out of a pane it was written
        to be readable in.

        **The assertion is untouched by #591 and the fixture is the half that moved.** The
        `fileno` patch used to sit OUTSIDE `redirect_stdout`, which patched whatever
        `sys.stdout` was at that moment — `PersonaIso`'s own `StringIO`, not `out` — and
        then `redirect_stdout` swapped a different object in underneath it. So
        `sys.stdout.fileno()` raised `io.UnsupportedOperation` (an `OSError`), this ran on
        `slots._width`'s FALLBACK, and the fallback read `$COLUMNS`: the test passed on a
        machine with no `$COLUMNS` and failed at `COLUMNS=400`, painting an 88-cell line
        into a 24-column pane. Both halves were wrong and only one of them is a test's
        business — the fallback no longer reads the shell at all (`slots._DEFAULT_COLS`,
        pinned in `test_frame_slots.py::Width`), and the ordering below is what makes this
        case exercise what its name says: a pane whose own tty answers 24.

        `PaintClampsToRealHeight` and `RendersToItsOwnPaneWidth` in this file already nest
        the two the right way round; this is now the third.
        """
        out = io.StringIO()
        with redirect_stdout(out), redirect_stderr(io.StringIO()):
            with mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
                 mock.patch("os.get_terminal_size",
                            return_value=os.terminal_size((24, 1))):
                panel.run("a-very-long-slot-name-indeed", "f-1", once=True)
        painted = out.getvalue().split("\x1b[2J", 1)[1]
        for line in painted.split("\n"):
            self.assertLessEqual(tui.width(line), 24, painted)

    def test_a_pane_that_cannot_be_measured_is_not_measured_by_the_shell_instead(self):
        """#591's live half, at the call site that has to hold it.

        The case above states a pane's real width; this one takes the width away. A panel
        whose stdout has no tty behind it (`charter panel top --session x > /tmp/log`, the
        case `_DEFAULT_ROWS` already exists for) has nothing to measure — and the paint
        that runs at that moment is a FAILURE paint, so whatever it lays out against is
        what an operator gets in place of a working pane. Reaching for `$COLUMNS` there
        answered with the terminal charter was launched from, which a panel process
        inherits whole; a 400-column shell put an 88-cell line where 80 was the most
        anything could claim.

        Asserted against `slots._DEFAULT_COLS` rather than a literal, because the number
        is not the property — the property is that the answer does not move when the shell
        does. Red before the fix at exactly the value #591 reproduced with.
        """
        out = io.StringIO()
        with mock.patch.dict(os.environ, {"COLUMNS": "400"}):
            with redirect_stdout(out), redirect_stderr(io.StringIO()):
                with mock.patch("os.get_terminal_size",
                                side_effect=OSError("not a tty")):
                    panel.run("a-very-long-slot-name-indeed", "f-1", once=True)
        painted = out.getvalue().split("\x1b[2J", 1)[1]
        self.assertTrue(painted.strip(), "nothing was painted at all")
        for line in painted.split("\n"):
            self.assertLessEqual(tui.width(line), slots._DEFAULT_COLS, painted)


class Height(unittest.TestCase):
    """Mirrors `test_frame_slots.py`'s own `Width` class: `_rows` must measure the
    pane's own tty, not assume one — `left`/`right` panels are full height while
    `top` is one row (`layout.SLOT_SIZE`) and `bottom` is one on a plane with no clones
    (its floor, since #488), and this is the one function that
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


class IdleCostsTheSameWhateverTheBottomPaneIsTallEnoughToDraw(PersonaIso,
                                                              unittest.TestCase):
    """The budget this module's own docstring states, checked against the thing #488 and
    #500 changed underneath it: `bottom` is no longer a one-row strip, so "the frame
    costs nothing at idle" is a claim about a pane that can now be fifteen rows tall.

    An idle tick is the steady state a panel sits in between bumps — the version has not
    moved, the pane has not resized, nothing is in flight — and it must cost the same
    there whether the pane is one row or `statusline._MAX_REPO_LINES + 1`, because the
    loop runs `1 / panel.TICK` times a second for the life of the frame.

    Counted DIFFERENTIALLY and at every spelling a filesystem touch arrives by, for
    `tests/test_frame_slots.py::BottomTable`' own reasons: an absolute number would have
    to be revised whenever the version record changes shape, and a revised number is not
    a budget. `os.stat` alone would miss it — `state.version` is a `read_text`, which is
    `io.open` and not `builtins.open`; a directory walk is `os.scandir`; a git call is
    `subprocess.Popen` under `run`. Each of those is the next spelling this would
    otherwise miss.

    Sibling to `BottomTable.test_a_taller_pane_costs_the_same_syscalls_as_a_one_row_one`,
    which bounds a REPAINT. This bounds the tick that decides not to repaint at all —
    the one that actually runs five times a second when nothing is happening.
    """

    _WATCHED = (("os", ("stat", "lstat", "scandir", "listdir")),
                ("io", ("open",)), ("builtins", ("open",)),
                ("subprocess", ("run", "Popen")))

    def _idle_tick_calls(self, fid: str, repos: int) -> list[str]:
        import builtins
        import contextlib
        import io as _io
        import subprocess as _sp
        from charter.frame import gather
        mods = {"os": os, "io": _io, "builtins": builtins, "subprocess": _sp}
        gather.save(fid, {"gathered_at": 0.0, "workspace": "w", "current_repo": None,
                          "repos": [{"name": f"r{i}", "branch": "main", "dirty": False,
                                     "tracked_dirty": False, "ahead": 0, "behind": 0,
                                     "ci": None, "change": None, "sigil": "",
                                     "current": False, "worktree_count": 0}
                                    for i in range(repos)],
                          "worktrees": []})
        state.bump(fid)
        version = state.version(fid)
        rows = 1 + repos
        seen: list[str] = []
        size = mock.patch("os.get_terminal_size",
                          return_value=os.terminal_size((200, rows)))
        out = mock.patch.object(sys.stdout, "fileno", return_value=1, create=True)
        with size, out, redirect_stdout(io.StringIO()):
            # Primed first: the resolvers this reaches memoise, so an unprimed tick
            # would be measuring the priming rather than the steady state.
            panel._tick({"flag": False}, version, "bottom", fid)
            with contextlib.ExitStack() as stack:
                for mod_name, fns in self._WATCHED:
                    for fn in fns:
                        real = getattr(mods[mod_name], fn)
                        tag = f"{mod_name}.{fn}"
                        stack.enter_context(mock.patch(
                            tag,
                            (lambda r, t: lambda *a, **k:
                                (seen.append(t), r(*a, **k))[1])(real, tag)))
                painted = panel._tick({"flag": False}, version, "bottom", fid)
        self.assertEqual(painted, version,
                         "this tick repainted — it is not the idle one being budgeted")
        return seen

    def test_an_idle_tick_does_not_grow_with_the_panes_height(self):
        from charter import statusline
        short = self._idle_tick_calls("idle-short", 0)
        tall = self._idle_tick_calls("idle-tall", statusline._MAX_REPO_LINES)
        self.assertTrue(short, "nothing was counted at all — the budget is vacuous")
        self.assertEqual(
            sorted(tall), sorted(short),
            f"a tall pane's idle tick cost {len(tall)} filesystem calls where a one-row "
            f"pane's cost {len(short)} — the loop is doing per-row work when it is not "
            f"even repainting")
        self.assertEqual(len(short), 1,
                         f"an idle tick cost {len(short)} filesystem calls, not the one "
                         f"read of the frame's version this module's budget is: {short}")


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
