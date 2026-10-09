### Fixed

- **`purlis migrate` makes git ignore the right folder when a project's path holds `*`, `?`,
  `[` or `\`.** The line it adds to the repository's `info/exclude` matches that path and no
  other (#1285).
