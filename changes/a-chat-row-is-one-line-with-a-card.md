### Changed

- **A chat's row in the Chats list is one line, and its details are in a card.** A row is its
  state's mark, its persona's badge and its name, with a hand when it needs you. Rest the pointer
  or the keyboard on it and a card says the state in words, what it is doing, its persona,
  harness and workspace, how long it has been in its state, its tasks, its own branch, where its
  work went or came from, and a task's tokens. A chat's tasks are rows under it; folded, it shows
  how many in a small count. A task that comes to need you opens the rows above it (#1675).

### Removed

- **Settings › Chats list › Rows.** A row is always one line. A layout file that still says
  `chats.lines` is read as if it did not (#1675).
