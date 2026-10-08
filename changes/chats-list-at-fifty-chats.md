### Added

- **A filter over the Chats list.** Type a name, a persona, a workspace or a state's word, or
  press the chip for the chats that need you or the ones at work. The session that asked for a
  task stays above it, the list says how many chats the filter hides, and Escape clears it. The
  palette finds a task that has no tab, too: *Show task …* (#1499).
- **Keys on a row of the Chats list.** Enter opens the chat. Delete on a task asks to stop it.
  Space asks for the chat beside the one in front, which purlis cannot do yet, and says so
  (#1499).
- **Two settings for the Chats list**, under You: rows on two lines or one, and the sessions
  grouped by workspace (#1499).

### Changed

- **The Chats list puts the sessions that need you first**, then the ones at work, then the
  idle. The tasks under a session stay in the order they were started. Nothing moves while the
  pointer is over the list or the keyboard is in it (#1499).
- **A session folds by itself when every task under it is over**, and its row then counts how
  they ended. It opens again when one of them works or needs you. A fold you set yourself is
  kept (#1499).
- **A row of the Chats list is two lines**: the name and the state, then where the chat works,
  how long it has been in that state where the window saw it change, and its own branch. In a
  narrow sidebar the name is cut short, with the whole of it on hover, and the state never is.
  A task says its workspace only when it is not the asking session's. The "no tab" badge is
  gone (#1499).
