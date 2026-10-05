### Fixed

- **A commit you make in an editor opened from a chat is no longer stamped as the agent's.** An
  editor or terminal started from a chat keeps the chat's environment, and its commits used to
  carry the chat's `Assisted-by` and `Charter-*` trailers. purlis now stamps a commit, and a
  `purlis save`, only when it runs below the program the app started for that chat. Commits a
  sandboxed chat makes get no trailers yet, and neither do commits on Windows: where purlis
  cannot tell, it leaves the message as written (#1018).
