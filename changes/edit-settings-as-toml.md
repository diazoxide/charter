### Added

- **Edit a project's settings files as TOML in Settings.** At the Project level, an "Edit as
  TOML" link for `charter.toml` and one for `charter.local.toml` sit under the groups. Each
  opens that file's whole text beside them. Nothing is written until you press Save, and
  Discard drops the edit. Text that does not parse, or that purlis would not read, is refused
  with the reason and nothing is written. If the file changes on disk while you are editing,
  the editor tells you, and Save is refused rather than overwriting the other change (SE-19,
  #1169).

### Removed

- **The separate Project settings page.** Its forms are the Project level's groups in Settings,
  and its Raw TOML view is Edit as TOML. Project settings… on a project's menu and in the
  palette opens Settings at the Project level. A Project settings tab left open by an earlier
  version comes back as that too (SE-19, #1169).
