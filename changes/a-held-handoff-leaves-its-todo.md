### Fixed

- **A handoff you allow later leaves its todo too.** A handoff to another persona waits for
  your Allow, and the command that asked has returned by then, so its todo was never written
  in the workspace the work moved to. The app now writes it when you allow, as the command
  does for a handoff that opens at once. If the todo or the handoff's row in the dispatch log
  cannot be written, the chat that asked is told with the rest of what it hears (#1471).
