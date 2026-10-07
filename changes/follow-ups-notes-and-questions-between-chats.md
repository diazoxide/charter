### Added

- **An asking chat and its persona chats can talk while a task runs.** `purlis dispatch tell
  <chat> "<text>"` sends a follow-up to a task that is still working, which reads it on its next
  turn; one to a task that has finished is refused with its state. A task sends a progress note
  with `purlis dispatch note "<text>"`, and asks a question with `purlis dispatch ask
  "<question>"`, which holds until the asking chat runs `purlis dispatch answer <chat>
  "<text>"`. Every message arrives quoted as data from the other chat, and none is typed into a
  chat. Messages pass only between a chat and the tasks it dispatched itself, ten a minute for
  one pair unless the project's `messages-per-minute` limit says otherwise (#1442).
- **No chat can answer a question a task put to you.** A task that needs you asks in its own
  tab, and `purlis dispatch answer` answers only a question the task asked its asking chat
  (#1442).
- **A report says when you stepped in.** If you typed in a task's chat while it worked, its
  report carries "The operator stepped in" and nothing of what you typed. Picking an option of
  a prompt the task showed you does not count; words do (#1442).
