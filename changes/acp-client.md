### Added

- **The groundwork for opencode chats over ACP.** charter's core now has an Agent Client
  Protocol client, tested against `opencode acp`. It starts the agent in the chat's worktree,
  hands the session charter's MCP server, and follows each turn's plan, usage, tool calls and
  reply in the same model it uses for hooks. It gives the agent no file system and no terminal
  of charter's, and it never answers a login prompt or supplies a credential. A permission the
  agent asks for waits, with no deadline, until you answer it; only you can answer it, and the
  first answer wins. Nothing in the window starts such a chat yet (HP-2, #669).

### Changed

- **The app leaves the terminal it was started from.** Started from a shell, the app no longer
  keeps that terminal as its controlling terminal, so no agent a chat runs can open it. Started
  straight from a shell, it runs again as its own child: the first process waits for it and
  ends as it ends, and if the first process is killed, the app ends with it. An app launched
  from a terminal now also ends when whatever launched it there exits, as Ctrl-C or closing the
  terminal ended it before (HP-2, #669).
