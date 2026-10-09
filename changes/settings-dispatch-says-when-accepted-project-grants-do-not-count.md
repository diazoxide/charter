### Fixed

- **Settings › Project › Dispatch says when the project's grants you accepted are not
  checked.** In the first moments after purlis starts, it has not checked them against the
  project's git history yet: the next dispatch checks first, and where the history reads they
  count as before. Where the history cannot be read, none of them counts: a chat you are at asks
  you on its tab, and one nobody is at is refused and listed under Needs you. Only the arrival
  Notice said any of this; the table now says which at the top, with Read again, and greys
  each such grant with a note (#1543).
- **Not on my machine is no longer offered for a teammate's grant limited to one workspace
  that nobody here accepted.** It already allowed nothing here, and pressing it recorded a
  decline in purlis's event log for a grant this machine never followed. It is refused now
  if asked for, and nothing is recorded (#1543).
- **Settings' list of dispatch grants no longer holds the window while it asks git** who
  committed each of the project's grants. The list, and the list a Revoke answers with, are
  read off the window's thread (#1543).
