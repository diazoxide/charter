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
- **An asking chat is told when a report lands.** Where purlis may type into it, purlis types
  one line of its own, which starts the turn the report arrives on as context. It types into a
  chat only on a harness it has measured (Claude Code; Codex once you have trusted purlis's
  hooks there), only once that chat has reported since it started, never in a turn that has
  shown you a prompt, and never after a key of yours in its pane until the chat next reports a
  turn. Anywhere else the report waits for the next turn, as before; on opencode it always
  does. A cancel sends Escape and its line under the same rules, and otherwise takes effect
  when the task's turn ends. One chat may have 16 waits under way at once (#1441).
