### Added

- **A chat can dispatch a task to its own persona, and gets the report on its next turn.**
  `purlis dispatch --name "<task>" <<'BRIEF'`, or purlis's `dispatch` tool, starts a chat of its
  own for the task: as the asking chat's persona, on its harness profile and in its folder, with
  the sandbox the project gives that persona. What you allowed the asking chat alone, and a
  start without the sandbox, are not carried to it. Its first message says which chat asked and
  that the brief is a request from a chat. In the explorer it is listed under the chat that
  asked for it. It finishes with `purlis dispatch report --outcome done|blocked|failed "<text>"`,
  and the chat that asked reads the outcome, the text, what changed and the session record's
  path on its next turn, quoted as data. A dispatch to another persona answers `needs a grant`
  and starts nothing yet. Where purlis's hook can tell a sub-agent made the call, it is refused
  and told to return to its chat. One chat waits on at most 6 tasks, a lineage holds 16 chats
  that still owe work, and a chain goes 3 deep (#1436).
