### Added

- **What a task changed.** A finished task's row has **Changes** (**Review changes** for a
  task on its own branch), and its report's line about what changed opens the same tab. A
  task on its own branch lists everything its branch changed, each file opening its diff. A
  task that worked in a folder other chats share lists the files its own edit tools wrote
  that are still uncommitted there; it cannot see edits made by a shell command, what the
  task already committed there, or anything after the app is started again, and says so.
- **Merge and discard a task's own branch.** In its Changes tab, **Merge…** lands exactly the
  commit you were shown in the branch it was cut from, as a fast-forward or not at all, and
  **Discard branch…** removes its folder; each asks first. A merge that does not apply
  cleanly changes nothing and says why. Only you can have purlis merge or discard: no chat
  can, through purlis.
- **Two tasks of one chat working in one folder with no branch of their own are named**, both
  of them, when the second starts and on the asking chat's tab (#1511).
