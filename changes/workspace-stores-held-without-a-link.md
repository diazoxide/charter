### Security

- **A link a chat plants in its own workspace no longer carries your writes anywhere else.** Your
  `purlis` commands and the app now write a workspace's todos, memory, session records and
  changes the way purlis's MCP tools do: with no link anywhere on the way, and through the
  directory they opened. A store that is a link, or a link inside one, is never followed by a
  write, even when it points at another workspace or a persona inside the project: the write is
  refused and writes nothing. The readers that list and read those stores leave such a link out
  too, apart from a few that #1083 tracks (the doctor's memory rows, some counts, and the landing
  logs). A store whose lock a chat keeps is refused after a few seconds instead of hanging your
  command (#1064).

### Fixed

- **A memory whose index line cannot be written is no longer left on disk.** When the line could
  not be added to `MEMORY.md`, the memory file stayed behind with nothing listing it. It is now
  removed again, for `purlis`'s commands, the app and the MCP tools alike (#1058).
