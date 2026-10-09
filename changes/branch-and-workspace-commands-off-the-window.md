### Fixed

- **The window no longer holds still while purlis asks git about a branch or a workspace.**
  Listing a repo's branches, finding which branch a chat works in, merging, removing or
  declaring a branch done, and creating a workspace, reading what removing one would lose, or
  removing it, now run off the window's thread, as New branch already did. A second press of
  Merge, Remove or Done on a branch while the first is on its way sends nothing, and another of
  them on that branch says what it is still doing (#1007, #1610).
