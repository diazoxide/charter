### Added

- **Database clients and ssh reach allowed hosts from a sandboxed chat.** `purlis secret exec`
  points a vault's connection string (a database URL, a libpq `host=… port=…` string, or
  `host:port`) at a tunnel to exactly that host and port when the chat may reach it, so `psql`
  and `usql` work with the command unchanged; a host it may not reach raises a Block with Allow
  instead of failing at the name lookup. git over ssh, and a bare `ssh` in a Codex or opencode chat, go through the chat's SOCKS port.
  Every tunnelled connection is in the network record (#1667).
