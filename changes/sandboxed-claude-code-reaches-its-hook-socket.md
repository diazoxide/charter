### Fixed

- **A sandboxed Claude Code chat can reach purlis again.** A `purlis` command run from a
  sandboxed Claude Code chat could not connect to the app, so a handoff printed a command
  instead of opening its tab, and every write the app makes on a chat's behalf failed. The
  chat's sandbox now allows the one socket its own hooks report on, and no other socket. Codex
  and opencode chats already could. On Linux, Claude Code's sandbox cannot allow a single
  socket, so a sandboxed Claude Code chat there still reaches none (#1328).
