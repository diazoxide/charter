### Fixed

- **Removing a repo that was never cloned no longer leaves a folder behind unnamed.** When a
  workspace names a repo that is not cloned, but something is at its path (a plain folder, a
  folder whose `.git` is not a real one, a file or a link), removing the repo from the workspace
  is refused, says what is there, and keeps the repo's row (#1249).
