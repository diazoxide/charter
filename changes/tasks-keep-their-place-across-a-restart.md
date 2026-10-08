### Changed

- **Tasks keep their place across a restart.** A task still at work when you quit or update
  purlis comes back under the chat that asked, on its conversation, and is told to carry on;
  its report still reaches that chat. A task that cannot be brought back on its conversation
  has ended by itself: the chat that asked is told, once, and it is a failed row you can
  Reopen. One whose start was refused (after Stop every agent, say) waits with the other chats
  that did not start, and Retry now brings it back told to carry on. Nothing is dispatched
  twice: a task is never sent its brief again, and a chat that asks for the same task again
  after it was started again, while the first is still working, is told it is running (#1513).
- **A task whose asking chat has closed keeps its report.** Its row reads "asked by <chat>
  (closed)". Its report is kept on its dispatch record and for the workspace, and when you
  bring the chat that asked back (Resume from any of its session records, Retry now, or the
  next launch) that chat is handed the report once, also when the report lands after you
  resumed it. So is a report it never read before it closed. A report a task sends after a
  restart names the session record it wrote last (#1513).
