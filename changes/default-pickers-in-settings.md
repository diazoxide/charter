### Changed

- **Settings picks the default persona, workspace and harness from what the project has.**
  Project › General's *Default persona* and *Default workspace*, and Harness & profiles' *Default
  harness* and *Default profile*, are pickers over the project's personas, workspaces and harness
  profiles instead of free text. Each has a *New…* entry: New persona… and New workspace… open the
  usual dialog and pick what they made, and New profile… opens `charter.local.toml` under Edit as
  TOML. A value set by hand that names nothing is shown as such, with charter's sentence about it
  beside the setting, until another is picked. A save that would make `[persona] default` or
  `[workspace] default` name nothing is refused, as one for `[harness] default` already was
  (ST-1, #1225).
