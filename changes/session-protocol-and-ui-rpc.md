### Added

- **The session protocol, split from the UI RPC.** The link to `charterd` now has two protocols on
  one control lane. The session protocol is small, public and versioned: list, attach and detach,
  write, resize, answer, stop, start, and subscribe from a cursor, with events pushed after that.
  Its version is the one the link negotiates. A host refuses a command it does not know and keeps
  going, so a client and a host one version apart can still talk. A recorded frame of every kind
  holds that promise in CI. It names a project by a stable id, `[project] id` in `charter.toml`,
  minted once the first time it is asked for, and never by a path. Beside it, the UI RPC carries
  the app's own commands, with a typed TypeScript client generated from the same list as the
  window's. Only the app's window of the same build can use it, and it is left out of the
  compatibility tests. Nothing serves either protocol yet: `charter serve` will (FD-26, #663).

### Fixed

- **`charter persona default` no longer breaks a `charter.toml` it cannot edit cleanly.** When
  `[persona]` is written as an inline table, as a dotted key, or behind a header with a comment,
  setting or clearing the default persona used to add a second `[persona]`, which left a file
  nothing could read. It now refuses, says why, and leaves the file as it was (FD-26, #663).
