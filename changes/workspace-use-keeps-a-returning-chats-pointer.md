### Fixed

- **`purlis workspace use` no longer removes a returning chat's workspace pointer.** Its 30-day
  clean-up of the per-session and per-terminal pointers in the project's state folder now
  follows the rule the app uses when it opens a project: a chat the app will reopen keeps its
  month-old pointer and lock, the tool gate's files are left to the gate, and nothing is
  removed through a link (#1027).
