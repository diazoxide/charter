### Fixed

- **A file made and removed at once is no longer missed by the project's watch or the kill
  switch.** Both folded the platform's events through a debouncer that took a removal arriving
  while the file's creation was still queued as the file never having been there, so a todo
  drawn in that moment could stay drawn, and a stop marker made and taken away inside a tenth of
  a second was not looked at. They now fold events the way the explorer's watch does (#1139).
