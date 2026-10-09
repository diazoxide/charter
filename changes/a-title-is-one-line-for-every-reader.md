### Fixed

- **A memory's title is one line for every reader.** The line and paragraph separators
  (U+2028, U+2029) and the direction overrides are folded to a space wherever a title is
  written, as a newline already was, and a chat's brokered write of a title holding one is
  refused (#1408).
