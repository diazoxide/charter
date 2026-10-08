### Changed

- **A chat waiting on its tasks says so, and is not in the needs-you list.** A chat whose turn
  has ended while tasks below it still work reads `waiting on 2 tasks`, in the working colour
  with a mark of its own, in the Chats list and the explorer. It needs you only once every
  task has reported or ended and it has then stopped with nothing to do. A task paused on a
  question to the chat that asked reads `asking <chat>` and is not in the list either. A
  permission prompt or a question for you still is, whatever the tasks are doing (#1491).
- **A chat's row counts its tasks.** `2 working · 1 waiting · 1 failed · 3 done`, each part
  only when it is not zero, and `6 of 6 tasks` when it is at the most it may have running.
  Waiting is a task that needs you or is at rest. A folded row says the same count (#1491).
- **A chat that needs you is never left at rest unmarked.** A chat waiting on its tasks is
  looked at again whenever one of them reports, is stopped, is closed or dies, and when a line
  purlis typed into it was never taken. A task you asked for yourself from a chat's tab does
  not hold that chat back (#1491).
- **Only a task that came to nothing flags its chat.** A task that finishes as done, or is
  cancelled, changes the count. One that failed, was blocked, ended without a report or did
  not start puts the hand on the chat that asked: the item says which task and why, and goes
  to that task's row. It goes, one failure at a time, once you have been shown it, open its
  row, clear the row or ignore the chat (#1491).

### Fixed

- **No notification for a task that finished.** A system notification is sent when a chat
  itself comes to need you, and no longer each time one of its tasks reports or each time a
  chat that already needs you moves (#1491).
