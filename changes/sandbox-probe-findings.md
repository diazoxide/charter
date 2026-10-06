### Added

- **`purlis secret list` with no vault lists the vaults.** It shows what `purlis vault list`
  shows, and says how to list one vault's keys. Before, it stopped with a usage error (#1345).
- **`purlis doctor` names vaults whose file is outside the project or gone.** A new `vault files`
  row lists each plain-file vault whose registered file is outside the project. It warns when a
  file is missing, gone or in a temp directory: each one still gets a deny rule in every
  sandboxed chat. The row names vaults and paths, never a value, and says how to remove a
  leftover (#1345).

### Fixed

- **`cargo` is on a chat's PATH.** A chat now also searches `~/.cargo/bin` and the folder that
  Homebrew's rustup uses, so `cargo` and `rustc` resolve as they do in a terminal (#1345).
- **A refused write names the file.** A purlis command used to print only "Operation not
  permitted (os error 1)". It now names the file it was writing. In a chat the app started
  sandboxed, it also says the chat's sandbox refused the write. It still exits non-zero (#1345).
- **Git on the project from a sandboxed workspace chat is explained.** `git add`, `git commit`,
  `git fetch` and other git commands that take a lock on the project's own repository would stop
  on a lock file. They are now refused before they run, with a sentence that the app saves and
  syncs the project, from the Save button or the Saving view where auto-save is off (#1345).
