### Fixed

- **A folder chain deeper than the system's path limit no longer blanks a branch's markers.** The
  walk for files git does not track read each folder by its whole path, so the first folder too
  long to name ended the whole read and no change was marked. Now such a folder is marked once,
  as not read past, and every other change in the branch is still marked (#1130).
