### Fixed

- **A command substitution in git's options no longer hides the command from the project-root
  guard.** In `git -C $(git rev-parse --show-toplevel) checkout -b x`, and the same with
  backticks, `--git-dir`, `--work-tree`, `GIT_DIR=$(…)` or `env -C $(…)`, the guard used to
  read the substitution's words as git's own and miss the branch move or reset after them. It
  now refuses those, an alias in their place included, saying it cannot tell which repository
  the command acts on and asking for the path spelled out. So does a branch move or reset run
  through a program held in a variable (`$G checkout -b x`) (#1354).
- **An alias chain longer than the project-root guard follows is refused.** A chain still on
  an alias after four hops used to be read as ending there and allowed; its end may move HEAD,
  so it is now refused, with a sentence that says so (#1354).
