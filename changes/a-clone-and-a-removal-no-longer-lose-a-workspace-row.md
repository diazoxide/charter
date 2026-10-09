### Fixed

- **A clone and a removal at the same moment no longer lose a workspace's repo.** Cloning a repo
  into a workspace and removing another from it both rewrite the workspace's `workspace.json`.
  When the two ran at once, from the window and a terminal for example, the later write could put
  back the list the earlier one had changed, so a repo it had just recorded or removed came back
  or went missing. Each now waits for the other (#1249).
