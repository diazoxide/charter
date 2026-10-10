### Fixed

- **The git identity fix takes a name written with a joiner.** A name with a family emoji or a
  flag (two emoji joined by a zero-width joiner), a Persian name written with a zero-width
  non-joiner between two letters, or a Hindi or other Indic name whose conjunct asks for a half
  form or a shown virama with either joiner, was refused as holding an invisible character. It
  is now written, and the confirmation shows the name as you typed it. A joiner anywhere else,
  and every other control, zero-width or direction-changing character, is still refused
  (#1301).
