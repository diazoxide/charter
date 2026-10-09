### Fixed

- **A screen reader counts only tabs on the tab strips.** The projects, workspaces and chat-tab
  strips announced their `×`, settings gears and `+` buttons as members of the tab list, so the
  count was wrong and the list held items that are not tabs. Each strip's tab list now holds its
  tabs only. The buttons stay where they were and work as before (#1204).
