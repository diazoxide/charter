### Changed

- **A handoff is a row of its own in the Chats list.** It was drawn under the chat it came
  from, like a task. The work moved, so its chat is a session of its own: its own tab with a
  close button, its own row at the top, and no part in the counts, the fold, the needs-you
  hand or "Stop and everything below it" of the chat it came from. Its row says
  `from <chat>`. The row of the chat it came from says `handed off to <chat>` on its second
  line while that chat is not working, with `and 2 more` where it handed off to several, and
  pressing those words goes to that chat. Both follow a rename (#1492).
- **A task you asked for yourself says so.** A persona chat you started with Ask from a tab is
  a task of that session, inside its tab, and its row and its breadcrumb now say
  `asked by you` (#1492).
- **The asking chat is told a task is yours, in those words.** `purlis dispatch cancel`,
  `tell`, `answer`, `wait` and `read` for a task you asked for from that chat's tab were
  refused as "not a task this chat dispatched". They are still refused, and the sentence now
  says the person asked for it and that its report comes to that chat. Any other chat is told
  what it was told before (#1492).
