### Fixed

- **A repo's own folder is compared with the upstream git uses.** Its change markers and how far
  it is ahead or behind now follow the remote's fetch refspec, as `@{upstream}` does, so a remote
  fetched under another name is still the base. A branch no refspec maps has no base, as before
  (#1130).
