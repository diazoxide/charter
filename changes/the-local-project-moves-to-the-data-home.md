### Fixed

- **A sandboxed chat starts in this machine's local project, and so does the First task.** The
  local project the first run makes was kept in purlis's config home, which a chat's sandbox
  keeps it from writing because it holds your approvals, so every sandboxed chat there was
  refused before it started. It is made in purlis's data home now (`local-project`, beside the
  event log), and the config home stays out of every chat's reach (#1670).

### Changed

- **A local project in the config home moves to the data home at the next launch.** It moves in
  one rename, with a link left at the old place until its clones' worktrees, the chats it
  reopens and the projects this machine remembers all name the new one, so nothing is lost and
  its approval and pins go with it. A launch that stops halfway is finished by the next one. One
  that something still works in (a terminal, an editor, another purlis) is left where it is: the
  app's log and the project's doctor say so, and the move is made at a launch when nothing works
  in it. A worktree outside the project keeps the link, and the log names the
  `git worktree repair` to run (#1670).
- **A Claude Code conversation from a chat that ran in the old place does not come back by
  Resume.** Claude Code keeps a conversation under the folder its chat ran in, and that folder
  has a new name. The conversation's file is not touched (it stays in `~/.claude/projects/`);
  Resume starts the chat fresh with its session record in its briefing, as for any conversation
  the harness cannot find (#1670).
