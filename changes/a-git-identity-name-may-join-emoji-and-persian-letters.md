### Fixed

- **The git identity fix takes a name written with a joiner.** A name with a family emoji or a
  flag (two emoji joined by a zero-width joiner), or a Persian name written with a zero-width
  non-joiner between two letters, was refused as holding an invisible character. It is now
  written. A joiner anywhere else, and every other control, zero-width or direction-changing
  character, is still refused (#1301).
