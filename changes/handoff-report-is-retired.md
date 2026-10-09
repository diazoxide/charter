### Changed

- **Every report is sent by `purlis dispatch report`, and `purlis handoff report` only says
  so.** A handoff owes no report, so the first message of a chat you hand work to no longer
  says how to send one. When you stop a chat that was handed its work, its one last report is
  sent with `purlis dispatch report`, as a task's is. `purlis handoff report` sends nothing and
  names that command, and an open from an older command line that asks for a report opens
  nothing and names `purlis dispatch` (#1471).
