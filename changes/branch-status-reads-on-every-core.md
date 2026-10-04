### Changed

- **A branch's changed-file marks come back about three times sooner on a large repo.** The
  explorer reads what a branch changed after every write an agent makes. That read now compares
  the working tree on every core and looks for new files at the same time, as `git status` does:
  at 100,000 files it takes about 0.45 s instead of 1.4 s, close to `git status`'s 0.3 s, and it
  still starts no git process (#1153).
