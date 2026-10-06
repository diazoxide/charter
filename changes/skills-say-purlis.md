### Fixed

- **Every skill purlis ships names only `purlis`.** The smart-close skill still showed the old
  command for writing the session record, and the handoff, persona and update skills named old
  commands, files or variables. They all use the purlis names now, and Smart close's
  pre-allowed command is `purlis session record`, so writing the record does not stop on a
  permission prompt (#1329).
