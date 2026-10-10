### Changed

- **Opening a project no longer waits on another project's cleanup.** The month-old session
  files of a project are collected as it opens, and that collection no longer holds the app's
  lock on its open projects: another project opens, closes and draws while it runs (#1027).
