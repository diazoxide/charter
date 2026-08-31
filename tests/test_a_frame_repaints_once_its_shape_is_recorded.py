"""#748: a launch writes the frame's shape down, so it moves the frame's version too.

**The defect this closes was permanent and it was the operator's oldest live
complaint.** #530 stopped the top bar repeating the persona roster the sidebar already
draws, by asking `slots._sidebar_live` — `"right" in state.panes(fid)` — live on every
repaint. That record is written by `commands_frame._draw_panels`, and it is written
*after* every `split-window`, because the ids in it are what those splits returned. A
panel is a `charter panel` PROCESS in a pane, and `top` is split FIRST: it can therefore
be up and painting before the launcher has finished carving the other three panes and
written the file. When it is, `state.panes` answers `{}`, `_sidebar_live` answers its
documented-safe `False`, and the roster is drawn on a screen whose sidebar is drawing it
too.

That would be a flicker if anything told the panel to look again. Nothing did. `top` is
not in `slots.ANIMATED`, so it repaints on a version bump and on nothing else, and the
launch's own bump happens *before* the splits (`cmd_launch`), so the only bump left that
could land after the record is the detached gather's — which is deliberately racing the
frame's own construction and just as often lands first. **Measured on this branch's
parent over 90 real launches at 200x50 on tmux 3.7c: 16 of them (17.8%) kept the
duplicated roster for the life of the frame.**

**The fix is the ordering one #748 and #774 both ask for, made local.** `_draw_panels`
bumps the frame immediately after `state.record_panes`, so writing the shape down is an
EVENT rather than a silent file. A panel that painted before the record now has a reason
to paint again, and its second paint reads the shape. `cmd_density` has recorded and then
bumped since #387 for exactly this reason — the launch path is the one that recorded
without saying so.

Two classes, because there are two properties and either alone is satisfied by a wrong
change. :class:`RecordingTheShapeMovesTheVersion` pins the ordering in the launcher, where
a bump placed before the record would look identical to a reader and close nothing.
:class:`APanelThatLostTheRaceIsToldToLookAgain` pins what the operator sees, through the
real `panel._tick` decision and the real `slots.render` — so the roster is asserted on
bytes charter would have put in the pane, not on a stubbed predicate.
"""

from __future__ import annotations

import os
import subprocess
import sys
import unittest
from unittest import mock

from charter import commands_frame, persona, tui
from charter.frame import panel, slots, state
from tests._isolation import PersonaIso

#: The roster the sidebar already draws — what must not also be on the identity row.
ROSTER = "◇ personas"
#: The half of the row that is IDENTITY rather than a list, and must survive either way.
ACTIVE = "◆ steward"


def _a_dead_pid() -> int:
    """A real pid that has exited and been reaped — the suite's own idiom, repeated
    rather than imported because a test module reaching into another test module's
    private helper couples two files that are otherwise independent. A hand-written `-1`
    would read as `launchd`, which never exits."""
    p = subprocess.Popen([sys.executable, "-c", "pass"])
    p.wait()
    return p.pid


class _Tmux:
    """A stand-in for `tmuxctl.run` big enough for one `_draw_panels` and no bigger.

    `list-panes` answers empty — a window that holds the harness pane and nothing else,
    which is what both launch paths hand `_reconcile_panels` — and each `split-window`
    answers the next pane id. Everything else succeeds silently, which is all
    `_dress_window` and `_install_resize_hook` need to be asked for here.
    """

    def __init__(self, new_panes=("%11", "%12", "%13", "%14")):
        self.new_panes = list(new_panes)
        self.calls: list[list[str]] = []

    def __call__(self, action, argv, *, env=None, timeout=None, report=True):
        self.calls.append(list(argv))
        out = ""
        if "display-message" in argv:
            out = "200:50"
        elif "split-window" in argv:
            out = self.new_panes.pop(0) if self.new_panes else ""
        return subprocess.CompletedProcess(argv, 0, stdout=out, stderr="")


class _LaunchFixture(PersonaIso):
    """A plane with a roster to duplicate, and the one launcher call that records a
    shape. The personas are real files rather than a patched `_persona_line_parts`,
    because what #530 conditions is the row `statusline` really builds."""

    #: Names carried purely so a hit in an assertion is a roster charter drew, never the
    #: fixture's own vocabulary.
    OTHERS = ("forge", "release")
    SLOTS = ["top", "bottom", "right"]

    def setUp(self):
        super().setUp()
        self.fid = f"race-{_a_dead_pid()}"
        self.make_persona("steward")
        for n in self.OTHERS:
            self.make_persona(n)
        persona.set_active("steward")

    def draw_panels(self, fake=None):
        """One real `commands_frame._draw_panels` — the launcher's own funnel, both
        launch paths' only way to record a shape."""
        fake = fake or _Tmux()
        with mock.patch("charter.frame.tmuxctl.run", side_effect=fake):
            return commands_frame._draw_panels(
                "charter", slots=self.SLOTS, fid=self.fid, harness_pane="%0",
                env=None, v=(3, 7))


class RecordingTheShapeMovesTheVersion(_LaunchFixture, unittest.TestCase):
    """The launcher half: writing the shape down is an event, and it is one at the right
    moment."""

    def test_a_launch_bumps_the_frame_after_it_has_recorded_the_panes(self):
        """Without this the record is a file nothing is watching. `top` repaints on a
        version bump and on nothing else, so a shape written with no bump behind it is a
        shape every panel that painted first will never read."""
        state.bump(self.fid)          # the launch's own bump, before any split
        before = state.version(self.fid)
        self.draw_panels()
        self.assertEqual({"top": "%11", "bottom": "%12", "right": "%13"},
                         state.panes(self.fid))
        self.assertNotEqual(before, state.version(self.fid),
                            "the launch recorded the frame's shape and left the version "
                            "where it was, so nothing that had already painted has any "
                            "reason to read it")

    def test_the_bump_lands_after_the_write_and_not_before_it(self):
        """The order is the whole property, and a bump on the wrong side of the write
        reads identically in a diff. What a panel woken by the bump then reads is the
        file as it stood when the bump was made — so at that moment the shape must
        already be on disk, whole.

        `notify.plane_changed` keeps this same order for this same reason ("a poller
        that saw the new version must never then read the old cache"), and
        `cmd_density` records before it bumps. This is the launch path saying so too.
        """
        seen_at_bump: list[dict] = []
        real_bump = state.bump

        def _watch_bump(fid):
            seen_at_bump.append(state.panes(fid))
            real_bump(fid)

        with mock.patch.object(state, "bump", _watch_bump):
            self.draw_panels()
        self.assertTrue(seen_at_bump,
                        "`_draw_panels` never bumped the frame at all")
        self.assertEqual({"top": "%11", "bottom": "%12", "right": "%13"},
                         seen_at_bump[-1],
                         "the frame was bumped before its shape was on disk, so a panel "
                         "woken by that bump reads the same empty record it already had")


class APanelThatLostTheRaceIsToldToLookAgain(_LaunchFixture, unittest.TestCase):
    """The operator half, driven through the real repaint decision and the real renderer.

    `panel._tick` is `panel._watch`'s one iteration — the function that decides "paint
    now, or wait" — so a test that calls it twice around a launch is asking exactly what
    a live `top` panel asks, without a `while True` or a real clock.
    """

    def _render(self, slot, fid, *, cols=200) -> str:
        with mock.patch("os.get_terminal_size",
                        return_value=os.terminal_size((cols, 3))), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True):
            return tui.strip_ansi(slots.render(slot, fid))

    def test_the_top_bar_drops_the_roster_it_drew_before_the_shape_was_recorded(self):
        """The whole of #748, in the order it happens.

        `top` is the FIRST pane split, so its process can be painting while the launcher
        is still carving `bottom` and `right`; `state.panes` is empty until every split
        has answered. The roster on that first paint is correct — charter genuinely
        cannot tell yet, and `_sidebar_live`'s False is the safe direction for a frame
        with no sidebar. What was wrong was that it stood: nothing moved the version
        afterwards, and `top` is not in `slots.ANIMATED`.
        """
        state.bump(self.fid)
        painted: list[str] = []
        resized = {"flag": True}      # `_watch`'s own first pass: paint, resized or not

        def _paint(slot, fid):
            painted.append(self._render(slot, fid))

        seen = panel._tick(resized, "", "top", self.fid, paint=_paint)
        self.assertEqual({}, state.panes(self.fid))
        self.assertIn(ROSTER, painted[0],
                      "the fixture never reproduced the losing paint at all")

        # The launcher finishes carving the frame and writes the shape down.
        self.draw_panels()

        panel._tick(resized, seen, "top", self.fid, paint=_paint)
        self.assertEqual(2, len(painted),
                         "the panel was never told to look again, so the duplicated "
                         "roster stands for the life of the frame — #748")
        self.assertNotIn(ROSTER, painted[1], painted[1])
        for n in self.OTHERS:
            self.assertNotIn(n, painted[1],
                             f"`{n}` is on the top bar and in the sidebar: {painted[1]!r}")

    def test_the_active_persona_survives_the_repaint_that_drops_the_roster(self):
        """The half that must never go with it. The roster is a LIST the sidebar holds
        better; `◆ <active>` is identity, and this row is where "who am I being" is read.
        Without this, a fix that simply blanked the persona half of the row would pass
        the test above."""
        state.bump(self.fid)
        painted: list[str] = []
        resized = {"flag": True}

        def _paint(slot, fid):
            painted.append(self._render(slot, fid))

        seen = panel._tick(resized, "", "top", self.fid, paint=_paint)
        self.draw_panels()
        panel._tick(resized, seen, "top", self.fid, paint=_paint)
        self.assertEqual(2, len(painted), "there was no repaint to inspect")
        self.assertIn(ACTIVE, painted[-1], painted[-1])

    def test_a_frame_whose_launch_recorded_no_sidebar_keeps_its_roster(self):
        """The other direction, which the bump must not quietly take away. A terminal too
        narrow for `right` (`layout.visible_slots` drops it first on any shortage) makes
        the top bar the plane's only roster — so the repaint the fix adds has to arrive at
        the same answer the frame already had, not at a blank half-row."""
        self.SLOTS = ["top", "bottom"]
        state.bump(self.fid)
        painted: list[str] = []
        resized = {"flag": True}

        def _paint(slot, fid):
            painted.append(self._render(slot, fid, cols=60))

        seen = panel._tick(resized, "", "top", self.fid, paint=_paint)
        self.draw_panels()
        panel._tick(resized, seen, "top", self.fid, paint=_paint)
        self.assertEqual({"top": "%11", "bottom": "%12"}, state.panes(self.fid))
        self.assertEqual(2, len(painted), "there was no repaint to inspect")
        self.assertIn(ROSTER, painted[-1], painted[-1])
        for n in self.OTHERS:
            self.assertIn(n, painted[-1], painted[-1])
