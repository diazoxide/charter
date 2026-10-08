### Changed

- **A task opens inside its session's tab.** Pressing a task in the Chats list, the explorer,
  the title bar's needs-you list or the palette brings forward the tab of the session that
  asked for it and switches that tab to the task. No tab is added. The pane's top line then
  says where you are, `steward 4 › talk · working`, with the whole path for a task of a task
  and `in <workspace>` for a task that works in another workspace. Pressing a name in it goes
  to that chat. The tab reads `steward 4 › talk`, and pressing it while it is in front goes
  back to the session's own chat. Each tab remembers the chat it shows, and goes back to the
  session's own chat when the task it showed is gone (#1486).
- **A task that needs you and is not on screen puts the hand on its session's tab.** Going to
  it switches the tab to that task. After you answer, the tab stays where it is (#1486).
- **A tab's close button closes the session, whatever the tab shows.** While a tab shows a
  task, the pane draws no close of its own, so nothing on screen reads as ending the task
  (#1486).

### Removed

- **A pressed task no longer becomes an ordinary tab.** It did, for good, and its close button
  read as ending the session. A task whose session has no tab in the window (the session was
  closed and left it running) still opens in a tab of its own (#1486).
