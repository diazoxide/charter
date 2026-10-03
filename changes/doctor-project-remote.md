### Added

- **The doctor says who can read what your project pushes.** `charter doctor`, and the full
  doctor in the app, have a `project remote` row: whether the project's `origin` is public,
  internal or private on GitHub or GitLab, and whether the forge's own push protection (GitHub's
  push protection, GitLab's secret push protection) is on. A public remote without it is a
  warning, and a setting the signed-in account cannot see is never read as on. The session-start
  preflight asks no forge, so it has no such row (SQ-8, #587).
