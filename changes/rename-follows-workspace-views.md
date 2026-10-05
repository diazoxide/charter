### Fixed

- **A renamed workspace's open tabs follow its new name.** A todo, a memory, the workspace's
  changes, its archive, a piece's files and a session record that were open when you renamed the
  workspace kept reading it under the old name, and said there was no such workspace. They now
  read it under the new name, and opening one of them again brings that tab forward instead of
  opening a second. A tab that comes back at the next launch comes back under the new name
  (#1248).
