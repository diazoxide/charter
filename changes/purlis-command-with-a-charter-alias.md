### Changed

- **The command line is `purlis`, and `charter` still runs it.** The app ships `purlis` beside
  itself with `charter` as an alias for the rename's window, and **Install `charter` command in
  PATH** now links both names; the `.deb` installs both. Every guard that recognises charter's
  own command recognises `purlis` too (RN-3, #1255).
