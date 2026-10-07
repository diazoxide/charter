### Fixed

- **Hooks of one chat that fire at once no longer lose a line while purlis is away.** When
  purlis does not take a hook's line (it is restarting, has quit or is slow), the hook keeps the
  line on disk for the next open of the project. Every hook of a chat wrote to one file, one at
  a time, and gave up after 250 ms of waiting for its turn. With several tool calls or
  sub-agents reporting together on a busy machine or a slow disk, some hooks ran out of that
  wait: each said on stderr that its line was lost, and that tool call or report was missing
  from the chat's record. Each hook now writes a file of its own and waits for no other hook.
  A line is still said to be lost when the disk refuses it or the chat already has 16,384
  lines waiting. When the disk takes more than a second over it, the hook now says the line
  may be lost, since the write can still finish and be read at the next open. Lines an
  earlier version left waiting are read as before (#983).
