### Fixed

- **The branch cockpit follows a checkout in its folder.** After another branch was checked out
  in the cockpit's folder, a commit on it that wrote no file was not counted until the focus was
  set again. The cockpit now watches the branch checked out there, as soon as the checkout is
  heard (#1152).
