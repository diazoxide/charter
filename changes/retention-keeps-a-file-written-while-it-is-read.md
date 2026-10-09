### Fixed

- **Collecting month-old session files keeps one written to while it was being read.** A
  trace that gained a line, or a file another took the name of, during the sweep is kept for the
  next one (#1027).
