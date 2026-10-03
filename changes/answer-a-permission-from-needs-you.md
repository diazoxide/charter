### Added

- **Answer a Claude Code chat's permission prompt from the needs-you list.** When a chat asks to
  run a tool, the prompt also shows in the title bar's ✋ list, with the options Claude Code
  offers: allow, deny, and a rule for the rest of the session. Your choice goes back through
  Claude Code's own `PermissionRequest` hook, so the chat carries on without you opening its
  pane. The pane shows its own prompt at the same time, and either place may answer; left
  unanswered for about a minute, the prompt is the pane's alone. charter never allows or denies
  anything by itself. Allow is offered only when the list shows the whole command, word for
  word and as it runs. A long command, one over several lines, one holding a credential or an
  invisible character, one asking to run outside the sandbox or in the background, or a file
  edit offers Deny and *Open in its pane* instead. The list never offers an option that saves a
  permission into the worktree's settings, or one that switches the session's permission mode;
  those stay in the chat's own pane (HP-6, #672).

### Security

- **Only you answer a chat's ask, and only for that chat.** An answer comes from the app's window
  and lands only on the ask of the chat it names, once. The permission hook believes a reply
  only from the app that started its chat, checked by the operating system's record of who is
  listening, so nothing a chat runs can answer in the app's place. The session protocol refuses
  its `answer` command, and the link's UI RPC never serves the window's answer, until client
  scopes are checked (#664) (HP-6, #672).
