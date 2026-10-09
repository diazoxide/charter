### Added

- **`workspace_vision` and `workspace_section` tools** on purlis's MCP server. A chat sets its
  workspace's vision, or adds one decision or one glossary term to its `workspace.md`, at any
  point in a session, not only at smart close. In a chat purlis started, purlis writes it for the
  chat, in the chat's own workspace, and credits it to the chat; what a section already holds is
  kept. `purlis workspace vision "<text>"` run in such a chat hands its write to purlis the same
  way (#1384).
