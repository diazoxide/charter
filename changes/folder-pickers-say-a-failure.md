### Fixed

- **A folder dialog that could not open is said.** *Open project…*, *Open a repo…* on the first
  run, *Browse…* in New project and *Locate…* in Settings › You › This machine used to treat a
  dialog that failed like a cancelled one and say nothing. They now say the failure where each
  already says a refused open; a cancel still says nothing. Forgetting or locating a project in
  This machine also clears the window's line about that project having gone, so its *Locate…*
  no longer answers that purlis does not remember the old path (#1291).
