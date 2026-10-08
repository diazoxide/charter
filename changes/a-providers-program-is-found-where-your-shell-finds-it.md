### Fixed

- **`purlis secret exec` finds the 1Password CLI when purlis was started from the Dock.** In a
  sandboxed chat the app reads the vault, and an app started from the Dock is given the
  system's short `PATH`, so it answered "the 1Password CLI ('op') is not on PATH" for an `op` in
  `/opt/homebrew/bin` that the chat's own shell found. A provider's program (`op`, and `vault`
  for a reference) is now looked for where purlis already looks for a harness and a forge's
  CLI: its own `PATH`, then `~/.local/bin` and the other directories installers use under your
  home, then Homebrew's and the system's. The refusal names every directory it searched, and
  `purlis doctor` has a row for each such program a project's vaults use, saying where it was
  found and whether only this `PATH` finds it (#1516).

### Security

- **A chat's environment does not choose a provider's program.** The directories searched are
  those of the process that reads the vault, which for a sandboxed chat is the app; a directory
  a chat may write (the project, a temp directory, a harness's own home) is searched after every
  other, wherever `PATH` names it; and the program is run by its absolute path, so the child's
  `PATH` chooses nothing (#1516).
