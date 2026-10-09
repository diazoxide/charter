### Fixed

- **The Shared settings file refuses `[chat_env]`.** Which variables a chat is started with is
  this machine's, and purlis reads it from the Local file alone. A save or a move in Settings
  that would have put `[chat_env]` in the committed file, where nothing reads it, now says so
  and writes nothing (#1197).
