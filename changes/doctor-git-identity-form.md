### Added

- **A missing git identity is fixed from a form.** When `user.name` or `user.email` is unset, the
  doctor's `git identity` row offers the fix `git-identity`. In the Doctor dialog, its Fix button
  opens a small form for a name and an email; in a terminal, it is
  `charter doctor --fix git-identity --name "Your Name" --email you@example.com`. Both are written
  to git's global config, the scope the row names. Each value is checked first, and a refusal is
  shown under its field and writes nothing (FX-3, #1235).
