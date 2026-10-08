### Added

- **A chat's Activity: one timeline of what it and its tasks said to each other.** A chat
  tab's menu and the palette have _Activity_, which opens a read-only tab listing every task
  the chat dispatched and the tasks under those, oldest first: each brief, follow-up, progress
  note, question, answer and report, with its time and who said it to whom. The name of the
  chat that said a line opens that chat. What a chat said is shown as text and never as markup,
  and a long line is clipped until you ask for all of it. A file that the reports of two tasks
  working in the same folder both name is marked on each. The tab follows the work while it is open. To make this possible a
  dispatch's record on this machine now keeps the text of the messages the two chats sent
  through purlis: the first 500 of a task, for 30 days after the task ended. After that each
  message keeps its line and its time, and its words are gone. Nothing of either chat's
  conversation is kept (#1495).
