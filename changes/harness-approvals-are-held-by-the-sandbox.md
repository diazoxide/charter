### Security

- **A sandboxed chat can no longer approve a harness or a persona's servers for you.** In a
  project that runs chats sandboxed, no chat may write the records of what you approved to run
  (a local profile's command, a project's harness declaration, or a persona's credentialed MCP
  servers), nor the project's `harnesses/` folder, wherever it stands, the project root
  included. This holds for Claude Code, Codex and opencode chats alike. You still approve in
  the window or with `purlis persona approve-mcp` in your terminal, and a chat can still read
  the records. So "approved" now means you approved it. `purlis persona approve-mcp` no longer
  says "Recorded" when it could not write the approval: it exits with an error and says
  nothing was approved (#1458).
