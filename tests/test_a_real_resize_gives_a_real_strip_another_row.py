"""A real window resize gives a real tab strip a real second row — and takes it back.

*"panes are resizable — and user can resize and it should show more tabs opened on new
resized rows."*

**This is pane geometry, so it is asked of tmux.** `tests/test_a_tab_strip_grows_a_row_
when_its_tabs_overflow.py` says what charter's arithmetic decides and what the renderer
draws into the rows it is given; neither can say that `resize-pane -y 3` on a pane that was
split `-l 1` actually leaves a three-row pane, or that the harness gives up exactly those
rows and no others. tmux redistributes every pane proportionally on a window resize and
grants an over-large height out of the neighbour rather than refusing it, which is why
`layout.HARNESS_MIN_ROWS` exists at all — so the one authority on whether a strip that grew
took its rows from the right place is the server.

**`commands_frame._reassert_sizes` is driven directly**, which is the same choice
`TmuxIntegration.test_the_frames_panes_keep_their_sizes_across_a_window_resize` makes and
for its reason: the `window-resized` hook's whole job is to run `charter frame-resize`,
which calls this, and a test that installed the hook would be measuring tmux's dispatch
rather than charter's answer.

**Verified by hand at the 3.2 floor as well**, which no test in this file can do — the
fixture runs the `tmux` on `$PATH`. On `~/.local/share/charter-testing/tmux-3.2`,
`set-hook -w window-resized …` answers `invalid option: window-resized`, rc 1, so nothing
re-lays-out on a resize there; the same `resize-pane -y` this file asserts on does work,
which is why `charter frame-resize` typed by hand is the recovery
`tmuxctl.below_resize_hook_message` already names.
"""

from __future__ import annotations

import os
import time
import unittest
from pathlib import Path
from unittest import mock

from charter import commands_frame, config, instance
from charter.frame import layout, state

from tests._isolation import PersonaIso, make_plane
from tests.test_frame_tmux_integration import (
    _HAS_TMUX, SOCKET, _REPO_ROOT, _TmuxServerFixture, _tmux,
)

#: How long a real panel process is given to come up and repaint. Polled, never slept
#: through: `_await` returns the moment the pane says what it was asked about, so a fast
#: machine pays a few hundredths and a loaded one still passes.
_DEADLINE = 30.0


def _await(predicate, timeout: float = _DEADLINE) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return predicate()

#: The fifteen workspaces this repository's own plane has — the list #725 measured the
#: windowed rung against. At 120 columns they need three rows and at 360 they need one,
#: which is exactly the band a resize crosses. (300 was the top of that band until #880
#: gave every workspace tab a `slots.TAB_COUNT_W` reserve for its chat count.)
NAMES = sorted([
    "authority-audit", "autonomy", "charter-update-skill", "default", "fleet",
    "harness-wrapper", "news-dispatch-guard", "opencode-integration", "plane-shape",
    "relations-and-delegations", "showcase", "statusline-improvements", "todos",
    "tracking-github-issues", "user-reporting",
])


@unittest.skipUnless(_HAS_TMUX, "tmux is not installed")
class AResizeGivesTheStripTheRowsItsNamesNeed(_TmuxServerFixture, PersonaIso):
    """One real server, one real window, and `_reassert_sizes` driven across a resize.

    **#903 added a second question to the same fixture and it is asked here for a
    reason.** A tmux pane drag is `resize-pane -y` — there is no other way to say it, and
    no hook that fires on one (`window-resized` is about the WINDOW) — so charter notices
    a drag on its NEXT resize, by comparing the strips' real heights against what it last
    asserted. Both halves of that turn on facts only the server has: that asserting every
    strip and then the harness leaves the strips where they were put (or every resize
    adopts a height nobody chose and the strip freezes for the life of the frame), and
    that a real `resize-pane` by a hand is visible to the same comparison.
    `tests/test_a_dragged_tab_strip_keeps_its_height.py` tells charter what the heights
    are; only this file can ask tmux.
    """

    def setUp(self):
        super().setUp()
        for name in NAMES:
            (Path(config.WORKSPACES_DIR) / name).mkdir(parents=True, exist_ok=True)
        self.placed = instance.frame_of({"frame": {"component": [
            {"use": "identity"}, {"use": "workspaces", "edge": "top", "size": 1},
            {"use": "attention"}, {"use": "repos"}]}})
        self.assertIn("workspaces", self.placed["slots"],
                      "the arrangement did not place the strip, so nothing below is "
                      "about a placed component")

    def _window(self, session, cols, rows):
        """A real window with a harness pane and the four panels split off it, in the
        arrangement's own order — every strip one row, which is what a launch that had
        never measured its names would produce."""
        r = _tmux("new-session", "-d", "-s", session, "-x", str(cols), "-y", str(rows),
                  "-P", "-F", "#{pane_id}")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        panes = {}
        for slot, args in (("top", ("-v", "-b", "-l", "1")),
                           ("workspaces", ("-v", "-b", "-l", "1")),
                           ("bottom", ("-v", "-l", "1")),
                           ("repos", ("-v", "-l", "6"))):
            out = _tmux("split-window", "-t", harness, *args,
                        "-P", "-F", "#{pane_id}", "--", "sleep", "600")
            self.assertEqual(out.returncode, 0, out.stderr)
            panes[slot] = out.stdout.strip()
        return harness, panes

    def _height(self, pane):
        return int(_tmux("display-message", "-p", "-t", pane,
                         "#{pane_height}").stdout.strip())

    def _reassert(self, session, harness, panes, cols, rows):
        self.assertEqual(_tmux("resize-window", "-t", session,
                               "-x", str(cols), "-y", str(rows)).returncode, 0)
        fid = state.frame_id(session, os.getpid())
        # **The ceiling is raised first, and since #880 that is what makes any of this
        # measurable.** A launch is one row deep whatever the names need, so a case about
        # the recompute giving a strip its rows has to have pressed `layout.BAR_ROWS_KEY`
        # — this is `commands_frame.cmd_bar_rows`' write, made directly because what is
        # under test here is the RESIZE and not the keypress
        # (`tests/test_a_key_cycles_the_tab_strips_height.py` is that one).
        state.record_bar_rows(fid, layout.BAR_MAX_ROWS)
        with mock.patch.dict(config.FRAME, self.placed), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=6):
            commands_frame._reassert_sizes(SOCKET, fid=fid, panes=panes,
                                           harness_pane=harness,
                                           window_cols=cols, window_rows=rows)

    def _pass(self, session, harness, panes, cols, rows):
        """One `_reassert_sizes` over a real window — :meth:`_reassert` WITHOUT its
        `record_bar_rows`, because a case about what charter ADOPTS may not start by
        recording the answer it is asking about (#903).
        """
        fid = state.frame_id(session, os.getpid())
        with mock.patch.dict(config.FRAME, self.placed), \
                mock.patch("charter.frame.slots.repos_rows_wanted", return_value=6):
            commands_frame._reassert_sizes(SOCKET, fid=fid, panes=panes,
                                           harness_pane=harness,
                                           window_cols=cols, window_rows=rows)
        return fid

    def test_a_narrow_window_leaves_the_strip_the_rows_its_names_need(self):
        """120 columns cuts these fifteen names into three pages, so the strip that was
        split one row tall comes back three rows tall — and the operator can press twelve
        workspaces where one row reached five."""
        harness, panes = self._window("grow-strip", 120, 40)
        self._reassert("grow-strip", harness, panes, 120, 40)
        self.assertEqual(self._height(panes["workspaces"]), 3)

    def test_the_rows_come_off_the_harness_and_not_off_the_table(self):
        """`layout._DROP_ORDER` gives the bars up BEFORE `repos` when rows run short, so
        they may not be fed from it when rows are plentiful. Asked of tmux because the
        table pane is the one `_reassert_sizes` never asserts a height for — it takes the
        remainder, so it is where a mis-charged row would land."""
        harness, panes = self._window("grow-budget", 120, 40)
        self._reassert("grow-budget", harness, panes, 120, 40)
        self.assertEqual(self._height(panes["repos"]), 6)
        self.assertEqual(self._height(panes["top"]), 1)
        self.assertEqual(self._height(panes["bottom"]), 1)
        # 40 rows, five panes, four borders: 3 + 1 + 1 + 6 + 4 = 15 spent on panels.
        self.assertEqual(self._height(harness), 25)

    def test_widening_the_window_takes_the_rows_back(self):
        """The gesture in both directions, which is the half a launch-time-only answer
        cannot have. 360 columns draws all fifteen names on one row, so the strip that
        grew to three gives two of them back to the harness rather than keeping a height
        it no longer needs."""
        harness, panes = self._window("shrink-strip", 120, 40)
        self._reassert("shrink-strip", harness, panes, 120, 40)
        self.assertEqual(self._height(panes["workspaces"]), 3)
        self._reassert("shrink-strip", harness, panes, 360, 40)
        self.assertEqual(self._height(panes["workspaces"]), 1)
        self.assertEqual(self._height(harness), 27)

    def test_a_real_panel_fills_the_row_the_resize_gave_it(self):
        """The last link, and the only one a fake cannot stand in for: a real
        `charter panel workspaces` process, in a real pane, redrawing into a row it did not
        have a moment ago.

        Everything else here is charter deciding a number and tmux honouring it. This is
        the panel LEARNING the number — `SIGWINCH` for its own pane, `panel._watch`'s
        repaint, `events.Dispatcher.note_resize`, and `ctx.height` reaching
        `slots.chats_bar`'s sibling. A renderer handed the width alone would leave the row
        the resize just bought blank, and nothing above would notice.

        Asserted on what the pane SHOWS, captured from tmux: the `+8` that stood for the
        eight workspaces one row could not draw is gone, and the eight names are there.

        **Three rows and not two**, since #880: every workspace tab reserves
        `slots.TAB_COUNT_W` for its chat count, so 160 columns holds seven names a row and
        the fifteen need three. The property is the one it always was — the panel draws
        into every row its pane was given — and the number it takes is measured rather than
        assumed.
        """
        plane = make_plane(self)
        for name in NAMES:
            (Path(config.WORKSPACES_DIR) / name).mkdir(parents=True, exist_ok=True)
        fid = "real-strip.1"
        state.frame_dir(fid, create=True)
        state.record_workspace(fid, "harness-wrapper")

        # **The pane's own environment, stated with `-e` rather than inherited.** A tmux
        # pane is born out of the SERVER's environment, not the client's, and `PYTHONPATH`
        # is in no `update-environment` list — so a panel started without these two either
        # imports the installed charter or resolves the developer's real plane, and this
        # case would be measuring neither the tree it is in nor the plane it built.
        existing = os.environ.get("PYTHONPATH", "")
        pythonpath = os.pathsep.join([str(_REPO_ROOT)]
                                     + ([existing] if existing else []))

        r = _tmux("new-session", "-d", "-s", "real-strip", "-x", "160", "-y", "30",
                  "-P", "-F", "#{pane_id}", "--", "sleep", "600")
        self.assertEqual(r.returncode, 0, r.stderr)
        harness = r.stdout.strip()
        env = dict(os.environ, CHARTER_ROOT=str(plane), PYTHONPATH=pythonpath)
        env.pop("CHARTER_HOME", None)
        env.pop("CHARTER_WORKSPACE", None)
        r = _tmux("split-window", "-t", harness, "-v", "-b", "-l", "1",
                  "-e", f"CHARTER_ROOT={plane}", "-e", f"PYTHONPATH={pythonpath}",
                  "-P", "-F", "#{pane_id}", "--",
                  *layout.panel_command(slot="workspaces", session=fid), env=env)
        self.assertEqual(r.returncode, 0, r.stderr)
        pane = r.stdout.strip()

        def shown():
            return _tmux("capture-pane", "-p", "-t", pane).stdout

        self.assertTrue(_await(lambda: "harness-wrapper" in shown()),
                        f"the panel never drew its strip: {shown()!r}")
        one = shown()
        self.assertIn("+8", one, f"160 columns drew every name on one row: {one!r}")

        self.assertEqual(_tmux("resize-pane", "-t", pane, "-y", "3").returncode, 0)
        self.assertTrue(_await(lambda: "+8" not in shown()),
                        f"the panel kept one row's worth of names in a three-row pane: "
                        f"{shown()!r}")
        grown = shown()
        for name in NAMES:
            self.assertIn(name, grown, f"{name} is on no row: {grown!r}")
        self.assertEqual(len([ln for ln in grown.splitlines() if ln.strip()]), 3, grown)

    def test_charters_own_resize_is_never_read_as_a_gesture(self):
        """**The freeze, asked of the server** (#903). charter asserts every strip and
        then the harness, in one chain, and `resize-pane -y` moves one boundary at a time
        — so whether the strips are still where they were put once the harness has been
        asserted is tmux's answer and not charter's. If they are not, every resize adopts
        a height nobody chose and the strip stops following its names for the life of the
        frame, which is a worse defect than the one being fixed and one a drag cannot
        undo.
        """
        harness, panes = self._window("no-drag", 120, 40)
        fid = self._pass("no-drag", harness, panes, 120, 40)
        for _ in range(3):
            self._pass("no-drag", harness, panes, 120, 40)
        self.assertIsNone(state.bar_rows(fid),
                          "charter adopted its own resize as the operator's choice")
        self.assertEqual(self._height(panes["workspaces"]), 1)

    def test_a_real_drag_of_the_divider_is_adopted_and_kept(self):
        """The gesture, end to end on a real server: charter states an intent, a hand
        moves the pane, and the next pass finds the strip somewhere charter did not put it
        and takes that as this frame's choice.

        Three rows because these fifteen names need three at 120 columns — what is adopted
        is a CEILING, exactly as `F3`'s is, so a drag on a plane whose names fit on one row
        would give the rows straight back.
        """
        harness, panes = self._window("real-drag", 120, 40)
        fid = self._pass("real-drag", harness, panes, 120, 40)
        self.assertEqual(self._height(panes["workspaces"]), 1)
        self.assertIsNone(state.bar_rows(fid))
        # The operator's hand: `resize-pane -y` is what a drag on the divider performs,
        # and tmux exposes no other way to say it.
        self.assertEqual(_tmux("resize-pane", "-t", panes["workspaces"],
                               "-y", "3").returncode, 0)
        self.assertEqual(self._height(panes["workspaces"]), 3,
                         "the drag this case is about did not move the pane")
        self._pass("real-drag", harness, panes, 120, 40)
        self.assertEqual(state.bar_rows(fid), 3,
                         "the drag was recomputed away instead of adopted")
        self.assertEqual(self._height(panes["workspaces"]), 3,
                         "the adopted height did not reach the pane")

    def test_the_drag_survives_the_layout_pass_that_used_to_undo_it(self):
        """*"when switching between workspaces — resized tabs resetting to one line."* A
        workspace switch re-lays the frame out, which is this call again — so the report's
        own sequence is a drag followed by more passes, and the strip has to be three rows
        at the end of them."""
        harness, panes = self._window("drag-holds", 120, 40)
        fid = self._pass("drag-holds", harness, panes, 120, 40)
        _tmux("resize-pane", "-t", panes["workspaces"], "-y", "3")
        for _ in range(3):
            self._pass("drag-holds", harness, panes, 120, 40)
        self.assertEqual(state.bar_rows(fid), 3)
        self.assertEqual(self._height(panes["workspaces"]), 3)

    def test_a_short_window_keeps_its_harness_and_the_strip_stays_one_row(self):
        """The bound, against the server that would otherwise grant it. A 22-row window
        has nothing above `layout.HARNESS_MIN_ROWS` to spend, so the strip stays where it
        is — measured here rather than argued, because tmux does not refuse an over-large
        `resize-pane -y`: it takes the rows out of the neighbour."""
        harness, panes = self._window("short-window", 120, 22)
        self._reassert("short-window", harness, panes, 120, 22)
        self.assertEqual(self._height(panes["workspaces"]), 1)
        self.assertGreaterEqual(self._height(harness), 12)


if __name__ == "__main__":
    unittest.main()
