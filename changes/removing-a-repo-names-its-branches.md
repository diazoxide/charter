### Changed

- **Removing a repo from a workspace names its branches when it refuses.** A repo whose
  branches have folders of their own is refused with each branch by name and the way out in
  the explorer, and a repo whose branches could not be read says so in the same words as the
  explorer's own refusals, with no "worktree" in them (#1102).
