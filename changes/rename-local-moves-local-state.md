### Added

- **Local state moves to the purlis names, and can be put back.** At launch, the app moves this
  machine's config home, the session host's folder in it, the data home, and in each project it
  remembers, `charter.local.toml` and `.charter/`, to their purlis names. `purlis migrate` and
  `purlis doctor --fix rename-local` do the same from a terminal, and move the log folder too.
  Approvals, pins, the device id, the update channel and the kill switch come along. Every move is
  journalled. A move that fails leaves the old name in place, where it is still read. Nothing
  moves while any charter or purlis window, chat or terminal is running, and that includes
  anything named charter or purlis, such as a dev build: quit them all first, and the app tries
  again at its next launch. Git ignores the new names through the repository's own
  `info/exclude`, so no committed file changes. `purlis migrate --undo` puts every move back, and
  an undo that cannot finish says what is in the way and finishes when run again. Going back to an
  older build on the same machine needs the undo first (RN-5, #1263).
