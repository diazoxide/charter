### Changed

- **The explorer lists a workspace's own chats, and one line for a session's tasks.** Under a
  workspace it lists the chats that have a tab of their own: sessions, handoffs, shells, and a
  task you gave a tab. Tasks are not listed there any more. Under a session that has tasks,
  one line says how many and how they stand, `5 tasks · 3 working · 1 waiting · 1 done`, in
  the same count and words as the session's row in the Chats list. Pressing it puts the
  keyboard on that session's row in the Chats list, opened and marked for a moment, which is
  where its tasks are listed; a filter in the way is taken off, and the list says so (#1490).
- **A branch still says a task is working in it.** Under a branch's row, and in the branch's
  own view, one line: `1 task in this branch · 1 working`. Tasks working in a workspace that a
  chat elsewhere asked for are one line under the workspace, `2 tasks from other places`.
  Pressing either shows those tasks in the Chats list (#1490).
- **A chat's helpers are a count on its row.** `3 helpers · 1 working`, with `1 failed` when
  one did, in place of a row of ids each. Pressing it, or Right on it, unfolds a row per
  helper, `helper a3882da5` with its state in the words every row uses; the whole id is the
  id's tooltip. It stays as you left it for each chat while the window is open. A task shown
  inside its session's tab says the same count on its row in the Chats list (#1490).
- **A task that needs you puts the hand on its session's row in the explorer**, as it does in
  the Chats list. Pressing the hand goes to the task (#1490).

### Removed

- **The explorer no longer lists the chats a chat started in other workspaces under its row.**
  The Chats list is the one tree of who asked whom (#1490).
