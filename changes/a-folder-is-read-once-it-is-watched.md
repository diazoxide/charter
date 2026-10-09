### Fixed

- **A file made while a folder opens in the explorer is drawn.** The folder was read before it
  was watched, so a file an agent made in between was missing until something else changed
  there. A folder is now read once its watch holds (#1427).
