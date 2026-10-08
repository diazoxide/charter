### Added

- **Two ways to end a task: Stop and get its report, and Close now.** They are on a task's
  row menu in the Chats list, on the breadcrumb's line while a tab shows the task, and in the
  palette. Stop and get its report ends the task's turn and gives it one short turn to say
  what it did. Close now ends its program at once. Neither shows the close dialog, and
  neither offers a Smart close. Delete on a task's row asks for the first (#1488).
- **The chat that asked is told which, in purlis's own words**: stopped by the person, with
  the task's report quoted as data; closed by the person, with no report; or ended without a
  report, for a task whose program died. The first two add "The person ended this task. Do
  not dispatch it again unless they ask." A task cannot say any of this of itself. `purlis
  dispatch wait` returns the same word and `purlis dispatch list` says it (#1488).
- **One question, and only where it is needed.** An idle or finished task is ended as pressed.
  A task in the middle of a turn asks once: "Stop task '<name>'? It is working." A task with
  tasks of its own still working asks, in the same question, whether they are ended too or
  kept. Where purlis may not type into a task, the question says why and offers Close now
  alone (#1488).

### Changed

- **A task you ended says so in its own words.** Its finished row reads "stopped by you" or
  "closed by you", each with a mark of its own, and stays a row of its own until cleared. The
  interim "cancelled" and "closed by the person" are gone from the window; "cancelled" is a
  task its asking chat cancelled (#1488).
- **A task's menu no longer has Stop chat and Stop with everything below it.** A session and
  a chat a handoff opened keep both (#1488).
- **A report a task sends once you have pressed Stop is the stop's report**, whatever it says
  of itself, and its row does not fold. A task that reported before you pressed ended by
  itself, and the chat that asked is told nothing more (#1488).
