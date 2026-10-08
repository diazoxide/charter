### Added

- **A working chat says in one line what it is doing.** Under its name in the Chats list, on
  the second line of its row: *thinking*, *running cargo*, *editing Notice.tsx*, *reading 3
  files*, *searching*, *fetching a page*, *waiting on a helper*, *dispatching a task*, *asking
  a question*, *writing its report*, *using a tool*. The words are purlis's own. The most it
  ever names is a file's base name or a command's first word: never a command's arguments,
  what it printed, a web address or a file's contents. A name with a space, an invisible
  character or markup in it is not shown at all. The line takes the place of the rest of the
  second line while the chat works and gives it back when the turn ends; a row never changes
  height for it. It is kept in memory only, and a chat whose harness purlis has heard nothing
  from has no line. A screen reader is told it as the row's description, on demand, and never
  as it changes (#1493).
