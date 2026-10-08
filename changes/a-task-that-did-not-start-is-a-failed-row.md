### Changed

- **A task that did not start is a failed row under its session, not a banner.** A task that
  was let through and then started no chat (the profile's program is missing or does not
  answer as its harness, the sandbox will not wrap it, there is no terminal to give it, its
  folder is gone, its worktree could not be cut, a limit filled after you pressed Allow, or a
  launch could not put it back) is now a finished row under the chat that asked: failed, with
  the reason shown in full under it. It never folds into "Finished (n)", it is cleared like
  any finished row, and it is there after the app is quit and reopened. The chat that asked
  is told as it is told a task's report, failed and in purlis's own words ("it did not start:
  <reason>"), so a command waiting on the task returns at once. A start refused while the
  asking command is still on the line is answered there, as before, and is not said twice
  (#1497).
- **The line across the window is kept for chats nobody asked for.** "<chat> did not start" is
  now drawn only for a chat you opened yourself, a handoff's chat, and a task whose asking
  chat did not come back either. A long reason in it wraps inside the window.
