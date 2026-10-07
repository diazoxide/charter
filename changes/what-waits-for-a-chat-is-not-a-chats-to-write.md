### Security

- **A sandboxed Claude Code chat can no longer read or write what waits to be told to another
  chat.** A dispatched chat's report, and purlis's own word on a dispatch you were asked
  about, wait in the project's state folder until the chat they are for takes its next turn.
  That folder is now denied, for reading and writing, to a sandboxed chat of a harness whose
  hooks run outside its sandbox: a chat could have written a line there that another chat
  would have read in purlis's voice. purlis's hooks deliver, outside the sandbox.
- **On Codex and opencode that folder is still readable and writable by a chat.** Their
  sandbox is a wrap around the whole harness, so the hooks that deliver run inside it and
  must reach the folder. What holds there is the check each file gets as it is read. The
  denial reaches them once delivery moves onto the hook socket (#1457).

### Changed

- **A workspace renamed from inside a sandboxed Claude Code chat leaves its kept reports for
  the app to move.** `purlis workspace rename` says so, and the app moves them when the
  project is next opened.
