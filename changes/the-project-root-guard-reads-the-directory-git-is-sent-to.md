### Fixed

- **The project-root branch guard reads `~` where git is sent.** `git -C ~/clone switch -c x`
  and the same `~` in `--git-dir`, `--work-tree`, `GIT_DIR`, `GIT_WORK_TREE` or `env -C` now
  act in the home directory, as the shell sends them, instead of being refused as the project
  root. A `~` the shell leaves alone (quoted, or in an attached option) is still read as a
  directory named `~` (#1323).
- **A directory the guard cannot name gets its own refusal.** When git is sent to a directory
  only the shell can name (`git -C "$DIR"`, `cd "$DIR" && git …`, a glob, `cd -`), the refusal
  now says so and asks for the path spelled out, instead of claiming what the command would do
  in the project root. That case is refused from any directory, including a workspace clone
  (#1323).
- **Naming the project root's own git directory is the project root.** For a root whose git
  directory lives beside it, `--git-dir=<it>` or `GIT_DIR=<it>` is now refused like the root
  itself (#1323).
