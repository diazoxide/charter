### Fixed

- **A save says when it could not record how its push went.** Where the plane's push record could
  not be written or cleared (a full disk, say), `purlis save` now warns that the saving status may
  show the plane as it was until the next save, where it used to say nothing (#1144).
