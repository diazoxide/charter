"""#326 — `tui` must measure and clamp only ITS OWN markup.

`_SGR` matched `\\x1b[…m` and nothing else, so an erase-in-display, an OSC title
sequence, a bare BEL or a backspace survived `strip_ansi`, were counted as visible
columns by `width()`, and were copied through verbatim by `truncate()`. The module's
one hard guarantee — "no rendered line ever exceeds the requested width" — was being
computed from a string the terminal does not render the way `tui` measured it.

The tension these tests pin down: `tui` *emits* SGR deliberately, so the fix must
remove everything that is not SGR and leave SGR alone. Both halves are asserted here;
a blanket strip would pass the first half and fail the second.
"""

from __future__ import annotations

import unittest

from charter import tui

#: The demonstrations named in #326, plus the C0 cases the same gap lets through.
ERASE_DISPLAY = "\x1b[2J"          # CSI, non-SGR final byte
OSC_TITLE = "\x1b]0;pwned\x07"     # OSC … BEL — repaints the terminal's title bar
CURSOR_HOME = "\x1b[H"             # CSI with no parameters at all
BEL = "\x07"
BACKSPACE = "\x08"
DEL = "\x7f"


class WidthCountsOnlyVisibleColumns(unittest.TestCase):
    """`width()` must not count a control character as a column.

    #326 measured a rendered line at 94 columns "while the escape occupied none of
    them" — the arithmetic every `Cell` and `Row` uses to stay aligned was reading a
    number the terminal disagreed with.
    """

    def test_a_non_sgr_csi_is_not_counted(self):
        self.assertEqual(tui.width(f"a{ERASE_DISPLAY}b"), 2)

    def test_a_parameterless_csi_is_not_counted(self):
        self.assertEqual(tui.width(f"a{CURSOR_HOME}b"), 2)

    def test_an_osc_sequence_is_not_counted(self):
        self.assertEqual(tui.width(f"a{OSC_TITLE}b"), 2)

    def test_bare_c0_controls_are_not_counted(self):
        self.assertEqual(tui.width(f"a{BEL}{BACKSPACE}{DEL}b"), 2)

    def test_sgr_colour_is_still_zero_width(self):
        """The pre-existing guarantee, restated so a broader strip cannot quietly
        change what SGR costs."""
        self.assertEqual(tui.width("\x1b[32mok\x1b[0m"), 2)


class SanitiseKeepsCharterSOwnMarkup(unittest.TestCase):
    """The half a blanket strip would break."""

    def test_sgr_survives_sanitising(self):
        self.assertEqual(tui.sanitize("\x1b[32mok\x1b[0m"), "\x1b[32mok\x1b[0m")

    def test_a_multi_parameter_sgr_survives(self):
        self.assertEqual(tui.sanitize("\x1b[1;38;5;208mx\x1b[0m"),
                         "\x1b[1;38;5;208mx\x1b[0m")

    def test_an_escape_that_is_not_sgr_is_removed(self):
        self.assertEqual(tui.sanitize(f"a{ERASE_DISPLAY}b"), "ab")

    def test_an_osc_string_is_removed_whole(self):
        """Removing only the introducer would leave `0;pwned` as visible text — worse
        than the escape, because it looks like data."""
        self.assertEqual(tui.sanitize(f"a{OSC_TITLE}b"), "ab")

    def test_an_unterminated_osc_string_is_still_removed(self):
        self.assertEqual(tui.sanitize("a\x1b]0;no-terminator"), "a")

    def test_a_lone_escape_is_removed_without_eating_the_text_after_it(self):
        self.assertEqual(tui.sanitize("a\x1bb"), "a")   # ESC b is a two-char escape
        self.assertEqual(tui.sanitize("ab\x1b"), "ab")

    def test_whitespace_controls_become_a_space_not_a_deletion(self):
        """A tab jumps to the next tab stop, which is the column-shearing this module
        exists to prevent; a newline breaks the one-line contract outright. Both keep
        their separation as a single, measurable space."""
        self.assertEqual(tui.sanitize("a\tb\nc"), "a b c")

    def test_c1_controls_are_removed(self):
        self.assertEqual(tui.sanitize("a\x9bb"), "ab")

    def test_ordinary_text_is_returned_unchanged(self):
        for s in ("", "plain", "wide 日本語", "non-breaking\xa0space", "…"):
            self.assertEqual(tui.sanitize(s), s, s)


class TruncateAndPadDoNotCopyControlsThrough(unittest.TestCase):
    def test_truncate_drops_a_non_sgr_escape_that_already_fits(self):
        """The fast path — `truncate` returns *s* unchanged when it fits — was the way
        an escape reached the screen without ever being looked at."""
        out = tui.truncate(f"a{ERASE_DISPLAY}b", 40)
        self.assertNotIn("\x1b", out)
        self.assertEqual(out, "ab")

    def test_truncate_drops_an_osc_when_it_also_has_to_cut(self):
        out = tui.truncate(f"{OSC_TITLE}abcdefghij", 4)
        self.assertNotIn("\x1b]", out)
        self.assertLessEqual(tui.width(out), 4)

    def test_truncate_still_carries_sgr_through_a_cut(self):
        out = tui.truncate("\x1b[32mabcdefghij\x1b[0m", 4)
        self.assertIn("\x1b[32m", out)
        self.assertLessEqual(tui.width(out), 4)

    def test_pad_produces_exactly_the_requested_columns(self):
        out = tui.pad(f"a{OSC_TITLE}b", 8)
        self.assertNotIn("\x1b", out)
        self.assertEqual(tui.width(out), 8)
        self.assertEqual(len(out), 8, "padded to 8 columns but not 8 characters")


class NoNodeRendersAControlCharacter(unittest.TestCase):
    """The module-level guarantee, asserted at the node boundary rather than the
    primitive — a natural-width `Cell` is copied into the row verbatim, so it is the
    one shape that could route round a fix applied only where padding happens."""

    HOSTILE = f"repo{OSC_TITLE}{ERASE_DISPLAY}{BEL}"

    def _assert_clean(self, lines, limit):
        for ln in lines:
            self.assertNotIn("\x1b]", ln)
            self.assertNotIn(ERASE_DISPLAY, ln)
            self.assertNotIn(BEL, ln)
            self.assertLessEqual(tui.width(ln), limit)

    def test_text(self):
        self._assert_clean(tui.Text(self.HOSTILE).render(40), 40)

    def test_row_with_a_fixed_width_cell(self):
        row = tui.Row(tui.Cell(self.HOSTILE, 12), tui.Cell("main", 10))
        self._assert_clean(row.render(40), 40)

    def test_row_with_a_natural_width_cell(self):
        row = tui.Row(tui.Cell(self.HOSTILE), tui.Cell("main", 10))
        self._assert_clean(row.render(40), 40)

    def test_columns(self):
        node = tui.Columns([(tui.Text(self.HOSTILE), 20), ([self.HOSTILE], None)])
        self._assert_clean(node.render(60), 60)


if __name__ == "__main__":
    unittest.main()
