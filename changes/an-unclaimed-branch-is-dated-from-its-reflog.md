### Fixed

- **A branch whose chat did start is no longer marked `unclaimed`.** The explorer used to date a
  branch's cut from its folder's `.git` file, which `git worktree repair`, `git worktree move` and
  a copied plane rewrite, so a quiet branch could be marked `unclaimed` five minutes later. The
  cut is now read from the branch's own history, which those leave alone. When purlis cannot
  write down that a chat started on a branch, it leaves a mark on the branch itself, so that
  branch is not marked either (#835).
