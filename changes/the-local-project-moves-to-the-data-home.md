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
  its approval and pins go with it. One that something still works in (a terminal, an editor,
  another purlis) is left where it is: the app's log and the project's doctor say so, and the
  move is made at a launch when nothing works in it (#1670).
