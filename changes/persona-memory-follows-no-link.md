### Security

- **A persona's memory and the shared store follow no link.** Listing, reading, editing,
  archiving and restoring a persona's memory, or the shared store's, now reach the store the way a
  workspace's store is reached: one folder at a time, with no link on the way. A link at or below
  the store, its `archive/` included, is refused or left out even when it points somewhere else
  inside the project, and nothing outside the store changes (D-90c, #1194).
