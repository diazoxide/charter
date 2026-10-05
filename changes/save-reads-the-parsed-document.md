### Security

- **A project save reads a JSON or TOML file as the document it is, not only as its text.** A
  string in either can spell a character as an escape, so the secret check now also scans the
  parsed values of each staged `.json` and `.toml` file, the way the settings editors already
  do. A file that does not parse is scanned as text, as before (#1295).
