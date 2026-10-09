### Changed

- **When git refuses to make or remove a branch's folder, the window says so in its own words.**
  What git printed (which could name a path inside the workspace, or carry git's own advice to
  force it) now stays in the terminal's `purlis worktree` message; the window names the branch
  and says that nothing was made or removed (#1102).
