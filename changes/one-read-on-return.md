### Changed

- **Coming back to the window asks git once.** A return both shows the window and gives it focus,
  and the save standing was read for each, so every return ran `git status` twice. The two now
  count as one return (#1392).
