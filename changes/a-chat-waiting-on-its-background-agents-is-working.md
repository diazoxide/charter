### Fixed

- **A Claude Code chat waiting for its background agents is shown as working, not as needing
  you.** When a chat's agent started background agents and its turn ended while it waited for
  them ("Waiting for 4 background agents to finish"), purlis put it in needs you and sent a
  notification. Now it reads as working until those agents are done and the chat stops with
  nothing left in flight, or until it asks you something. A chat whose agents were stopped
  before they woke it shows needs you once Claude Code says it is idle, without a "waiting on
  you" prompt it never showed. Background shells and monitors, which can run for as long as
  the chat does, still hand you the chat when its turn ends (#1626).
