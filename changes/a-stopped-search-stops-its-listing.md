### Fixed

- **Stopping a search in files stops it at once.** ⌘⇧F's listing of a branch's files now hears
  Stop, so a branch whose ignore file holds git's listing no longer keeps the search running
  until git's 30-second deadline (#1137).
