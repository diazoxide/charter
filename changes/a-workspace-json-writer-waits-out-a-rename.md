### Fixed

- **A change to `workspace.json` that waited while the workspace was renamed is refused.** A
  settings save, a clone, a removal or a snapshot that was waiting its turn on the manifest
  while `purlis workspace rename` moved the folder used to go on at the old path, and could make
  `workspaces/<old>` again. It now says the workspace was renamed or removed while it waited,
  and writes nothing (#1292).
- **Edit as JSON refuses a repo's `branch` git would not take.** A branch starting with `-`, a
  full `refs/…` name, or one `git check-ref-format --branch` refuses, such as `a..b`, is
  refused, unless the file already held that exact record (#1292).
