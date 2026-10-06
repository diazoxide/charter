### Fixed

- **A lock charter lets go of is free at once, even while one of its programs is starting.** A
  program charter starts has a copy of the lock's descriptor until it runs, and that copy used
  to keep the lock held, so `rename-local` and its undo could find the config home busy for a
  moment after the app or a chat's server let go of it. The config home's lock, the event log's
  writer lock and the hook spool's locks are now unlocked before they are closed, as charter's
  other locks already were (#1316).
