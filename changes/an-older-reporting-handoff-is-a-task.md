### Fixed

- **A chat an older purlis opened to report back now wakes the chat that asked.** Before the
  update, a handoff that asked for a report wrote it, and the asking chat read it only when you
  next typed into it. Such a chat, still open after the update, is now a task of the chat that
  asked, in every way. Its report wakes that chat as a task's report does. That chat lists it
  and can wait on it with `purlis dispatch`, and can tell it or cancel it. If it ends without
  reporting, the asking chat is told. Like any task, purlis ends it once it has reported, and
  you can reopen it from its finished row (#1519).
- **A handoff that asks for a report never opens as one now.** `purlis handoff --report` sends
  a task, which counts against the same limits as `purlis dispatch`, and a request for one
  from an older command line opens nothing and names `purlis dispatch` (#1471).
