### Fixed

- **An extension's theme name is held to a label's rule.** A colour or icon theme whose `name`
  is longer than 40 bytes or holds a control or invisible formatting character is left out, and
  the extension's other contributions still work. The approval and the extension's row in
  Extensions name the theme left out by its place, never by the name, as a view's or an
  action's title is. An extension that contributes nothing else is refused with that reason. A
  project's `[theme]` pick of such a name is not a pick (#1145).
