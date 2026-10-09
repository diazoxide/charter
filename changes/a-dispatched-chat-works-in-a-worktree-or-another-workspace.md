### Added

- **A dispatch can say where its chat works.** `purlis dispatch --in workspace:<name>` starts
  the persona chat in another workspace of the project, with that workspace's todos, memory
  and session records; it is listed under the chat that asked, with the workspace named, and
  reports back to it. `--in worktree` gives the chat a new worktree of the repo the asking
  chat works in, on a new branch. The app cuts the worktree, under a folder and a branch
  purlis names for the dispatch, so two tasks never share either. The `dispatch` tool takes
  the same word as `in`.
- **A worktree task's report names its branch**, from purlis's own record of what it cut.
  purlis merges nothing for a task by itself; you merge it from the task's Changes tab.
- **In a sandboxed project a worktree task commits with `purlis worktree commit`.** A
  worktree's git data is outside the folder a sandboxed chat may write, so its own `git
  commit` is refused there and the app commits for it on the task's branch. purlis tells the
  new chat and the asking chat so as the task starts. With no sandbox the task commits on its
  branch with git.
- **A task's own branch is listed on its row in the Dispatches tab**, with **Discard**.
  Discard removes the branch's folder: it asks first and names every uncommitted file and
  every ignored path of the task's that would go, and is refused while a chat is still open in
  the folder. The branch purlis cut loses no commit: one that holds work stays. A branch that
  is merged is removed with its folder when its chat is closed or the project is opened, and
  only where the folder holds nothing else: a folder holding an uncommitted or an ignored
  file of the task's is never removed for you.
- **`dispatch-isolation: worktree` on a persona** makes a worktree its chats' default place to
  work when a dispatch names none. The key is read again: a persona file that still carries
  the line gets this the moment you update, with no edit, and `purlis persona lint` no longer
  warns about it. Nothing asks before two chats write in one tree.
- **Ask <persona>… asks where the chat works**: this chat's folder, a branch of its own, or
  another workspace.
- **`purlis dispatch list` says the branch a task was given** and how its worktree stands, and
  a task's row in the Chats tree says it works on a branch of its own.
- **A report purlis writes for a chat that ended without one names the task's branch**, as the
  chat's own report would have.

### Changed

- **A dispatch into another workspace is held to that workspace's dispatch limits as well as
  the asking chat's.** A workspace with a limit set to 0 is not worked in by a chat that names
  it from elsewhere, whatever a persona's own limits say.
- **A chat nobody is at starts a chat in another workspace only under a grant that already
  stands** for the pair, yours on this machine or the project's, also for its own persona.
  This rule is a dispatched task's: a chat nobody is at still hands off to its own persona.
- **A handoff is held to the dispatch limits of the workspace it moves into** as well as the
  asking chat's, as a task sent into one is. A handoff into a workspace whose limit is set to
  0 opens nothing and says so.
