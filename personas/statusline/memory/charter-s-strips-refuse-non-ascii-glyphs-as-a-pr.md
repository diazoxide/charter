# Charter's strips refuse non-ASCII glyphs as a property of the ROW, not o

_2026-09-07 18:45 · persistent_

Charter's strips refuse non-ASCII glyphs as a property of the ROW, not of any constant: a click is resolved by COLUMN, so a glyph a terminal may draw two cells wide moves every field after it. `−` U+2212 measures 1 by tui.width and is East-Asian Neutral and is STILL refused — a terminal font may not carry it and a fallback into a CJK face is two cells. slots.CLOSE_CHAT is ASCII `-`.
