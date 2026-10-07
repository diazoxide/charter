### Security

- **purlis reads a folder's git link itself before it runs git there.** A branch folder's
  `.git` is a small file that names the repository it belongs to. Git that purlis runs in such
  a folder (the explorer's state, Changes, saves, removing or merging a branch folder) no longer
  follows that file wherever it points: purlis reads it and tells git exactly which repository
  to use. A folder purlis cut is read only as a worktree of the clone it was cut from. A link
  that names a git directory inside the folder itself is never followed. Elsewhere below a
  workspace a link must name a worktree of a clone, or a submodule of the repository around
  the folder. A folder whose link names anything else is not read, and says "this folder's git
  link was changed, so purlis is not reading it"; one whose link names a git directory that
  is gone says so. This narrows what a rewritten link can reach; where a sandbox cannot keep
  a prepared folder from being moved into a `.git` (#1065), that gap is still open for a
  folder purlis did not cut (#1055).

### Changed

- **Uncommitted work inside a submodule no longer shows in purlis.** Git looks into a
  submodule by starting another git inside it, which purlis cannot hold to a repository it
  checked. So git that purlis runs now reads a submodule by its recorded commit only: a new
  commit in a submodule still shows as a change, and edits not yet committed there do not.
  Your own `git status` is unchanged (#1055).
- **Two folder layouts are no longer read by purlis's own git.** A worktree folder moved by
  hand is refused, with a sentence that names `git worktree repair`, until that is run in it.
  A checkout below a workspace that keeps its git directory elsewhere (`git clone
  --separate-git-dir`) is not read. A project's own top, and a checkout outside any project,
  keep whatever layout they have (#1055).
