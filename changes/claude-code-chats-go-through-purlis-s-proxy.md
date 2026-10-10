### Changed

- **Claude Code chats now go through purlis's proxy, as Codex and opencode chats do.** Each
  Claude Code chat gets its own pair of proxy ports, HTTP and SOCKS5, and purlis names both in
  the settings it starts the chat with. Every host the chat reached before is reached the same
  way, and every connection is in the network record and on the chat's Network view. This needs
  Claude Code 2.1.285 or later. An older Claude Code keeps its own proxy and the same allowed
  hosts, and says so once.
- **A Claude Code chat can no longer connect to other local ports or listen on one**, even where
  your own Claude Code settings turn local binding on, so it reaches the network only through
  its own two ports. Codex and opencode chats already could not.
