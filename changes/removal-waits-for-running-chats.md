### Changed

- **A workspace or a persona in use by a running chat is not removed.** Removing a workspace is
  refused while a chat is running in it, and removing a persona is refused while a running chat
  has adopted it, `--force` or not, from the command line and the window alike. The refusal names
  the chats; close them first (#1373).
