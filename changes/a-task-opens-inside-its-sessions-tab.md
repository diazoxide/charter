### Changed

- **A task opens inside its session's tab.** Pressing a task in the Chats list, the explorer,
  the title bar's needs-you list or the palette brings forward the tab of the session that
  asked for it and switches that tab to the task. No tab is added. The pane's top line then
  says where you are, `steward 4 › talk · working`, with the whole path for a task of a task
  and `in <workspace>` for a task that works in another workspace. Pressing a name in it goes
  to that chat. The tab reads `steward 4 › talk`, and pressing it while it is in front goes
  back to the session's own chat. Each tab remembers the chat it shows, across a reload and a
  relaunch too (#1486).
- **A pane never changes what it shows by itself.** A task you are reading that ends stays on
  screen as ended, with how it ended and one button back to its session's chat. A pane never
  draws a task without the line that says which chat it is, and nothing typed reaches a task
  the pane cannot say the path to (#1486).
- **A chat of a tab that is waiting for you and is not on screen puts the hand on that tab.**
  That is a task that needs you, the session's own chat while the tab shows a task, and a chat
  with a dispatch held for your answer, a vault it was refused, a sandbox block or a restart
  it is owed. Going to it switches the tab to that chat. After you answer, the tab stays where
  it is (#1486).
- **What a chat of a tab has to ask you is on that tab's pane, whichever chat the tab shows.**
  A Notice of a chat that is not the one on screen says whose it is first and has **Go to
  it**. Its answers are that chat's (#1486).
- **A tab's close button closes the session, whatever the tab shows.** While a tab shows a
  task, the pane draws no close of its own, so nothing on screen reads as ending the task, and
  the close dialog says the session is what ends (#1486).
- **A task that needs you is counted on the workspace of the tab it lives in.** It was counted
  on the workspace it works in, where no tab wears it (#1486).
- **A file dropped on a tab goes to the chat the tab shows** (#1486).

### Removed

- **A pressed task no longer becomes an ordinary tab.** It did, for good, and its close button
  read as ending the session. A task whose session has no tab in the window (the session was
  closed and left it running) still opens in a tab of its own. The "no tab" badge on a task's
  row is gone: no task has one (#1486).
