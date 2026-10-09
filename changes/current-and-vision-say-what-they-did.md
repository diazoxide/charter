### Changed

- **`purlis workspace current` and `purlis persona current` say which rung decided.** The name
  on stdout is unchanged; a second line on stderr says where it came from (`• via cwd`,
  `• via session`), as `purlis status` and `purlis persona list` already did (#999).
- **`purlis workspace vision "<text>"` confirms the write.** It says `✓ Vision set for '<name>' →
  workspaces/<name>/workspace.md` on stderr instead of finishing silently (#999).
