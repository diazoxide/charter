### Fixed

- **A refused append, and a folder made ahead of a write, name their file.** When the filesystem
  refuses with "Operation not permitted", an append to the work, dispatch or trace log or a
  memory index, a folder made ahead of a write, a memory's archive, restore or move, a new
  persona and a workspace's vision now say which file or folder it was, and, in a sandboxed
  chat, that the chat's sandbox refused it. `purlis init` names a refused file once, not twice.
  A handoff whose dispatch row could not be written says the chat is open and nothing needs
  running again (#1359).
