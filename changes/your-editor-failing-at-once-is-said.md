### Fixed

- **An editor that fails at once is said, not dropped.** With `$VISUAL` or `$EDITOR` chosen as
  your editor, a terminal editor such as vi or nano has no terminal to open in and exits at
  once, and *Open in your editor* used to do nothing. It now says that the editor exited, and
  the first line of what it printed, such as `emacsclient` saying it found no server. An editor
  that printed nothing is said to need a terminal. What the editor prints is kept in a file
  with no name, so an editor that stays open after purlis quits can go on printing. A file whose
  name is not UTF-8 is also handed to the editor as it is, rather than with its name rewritten
  (#1044).
