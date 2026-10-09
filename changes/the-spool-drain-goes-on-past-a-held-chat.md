### Fixed

- **One chat's held hook spool no longer holds up the drain of every other chat.** When a
  project opens, purlis waits up to a second for each chat's spool. A chat still held after that
  is left as it is, with its keys, and drained at the next start. The file an older build left
  is read only up to the most that build could write. A spool folder whose first sync ran out of
  time is synced again by the next line written to it (#1419).
