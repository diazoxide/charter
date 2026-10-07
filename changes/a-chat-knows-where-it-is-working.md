### Added

- **A chat knows who asked for it and where else its persona is working.** A chat purlis
  started is told at its start which chat asked for it, which other tasks that chat asked for,
  and every other chat running as the same persona in the project, each with its workspace,
  name, state and start time. When that changes, its next turn is told in one line, and a turn
  is told nothing when nothing changed. `purlis persona where` and the `persona_where` tool
  show the same picture at any time. purlis answers from its own record of the open chats, so
  it works in a sandboxed chat; a chat can only ask about itself, and it learns names and
  states, never another chat's brief or transcript (#1450).
