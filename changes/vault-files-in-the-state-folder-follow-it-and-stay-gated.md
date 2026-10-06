### Fixed

- **A vault file named by its full path in the project's state folder follows the folder too.**
  A plain-file or reference vault whose `file` is an absolute path in `.charter/` (a hand-edited
  or older record) now reads and writes where the folder is now, after `rename-local` moved it
  or an undo put it back, as a relative record already did (#1321).

### Security

- **A vault in the state folder is never reached through a link.** A plain-file or reference vault
  inside the project's state folder is now checked from the project down, as the vault registry
  is, so a state folder that is itself a link is refused for the vault in it: nothing is read
  from where it points and nothing is written there (#1321).
