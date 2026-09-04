"""Reverse video survives the trip from a terminal into an SVG.

`docs/assets/ansi2svg.py` understood colour, bold, dim and underline — every attribute
that says how a glyph LOOKS — and dropped the one that says which thing is CHOSEN. SGR 7
is how a terminal marks a selection, and charter's frame uses it for three: the active
workspace tab, the active chat tab, and the persona row (`frame/slots.py`). A renderer
that ignores it turns a frame with an obvious current tab into a frame with no current tab
at all — and the capture still looks fine, which is the whole problem. `docs/assets/
README.md`'s rule is that a screenshot must not "quietly drift from what charter actually
prints"; an attribute silently discarded on the way to the SVG is exactly that drift, with
nothing left in the file to notice it by.

**The mark is a filled cell, so it is asserted as one.** A `<rect>` behind the run and the
glyphs redrawn in the background colour is what a terminal does with SGR 7, and it is the
only thing here that paints a background at all — so "the run is highlighted" is
observable in the SVG as a rectangle that is not there without the escape. Every test
below renders the SAME text twice, once with the escape and once without, because "the
mark is drawn" is equally satisfied by a renderer that draws a rectangle over everything.
"""

from __future__ import annotations

import importlib.util
import re
import unittest
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent
_TOOL = _REPO / "docs" / "assets" / "ansi2svg.py"


def _ansi2svg():
    """`docs/assets/ansi2svg.py`, imported from where it lives.

    Loaded by path rather than imported: `docs/assets/` is not a package and never should
    be — nothing there ships, and a capture tool that needed charter's own import machinery
    would be one more thing between an operator and a regenerated asset.
    """
    spec = importlib.util.spec_from_file_location("ansi2svg_under_test", _TOOL)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


#: One `<rect>` as the renderer emits it — x, y, width and the fill, in that order.
_RECT = re.compile(r'<rect x="([\d.]+)" y="([\d.]+)" width="([\d.]+)" height="([\d.]+)" '
                   r'fill="(#[0-9a-f]{6})"/>')

#: The `fill` of a drawn run. The frame rect that surrounds the whole image carries a
#: `stroke` too, which is what keeps it out of both of these.
_TEXT_FILL = re.compile(r'<text [^>]*fill="(#[0-9a-f]{6})"[^>]*>([^<]*)</text>')


class TheChosenRowIsDrawnAsChosen(unittest.TestCase):
    def setUp(self):
        self.tool = _ansi2svg()

    def _svg(self, text: str) -> str:
        return self.tool.render(self.tool.parse(text), None)

    def _rects(self, text: str):
        return _RECT.findall(self._svg(text))

    def test_a_plain_run_paints_no_cell(self):
        """The control. Without it, "reverse fills a cell" is satisfied by a renderer that
        fills one behind every run it draws."""
        self.assertEqual(self._rects("*billing-migration"), [],
                         "something was already painting a background")

    def test_a_reversed_run_fills_the_cells_it_covers(self):
        rects = self._rects("\x1b[7m*billing-migration\x1b[0m")
        self.assertEqual(len(rects), 1, f"expected one filled run, got {rects}")
        width = float(rects[0][2])
        self.assertAlmostEqual(width, len("*billing-migration") * self.tool.CHAR_W, places=2,
                               msg="the fill does not cover the run it marks")

    def test_the_fill_reaches_the_whitespace_inside_the_mark(self):
        """A tab is `*billing-migration (1)  ` — trailing spaces and all — and a highlight
        that stops at the last word is a ragged one. The glyphs are still positioned
        individually (whitespace is never drawn); it is the RECT that has to know the run
        is wider than its words."""
        run = "*billing-migration (1)  "
        rects = self._rects(f"\x1b[7m{run}\x1b[0m")
        self.assertAlmostEqual(float(rects[0][2]), len(run) * self.tool.CHAR_W, places=2)

    def test_the_glyphs_are_redrawn_in_the_background_colour(self):
        """Reverse swaps the two colours; drawing the text in its ordinary foreground over
        a filled cell would leave it invisible rather than marked."""
        plain = dict((t, f) for f, t in _TEXT_FILL.findall(self._svg("devops")))
        marked = dict((t, f) for f, t in _TEXT_FILL.findall(self._svg("\x1b[7mdevops\x1b[0m")))
        self.assertEqual(plain["devops"], self.tool.FG)
        self.assertEqual(marked["devops"], self.tool.BG)

    def test_a_colour_under_the_mark_becomes_the_fill(self):
        """`ESC[32m ESC[7m` is a green cell with dark text, not green text on a light cell
        — which is what a terminal does and what the sidebar's own rows rely on."""
        rects = self._rects("\x1b[32m\x1b[7mpassed\x1b[0m")
        self.assertEqual(rects[0][4], self.tool.PALETTE[32])

    def test_the_mark_ends_where_the_terminal_ends_it(self):
        """SGR 27 turns reverse off on its own, without resetting colour with it — and a
        renderer that only understood `ESC[0m` would run the highlight to the end of the
        line."""
        rects = self._rects("\x1b[7mon\x1b[27moff\x1b[0m")
        self.assertEqual(len(rects), 1, f"the mark did not stop at ESC[27m: {rects}")
        self.assertAlmostEqual(float(rects[0][2]), 2 * self.tool.CHAR_W, places=2)

    def test_a_reset_clears_it_too(self):
        rects = self._rects("\x1b[7mon\x1b[0moff")
        self.assertEqual(len(rects), 1, f"the mark survived a reset: {rects}")

    def test_the_fill_sits_on_the_row_it_marks(self):
        """A cell fill is a row's full height at the row's own top — a rect drawn from the
        text baseline would sit under the line instead of behind it."""
        rects = self._rects("first\n\x1b[7msecond\x1b[0m")
        y, height = float(rects[0][1]), float(rects[0][3])
        self.assertAlmostEqual(height, self.tool.LINE_H, places=2)
        self.assertAlmostEqual(y, self.tool.PAD_Y + self.tool.LINE_H, places=2)


if __name__ == "__main__":
    unittest.main()
