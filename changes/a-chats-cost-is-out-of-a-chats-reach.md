### Security

- **What a chat's harness says its session has cost is kept where no sandboxed chat can
  write it.** The token and cost figures purlis shows (a dispatch's *Cost (reported)*, a tab's
  tasks' tokens, a session's tokens against its limit) are now kept by each chat's own id in
  the app's folder of the project's state, which every harness's sandbox denies a chat writing.
  Claude Code's status line, which runs outside the sandbox, is the one writer. Before, the
  figure sat in a folder a chat writes, so a chat could change its own figure or another's
  (#1457).
