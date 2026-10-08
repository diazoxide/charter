### Changed

- **The explorer lists a workspace's own chats, and one line for a session's tasks.** Under a
  workspace it lists the chats that have a tab of their own: sessions, handoffs, shells, and a
  task you gave a tab. Tasks are not listed there any more. Under a session that has tasks,
  one line says how many and how they stand, `5 tasks 2 working 3 done`, with `1 failed` and
  `1 needs you` when there are any. Pressing it puts the keyboard on that session's row in the
  Chats list, unfolded, which is where its tasks are listed. Tasks working in a workspace that
  a chat of another workspace asked for are one line too, `2 tasks from other workspaces`
  (#1490).
- **A chat's helpers are a count on its row.** `3 helpers`, in place of a row of ids each.
  Pressing it, or Right on it, unfolds a row per helper, `helper a3882da5` with its state in
  the words every row uses; the whole id is the row's tooltip. It stays as you left it for
  each chat while the window is open (#1490).
- **A task that needs you puts the hand on its session's row in the explorer**, as it does in
  the Chats list. Pressing the hand goes to the task (#1490).

### Removed

- **The explorer no longer lists the chats a chat started in other workspaces under its row.**
  The Chats list is the one tree of who asked whom (#1490).
