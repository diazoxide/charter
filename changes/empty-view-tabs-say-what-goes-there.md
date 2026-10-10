### Changed

- **An empty view tab says what goes there.** The Saving tab with no saves yet, a chat's Network
  view that reaches no host or was refused nothing, a task's Changes with no changed file, and the
  repo instructions tab when the workspace's repos hold none now say what is missing and what
  would be listed, in the same empty state as the other tabs. The repo instructions tab names the
  files it looks for and has Read again (#626, #614).

### Fixed

- **The harness setup tab no longer says no harness is installed when it could not look.** A look
  at the machine that failed now says so, with the reason and Check again (#626).
