### Changed

- **A task can be started fresh where purlis can hand it its brief again.** The core no longer
  refuses Start fresh for a task whose brief it confirms as the one its dispatch was sent: the
  task starts with no conversation, is handed that brief under purlis's stamp, and is still its
  asker's task, owing its report. Where the brief cannot be confirmed (a dispatch from before
  purlis was started again, a record changed since, or none kept), Start fresh is still refused
  with the same sentence (#1609, #1489).
