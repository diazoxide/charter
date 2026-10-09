### Fixed

- **A sub-agent's run now ends in the event log.** A child run gets a `run.ended` when its
  `SubagentStop` is heard (`completed`, `child-ended`), and otherwise when its parent's run
  ends, in the parent's end state and cause, where before it was only forgotten (#1084).
