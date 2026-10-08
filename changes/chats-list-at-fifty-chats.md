### Added

- **A filter over the Chats list.** Type a name, a persona, a workspace or a state's word, or
  press the chip for the chats that need you or the ones at work. It finds finished tasks too,
  by how they ended. The session that asked for a task stays above it and is opened, whatever
  fold you had set; the fold is back when the filter is cleared. A line under the filter says
  how many chats it hides, and when one of those needs you it says so and has a button that
  goes to it. Escape clears the filter, and it stays at the top while the list scrolls. The
  palette finds a task that has no tab, too: *Show task …* (#1499).
- **Keys on a row of the Chats list.** Enter opens the chat. Delete on a task (the delete key
  on a Mac) asks to stop it. Space asks for the chat beside the one in front, which purlis
  cannot do yet, and says so (#1499).
- **Two settings for the Chats list**, under You: rows on two lines or one, and the sessions
  grouped by workspace (#1499).

### Changed

- **The Chats list puts the sessions that need you first**, then the ones at work, then the
  idle. The tasks under a session stay in the order they were started. While the pointer is
  over the list or the keyboard is in it, no row is moved: the order, the folds the list makes
  and the chats that arrive wait until you leave. A chat that ends is the exception: its row
  goes at once, and the rows below it move up (#1499).
- **A session folds by itself when every task under it is over**, and its row then counts how
  they ended. It opens again when one of them works or needs you. A fold you set yourself is
  kept (#1499).
- **A row of the Chats list is two lines**: the name and the state, then where the chat works,
  how long it has been in that state where the window saw it change, and its own branch. In a
  narrow sidebar the name is cut short, with the whole of it on hover, and the state never is:
  it goes under the name, and wraps where the sidebar is narrower than the state. A finished
  task's row follows the same rule. A task says its workspace only when it is not the asking
  session's. The "no tab" badge is gone (#1499).
- **The left region is never narrower than 11rem**, so a nested row of the Chats list keeps
  room for its state at any text size (#1499).
