### Fixed

- **`purlis secret exec` finds the 1Password CLI when purlis was started from the Dock.** In a
  sandboxed chat the app reads the vault, and an app started from the Dock is given the
  system's short `PATH`, so it answered "the 1Password CLI ('op') is not on PATH" for an `op` in
  `/opt/homebrew/bin` that the chat's own shell found. A provider's program (`op`, and `vault`
  for a reference) is now looked for where purlis already looks for a harness and a forge's
  CLI: its own `PATH`, then `~/.local/bin` and the other directories installers use under your
  home, then Homebrew's and the system's. The refusal names every directory it searched, and
  `purlis doctor` has a row for each such program a project's vaults use, saying where it was
  found and warning where only this `PATH` finds it (#1516).

### Security

- **A provider's program a chat could have written is never run.** `op` and `vault` are handed
  a vault's identity, so purlis now refuses one that lies where a chat may write: the project, a
  folder you let chats write, the project's cache home, a harness's own folders, a temp
  directory, or the asking chat's own folder. The file is judged by where it really is, so a
  link to such a file is refused too. A copy elsewhere in the search is used; with none, the
  refusal names the file, nothing is run, and no pin is stored for it. The directories searched
  are those of the process that reads the vault, which for a sandboxed chat is the app, never
  the chat's (#1516).
