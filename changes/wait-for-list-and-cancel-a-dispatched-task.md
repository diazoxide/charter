### Added

- **A chat can wait for a task's report, list its tasks and cancel one.** `purlis dispatch
  --wait` (and the `dispatch` tool's `wait`) holds the command until the report lands and
  prints it as the result, for 100 seconds unless `--timeout` says otherwise and 540 at most;
  a wait that runs out says the task is still running and how to look again, and
  `purlis dispatch wait <chat>` waits on a task dispatched earlier. `purlis dispatch list` (and
  the `dispatch_list` tool) prints each task's chat number, name, persona, place, state and
  age. `purlis dispatch cancel <chat>` ends the task's turn and asks it for one short report,
  which arrives with the outcome `cancelled`. A chat can do these only to tasks it dispatched
  itself (#1441).
- **An asking chat is told when a report lands.** If it is waiting for you and asking nothing,
  purlis types one line of its own into it, which starts the turn the report arrives on as
  context. Nothing is typed into a chat mid-turn, one showing you a prompt, or one you have
  started typing in. On Claude Code the report arrives with that turn; on Codex it does once
  you have trusted purlis's hooks there; on opencode it still waits for your next turn (#1441).
