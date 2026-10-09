### Changed

- **A chat's Allow on a sandbox block answers only the block it showed.** purlis now checks that
  the chat is still blocked on exactly what the Notice showed before it grants anything. An
  Allow for a block that was already answered, or that a newer block replaced, grants nothing
  and says why (#1538).
