### Changed

- **A chat is told how it was asked for.** Where a chat is working, at its start, in
  `purlis persona where` and the `persona_where` tool, now says whether the chat that asked
  for it dispatched a task or handed its work off, and whether it still waits on a report.
  The chat that asked is matched by the lineage it is in as well as by its number (#1455).
