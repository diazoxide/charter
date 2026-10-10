### Fixed

- **A chat's sub-agents end with it in the event log.** When a chat's program exits, or purlis
  stops or closes it, each of its sub-agents still running is now recorded as ending with it,
  in the same end state and for the same cause. Before, the log began a sub-agent's run but
  never ended it unless the sub-agent stopped first (#1084).
