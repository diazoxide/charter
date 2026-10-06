### Added

- **A sandbox block shows on the chat's tab, and `purlis doctor` counts blocks.** When a
  sandboxed chat's sandbox blocks a command, the chat's tab now shows a notice saying what was
  blocked: the operation and the kind of path or host, such as "a write to the project's own
  files". It never shows the path, the command or its output. If purlis itself was the program
  the sandbox blocked, the notice says it is a purlis bug and offers **Report…**. That shows a
  draft naming only the operation, the kind of path and the versions, and the app files it under
  your own `gh` login only when you press **File report**. Nothing is sent before that.
  `purlis doctor` has a new `sandbox blocks` row with a count per operation over the last 7 days
  (#1338).
