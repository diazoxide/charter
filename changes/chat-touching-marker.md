### Added

- **See which file a chat is working on.** When a chat's file tool reads or edits a file, the
  explorer and the branch cockpit put a breathing dot on that file and on each folder above it,
  naming the chat on hover. The dot fades a few seconds after the chat goes quiet on the file.
  Claude Code and opencode chats drive it; Codex's hooks see only shell commands, so a Codex chat
  marks nothing. The path stays in memory and is never written to the event log or any other
  store (FM-6, #1109).
