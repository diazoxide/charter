### Fixed

- **A network refusal printed on standard output now raises the chat's sandbox block Notice.**
  A command run with `2>&1`, a failed command's output, and a background command's output file
  read back later were never read, so a database client's "no such host" inside
  `purlis secret exec … 2>&1`, or Docker's refused socket in a background run, raised nothing.
  Now each raises a Notice. A line on standard output counts only when it ends the way the
  refusal does, so a file that quotes the words raises nothing. A host named there is never
  offered to allow.
- **A refused name lookup names its host.** The Notice for a program that looks its host up
  itself (psql, usql, ssh, Go and Node programs) now shows the host it tried, and still
  offers no Allow, because allowing the host would not let such a program through.
- **Codex and opencode chats get a Notice for each host purlis's proxy refused them**, named by
  the proxy itself, with Allow, as a Claude Code chat does.
- **The chat that dispatched a task is told when the task waits on a sandbox block's Notice**,
  at its next turn, as it is told of a task's permission prompt.
