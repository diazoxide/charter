### Changed

- **A hook line no longer starts `ps` on macOS.** To check that a line on the hook channel
  comes from inside its chat, purlis read the sender's parents and start time by running
  `/bin/ps` once per line, and once more as each chat started. It now asks the kernel in its
  own process, as it already did on Linux, so many lines at once no longer queue behind one
  another or delay a chat's start. What is admitted and what is refused is unchanged. Only
  macOS and Linux read a process's parents: on any other unix purlis has no answer, so
  nothing there is taken as inside a chat (#1407).
