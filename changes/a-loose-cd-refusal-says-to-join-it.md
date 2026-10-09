### Changed

- **The project-root guard says how to join a `cd`.** A branch switch or reset refused in the
  project root, after a `cd` joined to it by `;`, `||`, `&` or a newline, now says that such a
  `cd` leaves the shell where it was if it fails, and to join it with `&&`
  (`cd <path> && git …`) so purlis reads where git runs. Nothing that was refused is allowed now
  (#1326).
