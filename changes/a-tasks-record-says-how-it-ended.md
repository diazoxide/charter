### Added

- **A task's record says how it ended, what it says it changed and how much the two chats
  talked.** The Dispatches tab now shows a task's own outcome (done, blocked, failed,
  cancelled, or stopped by you), what its report says it changed under the report, and how
  many follow-ups, notes, questions and answers passed between it and the chat that asked. A
  dispatch that has not ended and whose chat is not open says *not open*, with no running
  clock (#1452).
- **`purlis persona stats` counts dispatches again.** `DISP` adds this machine's dispatch
  records to the committed dispatch log's count, so a persona you dispatch to is no longer
  marked *never dispatched*. The records are this machine's and are kept 30 days, and the
  notes under the table say so. Run in a sandboxed chat, which cannot read the records, the
  command says so and counts the log alone (#1452).

### Fixed

- **A dispatch record's interrupted write no longer leaves a copy of the brief behind for
  good.** The temporary file is collected with the records, 30 days after it was written
  (#1452).
