### Added

- **Remove a repo the workspace names but this machine has not cloned.** Each row under *Not
  cloned here* in the explorer has *Remove…* beside *Clone*, and the row's menu (and the bottom
  bar's) has *Remove <repo> from workspace…* below the line. Settings › Workspace › Repos lists
  the same repos under *Not cloned here*, each with *Remove from workspace…*. Each one asks
  first, then takes only the repo's row out of `workspace.json`; nothing is deleted. A repo
  that is being cloned is not offered it, and a cloned repo still goes through the existing
  removal, which refuses work that is not pushed (#1228).
