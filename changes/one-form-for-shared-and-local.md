### Added

- **One form for Shared and Local.** At the Project level of the Settings tab, every value that
  either file may hold has a "Shared / Only on this machine" choice: pick the other and press
  Move to take it out of one file and put it in the other, both files or neither. A value
  this machine sets over the team's shows an "Overrides charter.toml" badge and what the team's
  file has. Every value says which level and file it comes from, and a Reset takes it out of
  that file so the value beneath shows through (with Undo). Profiles, the environment passed
  to chats and the default profile stay this machine's; the sandbox is never moved or reset
  (SE-18, #1168).
