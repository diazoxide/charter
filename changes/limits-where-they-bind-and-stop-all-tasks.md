### Added

- **Stop all tasks.** A session's row menu and its tab menu have it beside Stop with
  everything below it, and its tab's task menu has it as the last line of "End a task". It
  asks once, naming how many tasks it ends, stops every task at work below the session as Stop
  and get its report does, and keeps the session running. The session is woken once, when the
  last of them has ended, not once per task (#1498).
- **A limit is shown where it binds.** A session's tab menu ends with `4 of 6 running`. A
  session whose dispatch was refused for a limit says which one under its row (`at its task
  limit (6)`, or its chain's, or a persona's across the project), with a link to Settings ›
  Project › Dispatch, until a slot frees (#1498).
