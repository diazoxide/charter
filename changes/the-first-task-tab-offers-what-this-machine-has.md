### Fixed

- **The First task tab offers only a harness this machine has.** A built-in profile whose program
  is not installed is no longer suggested for the second chat: its row says so (*Codex is not
  installed on this machine.*) and it cannot be started. A profile your project defines runs its
  own command, so it is offered as before (#1698).

- **The First task tab remembers its chats across a relaunch.** It reads which of its branches,
  `first-task-1` and `first-task-2`, are in the repo's copy, and shows each chat that already
  started on its branch instead of offering to start it again. Show its diff is still offered
  only for a chat started since the last launch (#1091).
