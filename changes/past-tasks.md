### Added

- **Past tasks, per workspace.** A workspace's menu, the palette and **See past tasks** under
  a chat's finished rows open a Past tasks tab: every task that was asked from the workspace
  or worked in it and has ended, newest first, with when it ended, who asked whom, how it
  ended, how long it ran and where it worked. Opening a row shows its report, what it said it
  changed and its brief, as plain text, and links to the session record its chat wrote. The
  list is narrowed by persona, by how a task ended, by a range of days and by the task's
  name, and takes in a task that ends while it is open. Times and days are in your local
  time. The project's root has its own, "Past tasks · project root", in the palette and on
  the project's menu. It reads only the dispatch records
  this machine keeps for the project (30 days after their last write), lists the newest 500,
  and says how many older ones, unreadable records and records it will not draw there are
  (#1510).
- **Reopen from Past tasks.** A past task can be reopened as an ordinary chat by the same
  rules as its finished row, including one whose row was cleared or went when its session
  closed. A task is reopened once (#1510).

### Changed

- **Clear finished says where the rows went.** Clearing finished rows leaves a line under the
  chat, "They stay in Past tasks", with **See past tasks** beside it (#1510).
