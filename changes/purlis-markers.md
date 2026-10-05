### Changed

- **Every marker charter writes is recognised under the purlis name too.** The `.gitattributes`
  merge rules, the live-workspaces block in the `.gitignore`, the personas block in the
  project's README, the `persona sync-agents` marker and a workspace manifest's digest key are
  read under either name. They keep charter's spelling until the project is migrated to purlis
  names, so a teammate on an older build still finds them; after that each is rewritten in
  place under purlis's, never left beside a second one. Files that are never committed move to
  purlis now: a checkout's `info/exclude` block, the layer record (`.purlis-generated`), the
  structure stamp (`.purlis-structure`), a chat's `AGENTS.md` line and the opencode shim's
  first line. A file charter's record or digest owned stays charter's after the record moves,
  so it keeps being updated instead of reading as hand-edited (RN-2b, #1258).
