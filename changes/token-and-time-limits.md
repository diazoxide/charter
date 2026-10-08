### Added

- **Optional limits on a session's tokens and on one task's time.** Settings › Project ›
  Dispatch has two more limits, off until set, at the same levels as the others: *Tokens per
  session* (its own chat and all its tasks, as their harnesses report them) and *Minutes per
  task*. At the token limit a new task is refused with the figure, the session's row says
  `at its token limit`, and its tasks at work are asked for their report; a task past its
  minutes is asked for its report and ended. The chat that asked is told which limit stopped
  it. A harness that reports no tokens is not counted, and Settings says so (#1512).
