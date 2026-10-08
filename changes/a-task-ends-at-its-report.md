### Changed

- **A task ends at its report.** When a task reports, the report is delivered to the chat
  that asked and then purlis ends the task's program: after the turn that sent the report is
  over, or after a bounded wait on a harness purlis cannot hear. A finished task no longer
  stays open holding memory until somebody closes it. Its row stays under the chat that asked,
  as a finished entry whose outcome and report can be read. `purlis dispatch list` keeps
  listing it until its row is cleared or the asking chat closes, and `wait` still answers with
  its report; a cancel, a follow-up or an answer to one says it has finished (#1485).
- **What is never ended.** A task you type into after its report, or begin a Smart close of,
  is yours from then and stays open. A task you have in front of you is ended only when you
  move to another chat. A task that reported itself blocked stays open, and so does a persona
  chat you started yourself with Ask from a tab. A task reading the report of a task of its own
  is ended when that turn is over, never during it (#1485).
- **A task that had reported when you quit is a finished row at the next launch**, and is not
  started again (#1485).

### Added

- **Finished (n) and Clear finished.** Under each chat in the Chats list, the tasks that came
  out done or were cancelled fold into one "Finished (n)" line with Clear finished. A task
  that failed, was blocked, ended without a report or was stopped or closed by you stays a row
  of its own until it is cleared, whatever its own last report says. Clearing takes rows away
  and nothing else: the dispatch records stay. The rows are there again after the app is quit
  and reopened, and go when the chat that asked closes (#1485).
- **Reopen a finished task.** Reopen on a finished row resumes the task's conversation as an
  ordinary chat with a tab. It is no longer a task: a `purlis dispatch report` from it is
  refused in plain words, and the chat that asked is told nothing. If its harness cannot bring
  the conversation back, the row stays and says so (#1485).
