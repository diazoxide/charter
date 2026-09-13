---
version: unreleased
headline: A charter started with `GIT_DIR` exported reads the repository it names, not the one the variable points at
---

git reads `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and `GIT_COMMON_DIR` whatever
`git -C <path>` says. git exports `GIT_DIR` inside every hook it runs, and an `export` left
in a shell does the same. So a charter started that way answered many of its own git
questions about that other repository. `git rev-parse --show-toplevel` still named the right
directory, so nothing looked wrong. Measured with `GIT_DIR` pointing at one repository and
charter asked about another:

- the status line called a clean clone dirty, because it read the other repository's index
  against this clone's files;
- a workspace worktree's base branch, the plane's HEAD for the session nudge, and the
  `origin` the CI column asks a forge about all came from the other repository.

Charter now withholds those four variables from every git it runs. `GIT_CONFIG_*` still
reaches git, so `GIT_CONFIG_COUNT` and `GIT_CONFIG_GLOBAL` keep working. Nothing to adopt.
