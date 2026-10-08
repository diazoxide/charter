### Changed

- **A chat's row says its state in a word beside a mark, and each state has its own shape.**
  The Chats list and the explorer show the same word for the same chat: working, needs you,
  `asking <chat>`, done, failed, cancelled, or ended without a report. A task that has
  reported no longer looks like a chat waiting on you. A chat at rest that is not asking for
  you reads idle, and one whose harness has sent nothing reads
  `running (no detail from <harness>)`. A task you stopped reads cancelled, and a row says
  needs you whenever the title bar's list does (#1484).
- **The app says "task" and "helper".** A chat a dispatch started is a task, and a harness's
  own sub-agent under a chat is a helper, in the explorer, the close dialog, the dispatch
  question and the dispatch settings (#1484).

### Fixed

- **A task with no tab has one name everywhere.** The explorer listed a task that had no tab by
  its number, while the Chats list gave its name (#1484).
