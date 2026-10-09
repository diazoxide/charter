### Fixed

- **Settings › Project › Dispatch says when the project's grants you accepted do not count.**
  In the first moments after purlis starts, and while it cannot read the project's git history,
  no grant of the project's that you accepted counts on this machine. Only the arrival Notice
  said so; the table now says it at the top and beside each such grant, with Read again
  (#1543).
- **Not on my machine is no longer offered for a teammate's grant limited to one workspace
  that nobody here accepted.** It already allowed nothing here, and pressing it recorded a
  decline in purlis's event log for a grant this machine never followed. It is refused now
  if asked for, and nothing is recorded (#1543).
- **Settings' list of dispatch grants no longer holds the window while it asks git** who
  committed each of the project's grants. The list, and the list a Revoke answers with, are
  read off the window's thread (#1543).
