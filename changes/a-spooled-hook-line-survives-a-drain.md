### Fixed

- **A hook line written while purlis was reading the spool is no longer thrown away.** When
  purlis is not there to take a hook's line, the hook keeps it in the chat's spool, and purlis
  records it the next time the project is opened. Purlis used to forget how to check a chat's
  lines as soon as it had read the spool once. So a line a hook was still writing at that
  moment, and every line from a hook that was still running afterwards, was rejected at the
  next open and its content was never recorded. Purlis now keeps what it needs to check a
  chat's lines, and the number of the last line it recorded, until the chat is closed or the
  project is opened without it. A late line is recorded at the next open, a line that is put
  back after it was recorded is rejected as repeated, and an open that stopped part-way no
  longer reports the lines it had already recorded as missing. A chat's spool is also read
  when the chat is closed or restarted, so nothing waits for the next open (#983).
