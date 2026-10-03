### Added

- **Run a harness charter does not ship, from one file.** A project declares a terminal
  harness in `harnesses/<name>.toml`: its program, how it starts and resumes a session, and
  what it can and cannot report. It appears in the new-chat picker under its own name, asks
  you once on this machine before its program first runs, showing every word it will run, and
  again after the file changes, and runs in its terminal. `charter harness show <name>` prints
  any harness's declaration, including the three charter ships (FD-14, #654).
