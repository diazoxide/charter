### Security

- **Every git call purlis makes uses a bare repository only where it is named.** Not only the
  git the app runs for a sandboxed chat: `purlis worktree add` from your own terminal and every
  other call too, so a bare repository made in a folder a chat writes is never found in place
  of the clone meant. A clone or worktree the app makes for a chat is also held to the very
  git directory it checked, not only to its path: one replaced between the check and the run is
  refused (#1415).
