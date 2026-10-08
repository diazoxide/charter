### Added

- **A session's tab wears a chip of its tasks.** After the session's name the tab says how
  many of its tasks are working, how many are waiting (idle, or needing you), how many failed
  and how many are finished, each count with its state's shape, and nothing for a zero. A ring
  means something is at work; no ring means nothing is. The counts move as the tasks do, and a
  task that has ended is still counted until its finished row is cleared. A session with no
  tasks has no chip and its tab is exactly as it was. On a crowded strip the name gives way
  first, then the counts from the end; a tab is never wider for having a chip (#1487).
- **The chip opens the menu of the chats in that tab.** The session's own chat first, then its
  tasks under who asked for them, each with its persona's mark, its name, its state in a word
  that is never cut, how long it has been in that state where this window saw it begin, and
  `in <workspace>` for one that works elsewhere. The chat the tab shows now is marked. Done and
  cancelled tasks fold into **Finished (n)**; a failure, and a task you closed, keeps a row of
  its own. Picking a chat switches the tab to it; picking a task that has ended opens its
  report under its line. The menu opens on a press, and when you move the pointer onto the chip
  and hold it still for about a third of a second. A pointer passing over the strip opens
  nothing, and neither does a chip that appears under a pointer you are not moving, or a tab
  being dragged. A menu the resting pointer opened takes no keyboard, so typing goes on where
  it was, and it closes a moment after the pointer has left both the chip and the menu. A menu
  you pressed or opened with a key stays until Escape, a pick or a press outside (#1487).
- **Keys for the chats inside a tab.** On a Mac: ⌘⇧J opens the tab's task menu with the
  keyboard on the chat it shows, ⌘⌥↓ and ⌘⌥↑ go to the next and the previous chat in the tab,
  ⌘⇧H goes back to the session's own chat. Elsewhere: Ctrl+Shift+J, Ctrl+Shift+] and
  Ctrl+Shift+[, Ctrl+Shift+H. Down on a focused tab that has tasks opens its menu too, and the
  tab says it has one. All four are rows in the palette, which says each key. None is a key a
  terminal turns into anything, so a chat loses nothing (#1487).

### Changed

- **The hand on a tab is pressed.** Where a chat of a tab is waiting for you and is not on
  screen, the hand is on the tab's chip and a press goes to the chat that has waited longest.
  It was a mark that could not be pressed (#1487).
