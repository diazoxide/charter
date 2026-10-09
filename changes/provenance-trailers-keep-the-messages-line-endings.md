### Fixed

- **Provenance trailers follow the commit message they are added to.** A message written with
  CRLF line endings gets its trailer lines with CRLF too, and a `git commit -m` message whose last
  line starts with `#` gets the trailers after that line rather than before it, since git keeps
  such a line when no editor ran (#1021).
