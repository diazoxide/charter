### Added

- **Settings › Sandbox › Granted lists every grant, each with Revoke.** It shows what was
  allowed past the project's sandbox at every level (one chat, me on this machine, everyone in
  this project), who granted it and when, and the chat it came from. Revoke takes it out of
  every later start and is recorded; revoking a project host is an ordinary change to
  `charter.toml` that teammates follow. The same page lists the folders outside the project
  that chats may be granted, such as a tool's cache, kept on this machine only; it warns when
  one holds what other programs load code from, and drops one that comes to lead elsewhere
  through a link (#1348).
