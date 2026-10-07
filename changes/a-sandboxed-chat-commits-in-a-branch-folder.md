### Added

- **A sandboxed chat can commit in a branch folder.** A git worktree keeps its index and
  objects in its clone's `.git`, which a sandboxed chat may not write, so `git add` and `git
  commit` were refused there. `purlis worktree commit -m "<message>" --all` (or paths in place
  of `--all`; a new file must be named) now asks the app to stage and commit, on the branch
  the folder is on, with your git identity and the chat's trailers. A chat started in a branch
  folder is told the command when it starts. It only commits: it never amends, resets,
  rebases, merges or pushes, and it asks for no approval, as a commit in a clone asks for
  none. **The repository's own hooks are not run** for such a commit (a `pre-commit` that
  lints or formats, husky's, a `commit-msg` check), and the command says so each time one is
  there; purlis's scan for secrets and personal data does run. Git run this way reads none of
  your global or system git config but your name and email. The app refuses, in a sentence, a
  repository whose own config names a program, one that signs its commits, one that borrows
  objects from another, a folder inside that is a repository of its own, a change to
  `.gitmodules`, a path reached through a link and a name that differs from another only by
  case. **A file your attributes mark for a content filter (Git LFS, most often) is refused
  too**: your filters are in the config this commit does not read, so the file would be
  stored unfiltered; commit such a file from your own terminal. One commit runs in a branch
  folder at a time, and one the app was stopped in the middle of is unstaged by the next. The command works for a chat started in the branch folder itself; a chat that is not
  sandboxed, and any chat in a clone, uses `git commit` as before (#1055).
