### Changed

- **Claude Code no longer asks before listing a chat's own tasks.** The `dispatch_list` tool
  runs without the harness's permission prompt, as the same list's command line already did.
  It lists only the tasks that chat dispatched, or that you started from its tab, and a helper
  sub-agent's call of it is still refused (ADR 0064, amended, #1463).
