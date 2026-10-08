### Changed

- **Tasks keep their place across a restart.** A task still at work when you quit or update
  purlis comes back under the chat that asked, on its conversation, and is told to carry on;
  its report still reaches that chat. A task that cannot be brought back has ended by itself:
  the chat that asked is told, once, and it is a failed row you can Reopen. Nothing is
  dispatched twice: a task is never sent its brief again, and a chat that asks for the same
  task again after the restart, while the first is still working, is told it is running
  (#1513).
- **A task whose asking chat has closed keeps its report.** Its row reads "asked by <chat>
  (closed)". Its report is kept on its dispatch record and for the workspace, and when you
  bring the chat that asked back (Resume from its session record, Retry now, or the next
  launch) that chat is handed the report once. So is a report it never read before it closed
  (#1513).
