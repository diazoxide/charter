### Added

- **One shape for every harness's ask.** A permission request from Claude Code, Codex, opencode
  or an ACP agent is read into the same ask: what it would do, the options in the harness's own
  words, who may answer, a deadline below the hook's timeout, a risk class, a one-line summary
  with credentials masked, and whether it elicits a secret. When two clients answer one ask, the
  first answer applies and the other hears "answered elsewhere"; a late, replaced or withdrawn
  ask refuses its answer and says why. Answering from the inbox builds on this (HP-5, #671).
