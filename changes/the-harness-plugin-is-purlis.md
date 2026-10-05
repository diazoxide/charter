### Changed

- **The harness plugin is `purlis`.** A Claude Code chat the app starts loads it as
  `purlis@inline`; its skills are `purlis:<skill>` and its MCP tools `mcp__purlis__<tool>`.
  Every id it had before (`charter@inline`, `charter-app@inline`, and `charter@charter-app`, the
  copy `charter plugin install` wrote) is turned off in each chat, so an old copy never loads
  beside the new one, and a project file that still names one is told the new id.
  `charter plugin install` now registers `purlis@purlis-app` and writes the opencode guard as
  `purlis.ts`, taking out the install under the old names in the same write. The local migration
  (`purlis migrate`, and the app at its launch) moves an existing install for every harness, and
  `purlis migrate --undo` puts it back under the old ids. A project's own permission rules that
  name `mcp__charter__<tool>` need the `mcp__purlis__<tool>` spelling to keep applying (RN-8,
  #1266).
