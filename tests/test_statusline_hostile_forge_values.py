"""#326 end to end — a repo row drawn from hostile forge data.

The two demonstrations in the issue were a change "number" that was an escape sequence
and a clone directory name carrying an OSC string, both "rendered byte for byte" into
the status line. They enter by different doors: the change comes off a forge's JSON and
is guarded at the boundary (`glstate._change_or_none`), while the directory name comes
off the filesystem and never passes through `glstate` at all — so it is `tui` that has
to hold, and this is the test that says so.

The alignment assertion is the point of the whole issue, not a nicety: #326 measured a
rendered line at 94 columns "while the escape occupied none of them", which is the
column arithmetic every row below it inherits.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

from charter import statusline, tui

OSC_TITLE = "\x1b]0;pwned\x07"
ERASE_DISPLAY = "\x1b[2J"

#: Everything a terminal would act on. SGR is charter's own and is expected to be here,
#: so ESC (0x1b) is exempted from the control-character class and matched only by the
#: first branch, where the lookahead lets a colour escape past.
_NON_SGR_ESCAPE = re.compile(
    r"\x1b(?!\[[0-9;]*m)|[\x00-\x08\x0b-\x1a\x1c-\x1f\x7f-\x9f]")


def _row(label, change, d=Path("/tmp/plain-repo")):
    return statusline._tree_cells(
        "", label, d, states={}, branches={d: "main"},
        gl={d: {"ci": "success", "change": change, "sigil": "#"}},
    )


class AHostileValueCannotReachTheTerminal(unittest.TestCase):
    WIDTH = 100

    def _render(self, label, change):
        lines = _row(label, change).render(self.WIDTH)
        self.assertEqual(len(lines), 1, "a repo row is one line, always")
        return lines[0]

    def test_a_directory_name_carrying_an_osc_string_is_not_rendered(self):
        line = self._render(f"api{OSC_TITLE}", 7)
        # Precondition: the hostile label really is what was drawn, not a fallback.
        self.assertIn("api", line, "the label never reached the row — proves nothing")
        self.assertIsNone(_NON_SGR_ESCAPE.search(line), repr(line))

    def test_a_change_number_carrying_an_escape_is_not_rendered(self):
        line = self._render("api", f"{ERASE_DISPLAY}31")
        self.assertIsNone(_NON_SGR_ESCAPE.search(line), repr(line))

    def test_the_row_still_fits_its_width(self):
        line = self._render(f"api{OSC_TITLE}{ERASE_DISPLAY}", f"{OSC_TITLE}31")
        self.assertLessEqual(tui.width(line), self.WIDTH)

    def test_an_escape_costs_no_columns_so_the_row_below_stays_aligned(self):
        """The strongest form: a row drawn from a poisoned name is byte-identical to the
        one drawn from the same name with the escapes taken out. Nothing shifts."""
        self.assertEqual(self._render(f"api{OSC_TITLE}", 7), self._render("api", 7))

    def test_a_clean_row_is_unchanged(self):
        line = self._render("api", 7)
        self.assertIn("api", line)
        self.assertIn("#7", tui.strip_ansi(line))


if __name__ == "__main__":
    unittest.main()
