### Fixed

- **Starting a chat, opening a project and closing a chat no longer wait for ever on the hook
  spool.** Each waited, with no limit, for the lock on the spool's keys while another process
  held it. Now each waits two seconds. A chat whose key could not be recorded in that time still
  starts, and any line it spools is refused at the next drain rather than accepted unchecked. A
  drain that gives up reads nothing, and the next start drains it (#1426).
