### Added

- **Rename a harness profile and what uses it in one go.** In Settings, a profile that this
  machine's own `[harness] default` names can now be renamed: Rename first says which settings
  would change with it, and *Rename everywhere* renames the profile and them in one write, so
  nothing is left half-renamed. Undo renames both back. A default in the project's shared file
  is every teammate's, so it still stops the rename, naming it (#1380).
