### Fixed

- **Every refused write names its file, and blames a sandbox only where one refused it.** The
  last writes that printed a bare "Operation not permitted (os error 1)" now name what they were
  writing: a session record's and the reopen record's folders, the event log, rename-local's
  log folder and undo, the default workspace, a removed workspace or clone, a harness plugin's
  files, and the app's git and shell shims. A refusal a hook or the MCP server meets in a Claude
  Code chat is no longer told as the chat's sandbox's, since Claude Code's sandbox holds only
  its Bash tool. `purlis init`, `browser install` and a handoff's warning keep saying "this
  chat's sandbox refused it" in a sandboxed chat, and a refused `.gitignore` reached through a
  link names the file the link leads to as well. A sandbox's refusal of purlis's own write says
  that purlis does not grant its own writes to a chat, so the operator runs the command outside
  it, and on Codex and opencode it now shows the Report notice for purlis's own block (#1421).
- **A handoff from a sandboxed chat keeps its dispatch row.** The app writes the row where it
  opens the chat, since a sandboxed chat may not write the project's dispatch log, and a
  handoff's own lines start `purlis handoff:` (#1421).
