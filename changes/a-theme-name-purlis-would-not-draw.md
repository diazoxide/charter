### Fixed

- **An extension's theme name is held to a label's rule.** A colour or icon theme whose `name`
  is longer than 40 bytes or holds a control or invisible formatting character is refused, and
  the refusal names the theme by its place, as a view's or an action's title is (#1145).
