### Fixed

- **A focused branch's count follows a commit that writes no file.** With the explorer focused on
  a branch, `git commit --amend --no-edit` or a commit of what was already staged moves its
  ahead and behind count at once, rather than at the next write in its folder: the branch's ref
  in the clone's git folder is watched too, for the focused branch only (#1152).
