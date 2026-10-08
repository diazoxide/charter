### Security

- **A sandboxed chat can no longer approve a harness for you.** In a project that runs chats
  sandboxed, no chat may write the records of what you approved to run, a local profile's
  command or a project's harness declaration, nor the project's `harnesses/` folder, wherever
  it stands, the project root included. This holds for Claude Code, Codex and opencode chats
  alike. The app still writes your approval when you give it in the window, and a chat can
  still read both. So "approved" now means you approved it (#1458).
