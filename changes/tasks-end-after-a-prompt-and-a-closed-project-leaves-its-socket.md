### Fixed

- **A task whose reporting turn hangs after you answer its prompt is still ended.** The time a
  task is given after its report used to be spent if it passed while the task was showing you a
  prompt, so a turn that then hung kept the task's program for good. It is given that time again
  once you answer. A task whose harness starts a turn of its own after its report is treated as a
  later turn: it is ended when that turn ends (#1525).
- **A task you read beside a view is not ended under you.** In a tab that opened on a view, such
  as a persona card, with chats split beside it, a task any of those chats shows now counts as
  one you are looking at, as it does in any other tab (#1525).
- **Closing a project no longer takes away the hook socket of the same project opened again.** A
  project that stops listening removes only the socket file it made, and stops at once even when
  that file was removed or replaced while it listened (#1525).

### Changed

- **purlis's refusals say "task" and "helper".** A dispatch past a limit says how many tasks are
  running, and a dispatch or handoff from inside a harness's own helper is refused in those words
  (#1525).
