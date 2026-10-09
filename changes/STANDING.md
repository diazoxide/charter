### Deprecated

- **The charter names stop working at 1.0.** Until then purlis reads its old names and writes
  only the new ones. From 1.0 it stops reading them: the `charter` command (run `purlis`), the
  `CHARTER_*` environment variables (set `PURLIS_*`), a project's committed files under their
  charter names (`purlis doctor --fix rename-plane` moves them), this machine's local state under
  its charter names, such as `.charter/`, `charter.local.toml` and the config and data homes
  (`purlis migrate` moves them), and keychain items under `charter/` (the same migration copies
  them to `purlis/`). The removal waits past 1.0 while `purlis doctor` still sees an old name in
  a project you opened, so run it before you update: it names every old name it finds. A
  migration waits until nothing named charter or purlis is running, and going back to a build
  from before the rename on the same machine needs `purlis migrate --undo` first. Every
  version's notes say this until 1.0 (ADR 0091).
