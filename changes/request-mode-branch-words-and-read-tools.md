### Changed

- **A Claude Code chat reads purlis's todos, memory, session records and change status without
  asking.** The five read-only tools of purlis's MCP server are pre-allowed for each chat the
  app starts, for that chat alone. Writing a todo or a memory, and `ask_operator`, still ask, and
  your own `ask` or `deny` rules still win (#1050).
- **The `pr` and `pr-merge` save modes are called request modes in purlis's messages and docs,
  on GitHub and GitLab alike.** `charter.toml` still takes `pr` and `pr-merge` (#1087).
- **The window calls a worktree its branch, and its directory the branch's folder.** The
  explorer and the command palette say *Merge branch …* and *Mark branch … done* by the branch's
  own name, and *Remove folder …*. The bottom bar counts branches. When purlis refuses to
  merge, remove, mark done or list one, the window says why in those words, with no
  command-line flag to run; `purlis worktree` keeps its own sentences (#989).
