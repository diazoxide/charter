### Added

- **You can stop a chat, or a chat and everything below it.** Every row of the Chats section,
  and every chat's tab menu, has **Stop chat …** and **Stop chat … and everything below it**.
  Each asks first and says what ends. A chat another chat started gets one short turn to write
  what it did, then it ends, and the chat that asked is told you stopped it. Nothing is typed
  into a chat that is showing a prompt: that one ends as it stands. The stop of a subtree ends
  the deepest chats first and touches nothing outside it. Pressing Stop on a chat that is
  already stopping ends it without waiting. Only you can stop a chat: no chat can. A chat that
  is being stopped, and any chat below it, cannot start another chat or be started again, so
  nothing outlives the stop. The chat that asked reads the stop as purlis's own line, which no
  report can pass for, and its row says the chat was stopped (#1448).
- **The needs-you hand rolls up the tree.** A chat that needs you marks its own row and every
  row above it in the Chats section, and counts on the workspace it works in. Rows with chats
  under them fold, and a folded row still shows the hand for what it hides. Pressing the hand
  on a row above goes to the chat that needs you, opening its tab when it has none (#1448).

### Changed

- **A report is no longer a needs-you item.** A report from a chat another chat started goes to
  the chat that asked, which reads it on its next turn. It raises no item for you while that
  chat is open, on either chat. A report whose asking chat has closed has nowhere to go, and
  that is an item, on the chat that wrote it, whether the asking chat closed before the report
  was sent or before it was read (#1448).
