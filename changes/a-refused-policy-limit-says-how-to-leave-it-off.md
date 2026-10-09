### Changed

- **A policy file's `0` for a limit that is off until set says how to leave it off.** A policy
  whose `tokens-per-session` or `minutes-per-task` is not a whole number in range is still
  refused whole, and its sentence now ends "remove the key to leave it off" (#1545).
