### Added

- **Move to its own tab.** A task lives inside its session's tab. It gets a tab of its own only
  when you ask: from its row's menu in the Chats list, from the buttons at the end of its line
  in its tab's task menu (or `⌘`/`Ctrl`+Enter on that line), from its pane's top line while its
  session's tab shows it, or from the palette. The tab is on the strip of the workspace the
  task works in, and is drawn so it is not mistaken for a session's: the task mark, its name,
  and whose task it is, dimmer (`talk · steward 4`). Its pane says the whole path, as a pane
  switched to a task does (#1489).
- **A task's tab has a minimise button, not a close.** `−` sends the task back into its
  session's tab and ends nothing: the task goes on working, and its session's tab still lists
  it. Nothing on a task's tab ends the task. The key that closes a tab (Delete, or Backspace on
  a Mac) minimises a task's tab. A task whose asking chat has no tab in this window goes back
  to the Chats list (#1489).
- **Open beside.** A task opens beside the session that asked for it, inside that session's
  tab: the session's chat on one side, the task on the other, each pane saying which chat it
  is. Space on a task's row in the Chats list does it, and so do its row's menu, its line in
  the task menu (`Alt`+Enter, or the button at the line's end) and the palette. The task's
  pane has a minimise where the session's pane has its close; it sends the task back to the
  list and the session's pane takes the room (#1489).
- **A setting, "Open tasks in their own tabs"**, under Settings, You, Chats list. It is off:
  pressing a task switches its session's tab to it. On, pressing a task opens it as a tab of
  its own, with the minimise button. Either way the session's tab keeps its chip and its menu.
  It is `chats.tabbed` in `layout.json` (#1489).

### Changed

- **A session's tab still lists a task that has a tab or a pane of its own.** Its chip counts
  it and its menu marks it "in its own tab" or "beside it"; picking it there brings that tab or
  pane forward. While a task in its own tab waits for you and that tab is not in front, the
  session's chip wears the hand and names it (#1489, #1601).
- **The close button on a session's tab is the only close on the strip.** It closes the session
  and asks once about the tasks still working, as before. The dialog now says how many of the
  session's tasks have a tab of their own: those tabs close with the session's, and a task you
  keep running stays in the Chats list. A task beside its session is not ended by the
  session's close either (#1489).
- **Whenever a session's pane goes, its tasks' own tabs go back to the list.** The tab's close,
  a close of the session's pane, a Smart close and the session ending all do the same: a task
  the session leaves running has no tab left, a task beside it included, and is in the Chats
  list. None is ended by that (#1489).
- **A task is not started fresh.** A fresh start is a new conversation, and a task's brief is in
  the one it has. Its tab draws no Start fresh mark, the row in its tab's menu says why it
  cannot run, and purlis refuses it from anywhere else. Restart chat keeps the conversation,
  and on a task it asks first (#1489).
- **A task you are reading is not ended under you in any pane.** A task that has reported is
  held while it is on screen in the tab in front: inside its session's tab, beside its session,
  or in a tab of its own. It is ended once you look away. No system notification is sent about
  a chat that is on screen in any pane of the tab in front (#1489).
- **A task opened beside its session, and a task in a tab of its own, come back after a
  relaunch** where they were. A task whose session did not come back is in the Chats list
  (#1489).
