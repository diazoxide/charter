### Fixed

- **A reader of the event log whose place is past its end is told so.** After a power loss took
  lines it had already read, it gets an `ahead` marker and every event written after, where it
  used to skip the new events that reused those numbers (#941).
