### Added

- **A project marked by `purlis.toml` is found.** Ahead of the rename to purlis, a project is
  found by `purlis.toml` as well as `charter.toml`. This build opens it read-only, because some
  of its parts are still read only under the charter names. When a project holds something
  under both names, the doctor's *renamed leftovers* row names both. Keep both for now: the
  `rename-plane` fix will reconcile them (RN-1, #1254).
