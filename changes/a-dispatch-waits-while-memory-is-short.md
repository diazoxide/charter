### Added

- **A dispatch waits while this machine is short on memory.** A task or handoff that every
  other check allows no longer starts a new chat when the system reports that memory is short
  (critical memory pressure on macOS; memory stalls, or too little available, on Linux). The
  chat that asked is told it is waiting, the Dispatches tab lists it under *Not started*, and it
  starts by itself once memory frees, checked against the limits as they are then. If memory is
  still short after 10 minutes, nothing starts and the chat that asked is told. A dispatch you
  start yourself from a chat's tab does not wait (#1467).
