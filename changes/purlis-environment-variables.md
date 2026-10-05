### Changed

- **The product's environment variables are now `PURLIS_*`, and `CHARTER_*` still works.** Every
  variable that steers a command, set by you, by the app for a chat, or by a harness
  (`PURLIS_ROOT`, `PURLIS_HOME`, `PURLIS_WORKSPACE`, `PURLIS_CONFIG_HOME`, `PURLIS_DATA_HOME`,
  `PURLIS_LOG_DIR` and the rest), is read under its `PURLIS_` name first. Its `CHARTER_` name is
  read when the `PURLIS_` one is not set. Chats and extensions are given both names until 1.0,
  so hooks and scripts that read the old name keep working. A harness profile may set neither.
  Inside a chat, an inline override now has to use the `PURLIS_` name, because the chat already
  has the `PURLIS_` one set (RN-2d, #1260).
