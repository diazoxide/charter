### Changed

- **The product's environment variables are now `PURLIS_*`, and `CHARTER_*` still works.** Every
  variable that steers a command, set by you, by the app for a chat, or by a harness
  (`PURLIS_ROOT`, `PURLIS_HOME`, `PURLIS_WORKSPACE`, `PURLIS_CONFIG_HOME`, `PURLIS_DATA_HOME`,
  `PURLIS_LOG_DIR` and the rest), is read under its `PURLIS_` name first. Its `CHARTER_` name is
  read when the `PURLIS_` one is not set. Chats and extensions are given both names until 1.0,
  so hooks and scripts that read the old name keep working. A harness profile may set neither.
  A variable that chooses what a command acts on is refused rather than guessed when both names
  are set to different values. Those variables are the project, its state, the workspace, the
  worktrees, the persona, and the config and data homes. The refusal says to set both
  names to the same value (`PURLIS_ROOT=<x> CHARTER_ROOT=<x> charter …`). So a single name
  typed in front of a command inside a chat tells you what to type, instead of acting on the
  chat's own project (RN-2d, #1260).
