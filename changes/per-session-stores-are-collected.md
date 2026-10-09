### Fixed

- **The last per-session stores stop growing.** When the app opens a project it now also
  collects, a month after they were last written, the commit gate's per-session cooldowns, the
  first-edit-in-a-clone markers, the saved-record lines a workspace's chat left for a `Stop` that
  never ran, and a session's ephemeral memory, which goes whole once every file in it is a month
  old. A chat the project brings back keeps its own, however old (#1004).
