# The frame strip's arrived mark (charter/frame/slots.py _ARRIVED_MARK) is

_2026-09-12 15:57 · persistent_

The frame strip's arrived mark (charter/frame/slots.py _ARRIVED_MARK) is '✶' U+2736, reused from TAB_SPINNER rather than newly chosen: the spinner table is this repo's only measurement of 'a strip glyph that is width 1 and East-Asian Neutral'. Picking a fourth glyph means a fourth measurement, and the two that shipped broken here (◈, ◆) were both picked without one. It rides _BAR_MARK's own cell — never a cell beside the name — so _cuts, bar_rows_wanted and the click map are byte-identical with and without it; verify by composing the same names at 200/120/80/60/40/24/12 and asserting equal tui.width per line AND an equal columns dict.
