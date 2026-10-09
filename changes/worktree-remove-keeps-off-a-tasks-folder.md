### Changed

- **`purlis worktree remove` leaves a task's branch folder alone.** A folder purlis cut for a
  task is refused, `--force` or not, and the refusal says to discard it from the task's Changes
  tab, as the explorer's own Remove already did. A folder already gone, and an ended task's
  folder in a repo where Discard cannot run, are still removed, but never with
  `--delete-branch`: a task's branch can hold commits nothing else has. A dispatch record that
  does not read now keeps the folder it names from both, where before it kept nobody off it
  (#1534).
