### Changed

- **A task that did not start is a failed row under its session, not a banner.** A task that
  was let through and then started no chat (the profile's program is missing or does not
  answer as its harness, the sandbox will not wrap it, there is no terminal to give it, its
  folder is gone, its worktree could not be cut, or a limit filled after you pressed Allow)
  is now a finished row under the chat that asked: failed, with the reason shown in full
  under it. It never folds into "Finished (n)", it is cleared like any finished row, and it
  is there after the app is quit and reopened. A task the chat tried several times is one
  row that says how often. Where you pressed Allow, the chat that asked is told as it is told
  a task's report, failed and in purlis's own words ("it did not start: <reason>"), so a
  command waiting on the task returns at once. A start refused while the asking command is
  still on the line is answered there, as before, and is not said twice. `purlis dispatch
  list` says "did not start" for such a task (#1497).
- **A task a launch could not start again is drawn under the chat that asked, and is not
  ended.** It stays recorded and is tried again at the next launch, as before. Its row gives
  the reason and offers Try to start again, Review and approve… where its profile waits on
  your approval, and End task. Only End task ends it: the chat that asked is then told it
  failed, and Reopen carries its conversation on as an ordinary chat. Until then that chat is
  told nothing, and a wait on the task answers that it is waiting on you (#1497).
- **The line across the window is kept for chats nobody asked for.** "<chat> did not start" is
  now drawn only for a chat you opened yourself, a handoff's chat, and a task whose asking
  chat did not come back either. A long reason in it wraps inside the window.
