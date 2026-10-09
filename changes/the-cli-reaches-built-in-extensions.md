### Added

- **The `purlis` command line reaches the app's built-in extensions.** The binary finds the app
  bundle it shipped in from its own real path, so a built-in's commands run from a terminal,
  and its badges, events and session-start section reach the footer and chats as an installed
  extension's do. A `purlis` outside an app bundle reaches none, and says so (#1366).
