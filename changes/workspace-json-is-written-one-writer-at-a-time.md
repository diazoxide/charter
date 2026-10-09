### Fixed

- **Every change to a workspace's `workspace.json` waits for the one before it.** Saving the
  Workspace settings or Edit as JSON, `purlis workspace snapshot`, `fork`, `rename` and `reinit`,
  and the first manifest purlis writes for a new workspace, now take the same turn that clone and
  removal take, so none of them writes over a change made while it was reading the file (#1292).

### Changed

- **Edit as JSON refuses a workspace manifest its readers would misread.** A `name` that is not
  the workspace's folder, or `repos` that is not a list of `{"name": …}` records (each with an
  optional `"branch"`), is refused with the key it is about, unless the file already held it
  (#1292).
