### Changed

- **A project's harness declarations are checked more closely.** A declaration may no longer
  take a name purlis gives a harness it ships (such as `claude-code`), since a chat on it would
  be taken for that harness. `[terminal] newline` must be one of the keys a terminal reads as a
  new line (`"\r"`, `"\n"`, `"\r\n"`, `"\u001b\r"` or `"\\\r"`); anything else is refused with
  a sentence saying so (#1119).
