### Changed

- **Each row of the Alerts drawer has its way out, in place of a command to type.** A version
  pin and a front door naming no persona open the project's General settings, where the version
  lock, the update channel and the default persona's picker are. Workspaces behind the layout
  have Reinit, the doctor's new `workspace-reinit` fix. A nested project offers Open the outer
  project, and a project root being worked in or a save that is blocked offers Go to Saving. A
  broken editor or text-size setting links to Settings › You, and a theme file charter could
  not use whole offers Use built-in…, which asks first and then moves the file aside to
  `theme.aside.json` (or the next free name), never over another file. The rest can be
  dismissed (NO-6, #1238).
- **The doctor's `version lock` and `front door` rows link to Settings › General** when they
  warn, as the Alerts drawer's rows for the same problems do (NO-6, #1238).

### Added

- **`charter doctor --fix workspace-reinit`** runs `charter workspace reinit --all`, which brings
  every workspace up to the current layout and never removes your content. No doctor row offers
  it yet, so bare `charter doctor --fix` does not run it (NO-6, #1238).
