### Fixed

- **A window forgets its focus on a branch that was removed.** Once the workspace's branches have
  been listed without it, the window stops remembering the branch it was focused on, so the next
  launch does not try to put it back. While the branches are still being listed, or when listing
  them failed, the focus is kept (#1152).
