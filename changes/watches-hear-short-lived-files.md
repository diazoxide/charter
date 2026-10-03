### Fixed

- **A file made and removed at once is no longer missed by the project's watch, a branch's
  change markers or the kill switch.** Each folded the platform's events through a debouncer
  that took a removal arriving while the file's creation was still queued as the file never
  having been there, so a todo drawn in that moment could stay drawn, and a stop marker made
  and taken away inside a tenth of a second was not looked at. They now fold events the way
  the explorer's watch does, and a watch that loses track reads everything again (#1139).
