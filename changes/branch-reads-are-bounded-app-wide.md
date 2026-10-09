### Fixed

- **Many branches whose reads hang no longer start a reader each.** purlis reads a branch's
  changes in a short-lived child process. At most six run at once now, across every window, and
  a read that waits for a place past its deadline says purlis is busy. When a window starts
  watching its branches, each is looked up on its own, so one that hangs no longer holds back
  the others (#1130).
