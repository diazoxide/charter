### Fixed

- **A report kept for a chat that comes back is not lost on the way.** If purlis quits while the
  chat that asked is starting again, or the report cannot be written for it, the report stays
  owed on its dispatch record and reaches the chat next time. Reopen on a finished task's row
  now also hands it the reports of the tasks it had asked for that were kept while it was gone.
  Try to start again no longer tells a task to carry on once its dispatch has ended, and a
  task is told to carry on after a restart only where it runs on the profile and in the folder
  it was dispatched with. An answer to a question a task asked before a restart says the
  question was not kept and that the task will ask again (#1546).
