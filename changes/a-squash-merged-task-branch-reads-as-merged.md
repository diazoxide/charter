### Fixed

- **A task's branch that was squash-merged reads as merged.** purlis used to keep its folder
  until you discarded it, because git finds none of its commits in the branch it was cut from.
  Where every file the branch changed reads there as the branch has it, its folder is taken
  away as a merged one's is; the branch itself stays, since it is the one place its commits
  are (#1472).
