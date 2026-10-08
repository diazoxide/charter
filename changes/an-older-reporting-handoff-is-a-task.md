### Fixed

- **A chat an older purlis opened to report back now wakes the chat that asked.** Before the
  update, a handoff that asked for a report wrote it, and the asking chat read it only when you
  next typed into it. Such a chat, still open after the update, is now a task of the chat that
  asked. Its report wakes that chat as a task's report does. That chat lists it and can wait on
  it with `purlis dispatch`. If it ends without reporting, the asking chat is told. It keeps its
  tab (#1519).
