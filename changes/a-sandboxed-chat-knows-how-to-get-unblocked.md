### Added

- **A sandboxed chat is told how to get unblocked under purlis.** At its start, every chat
  purlis runs in the sandbox is told what happens when the sandbox blocks a command: you see a
  Notice on its tab and choose, or you start the chat again with **Start without the
  sandbox**. It is also told that Claude Code's `/sandbox`, `/sandbox exclude` and
  `excludedCommands` do not apply under purlis, so it stops recommending them, and that a
  credential goes through `purlis secret exec` (#1342).
