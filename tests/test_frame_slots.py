"""Panels compose the renderers the status line already has.

Zones are not re-invented here: `statusline.py` already argues for identity in one place
and alerts in another, and a frame that split them differently would be a second layout to
keep in step with the first.

Width is measured, not theorised (the coordinator correction this task shipped under): a
panel process started as a tmux pane command inherits the *launching* shell's whole
environment, so `$COLUMNS` can describe a completely different rectangle than the pane
this process is actually drawing into. Measured against a real tmux 3.7c: a 22-column
pane, launched from a shell exporting `COLUMNS=200`, saw `COLUMNS='200'` in its own
environment. `Width` below pins that a panel lays out against its own tty instead.
"""

from __future__ import annotations

import os
import sys
import unittest
from unittest import mock

from charter import config, tui
from charter.frame import slots

from tests._isolation import PersonaIso


class Render(PersonaIso, unittest.TestCase):
    def test_top_names_the_workspace(self):
        out = slots.render("top", "f-1")
        self.assertTrue(out.strip())

    def test_bottom_renders(self):
        self.assertTrue(slots.render("bottom", "f-1").strip())

    def test_a_slot_never_exceeds_the_pane_width(self):
        """`tui.width` counts display cells, not characters — a wide glyph that fits by
        len() still wraps the pane and pushes the frame apart."""
        for slot in ("top", "bottom"):
            for line in slots.render(slot, "f-1").splitlines():
                with self.subTest(slot=slot):
                    self.assertLessEqual(tui.width(line), tui.term_width(default=80))

    def test_a_failing_renderer_yields_a_line_rather_than_an_exception(self):
        """A panel that raises leaves a hole in the frame; `statusline.render` makes the
        same promise for the same reason."""
        slots.SLOTS["boom"] = lambda fid: 1 / 0
        try:
            self.assertIn("charter", slots.render("boom", "f-1"))
        finally:
            del slots.SLOTS["boom"]

    def test_the_bottom_row_names_the_configured_hotkey_not_a_hardcoded_one(self):
        """`[frame] hotkey` is configurable and this row spelled `F2 menu` literally, so
        a plane on `hotkey = "F1"` had its own panel telling every operator the wrong
        key, on every repaint, forever.

        `F1` is chosen precisely because it is NOT the default: asserting against `F2`
        would pass against the hardcoded string this test exists to remove. The absence
        assertion is the one that fails on the mutation."""
        with mock.patch.dict(config.FRAME, {"hotkey": "F1"}):
            out = slots.render("bottom", "f-1")
        self.assertIn("F1 menu", out)
        self.assertNotIn("F2", out)

    def test_a_modifier_hotkey_reaches_the_panel_intact(self):
        """A second, differently-shaped value — `F1` alone could be satisfied by a
        one-character substitution. `M-m` shares no characters with `F2`."""
        with mock.patch.dict(config.FRAME, {"hotkey": "M-m"}):
            self.assertIn("M-m menu", slots.render("bottom", "f-1"))

    def test_an_unknown_slot_is_named_rather_than_drawn_blank(self):
        """`panel.run` (Task 7) refuses an unknown slot before ever spawning a pane for
        it — but `render` is the one place that can explain *why*, so it must not answer
        an unknown name with silence either."""
        out = slots.render("sideways", "f-1")
        self.assertIn("sideways", out)


class Unimplemented(unittest.TestCase):
    """Which configured slots charter sizes but cannot draw — asked in one place because
    three callers need the same answer and must not drift: `cmd_launch` (to skip
    splitting a pane that would be permanently dead under `remain-on-exit on`),
    `frame_ready` (`--probe`) and `doctor.check_frame`."""

    def test_the_two_sized_but_unrendered_slots_are_named(self):
        self.assertEqual(slots.unimplemented(["top", "left", "bottom", "right"]),
                         ["left", "right"])

    def test_an_all_implemented_configuration_names_nothing(self):
        self.assertEqual(slots.unimplemented(["top", "bottom"]), [])

    def test_the_answer_comes_from_the_registry_not_a_hardcoded_pair(self):
        """`left`/`right` are today's answer, not the rule. A renderer landing for one
        of them must take it off this list without anybody remembering to edit a
        literal — so the registry is patched and the answer must follow."""
        with mock.patch.dict(slots.SLOTS, {"left": lambda fid: "drawn"}):
            self.assertEqual(slots.unimplemented(["top", "left", "right"]), ["right"])


class Width(unittest.TestCase):
    """Pins the coordinator correction directly against `_width`, independent of
    whatever `_top`/`_bottom` happen to render — so a future change to panel *content*
    can never accidentally paper over a regression in *how much room it thinks it has*.
    """

    def test_width_measures_the_panes_own_tty_ignoring_columns(self):
        with mock.patch.dict(os.environ, {"COLUMNS": "200"}), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((22, 5))):
            self.assertEqual(slots._width(), 22)

    def test_width_falls_back_to_env_first_term_width_when_no_tty_is_available(self):
        """The one case `tui.term_width()` is allowed to answer: no tty behind the fd at
        all (stdout piped to a file, say), not merely a pane that disagrees with
        `$COLUMNS`."""
        with mock.patch.dict(os.environ, {"COLUMNS": "55"}), \
             mock.patch("os.get_terminal_size", side_effect=OSError("not a tty")):
            self.assertEqual(slots._width(), 55)


class RenderFollowsThePane(PersonaIso, unittest.TestCase):
    """The end-to-end version of `Width`, above: what a real panel process would see
    with a launching shell's `COLUMNS` still in its environment and its own pane far
    narrower. `PersonaIso` redirects `sys.stdout` to a `StringIO`, so `fileno()` is
    patched back to something callable rather than left to raise — the isolation harness
    would otherwise force every test onto the fallback branch and hide exactly the bug
    this class exists to catch.
    """

    def test_render_wraps_to_the_pane_not_to_the_wider_columns_value(self):
        with mock.patch.dict(os.environ, {"COLUMNS": "200"}), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((10, 3))):
            for slot in ("top", "bottom"):
                out = slots.render(slot, "f-1")
                with self.subTest(slot=slot):
                    for line in out.splitlines():
                        self.assertLessEqual(tui.width(line), 10)


class WideGlyphs(PersonaIso, unittest.TestCase):
    """`tui.width` counts display cells, not `len()` — every other test in this file
    uses pure-ASCII content, where the two coincide, so none of them would notice a
    future edit that swapped `tui.truncate` for character slicing (`x[:w]`). Caught in
    review by a hand-built probe: a workspace name of CJK glyphs measuring 57 display
    cells rendered untouched into a 30-cell pane. This pins the same shape with an
    assertion, not a probe: 30 CJK characters are 60 display cells (two each) but only
    30 *characters* — half the false margin `len()`-based slicing would report as safe
    against a pane this narrow.
    """

    def test_a_cjk_workspace_name_still_fits_a_narrow_pane(self):
        cjk = "測" * 30  # 30 characters, 60 display cells
        with mock.patch.dict(os.environ, {"CHARTER_WORKSPACE": cjk}), \
             mock.patch.object(sys.stdout, "fileno", return_value=1, create=True), \
             mock.patch("os.get_terminal_size",
                         return_value=os.terminal_size((20, 3))):
            line = slots.render("top", "f-1")
        self.assertLessEqual(tui.width(line), 20)


if __name__ == "__main__":
    unittest.main()
