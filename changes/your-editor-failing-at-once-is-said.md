### Fixed

- **An editor that fails at once is said, not dropped.** With `$VISUAL` or `$EDITOR` chosen as
  your editor, a terminal editor such as vi or nano has no terminal to open in and exits at
  once, and *Open in your editor* used to do nothing. It now says that the editor exited, and
  that a terminal editor needs a terminal. A file whose name is not UTF-8 is also handed to
  the editor as it is, rather than with its name rewritten (#1044).
