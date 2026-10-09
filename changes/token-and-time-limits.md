### Added

- **Optional limits on one task's working time and on a session's tokens.** Settings › Project
  › Dispatch has two more limits, off until set, at the same levels as the others. *Minutes per task* counts a task's working time only (not time it waits on you
  or on its own tasks, nor time purlis was closed, and a task put back after a restart keeps
  what it had); a task past it is asked for its report and ended with its own tasks, never while
  you are answering or typing in it, and the chat that asked is told it was its time limit.
  At *Tokens per session* a session starts no new task, its tasks at work are asked for their
  report (never one you are answering or typing in), and its row shows its figure against the
  limit. A harness that reports no tokens is not counted, and Settings says so (#1512, #1457).
