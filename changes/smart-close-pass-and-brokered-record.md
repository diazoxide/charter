### Changed

- **Smart close saves its record from a sandboxed Claude Code chat, one in a clone included.** In
  a chat purlis started, the session record is now written by purlis itself: the smart-close
  skill calls purlis's new `session_record` tool, and `purlis session record` hands the record to
  purlis over the chat's own connection instead of writing it. The chat's sandbox no longer has
  to reach the project's files. Codex and opencode sandboxes are not covered yet (#1332).
- **Typing `/smart-close` in a terminal chat closes the tab too.** A smart close you type into a
  waiting terminal chat now counts as yours, like the tab's **Smart close**: the record is written
  with no prompt and the tab closes when that turn ends. A chat that starts a smart close on its
  own still writes its record, and its tab stays open for you to close (#1332).
