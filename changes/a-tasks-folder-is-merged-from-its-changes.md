### Changed

- **A task's own branch folder is merged and discarded from its Changes tab, not from the
  explorer.** The explorer's **Merge** and **Remove** on a branch folder purlis cut for a task
  now say whose it is and point to that task's **Review changes**, where the merge shows what
  would land and waits until the task has ended. The Dispatches tab's row of such a task has
  **Review changes** too. Where purlis runs no git in a repo (its own git settings name a
  program), an ended task's folder is still removed from the explorer (#1534).
- **A task's Changes list marks every other chat that wrote a file**, not only a task of the
  same chat: the asking chat's edits and any other chat's are marked with its name too.
- **Tasks sharing a folder are named whichever chat asked for them**, and **Ask a persona…**
  names the tasks already working in the folder you pick before it starts one there.
