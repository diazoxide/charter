### Added

- **A working chat says in one line what it is doing.** Under its name in the Chats list, on
  the second line of its row, with how long it has been at work after it: *running cargo ·
  2m*. While a tool runs the line is in the present (*running cargo*, *editing Notice.tsx*,
  *reading 3 files*, *searching*, *fetching a page*, *waiting on a helper*, *dispatching a
  task*, *asking a question*, *writing its report*, *using a tool*), and once the tool has
  come back it is in the past (*ran cargo*, *edited Notice.tsx*, *read 3 files*) until the
  next tool purlis hears. *thinking* is said only from a turn's start until its first tool.
  purlis says nothing it has not heard: a tool it gets no word of leaves the line as it was.
  The words are purlis's own. The most a line ever names is a file's base name, or a program
  from a short list purlis keeps (cargo, npm, git and the like); a command that starts with
  anything else reads *running a command*, and a file whose name is not plain ASCII letters,
  digits and a few marks reads *editing a file*. Never a command's arguments, what it
  printed, a web address or a file's contents. The line stands in for where the chat works
  while it works and gives the line back when the turn ends; a row never changes height for
  it. It is kept in memory only, and a chat whose harness purlis has heard nothing from has
  no line. A screen reader is told it as the row's description, on demand, and never as it
  changes (#1493).
