### Changed

- **Remembering and todos work from a sandboxed chat, one in a clone included.** In a chat purlis
  started, `purlis persona remember` (the persona's own memory and `--shared`), `purlis workspace
  remember`, `purlis workspace note` and `purlis workspace todo "<text>"` hand the write to purlis
  over the chat's own connection, and print what they always printed. purlis writes it in the
  chat's own workspace or persona, credits it to the chat, and holds each chat to 60 such writes a
  minute. One such write holds at most 12 KiB, and its title is one line. No such write touches
  git's, a hook manager's, a harness's or an editor's folders, an ignore file, or the sandbox
  settings (#1333).
- **A memory title is always one line.** A newline in a title no longer reaches the memory's
  heading or its index line, whichever command wrote it (#1333).

### Added

- **A `persona_remember` tool** on purlis's MCP server, beside `memory_add`: one memory of the
  chat's persona, or of shared memory, written by purlis for the chat (#1333).

### Fixed

- **purlis's `session_record` and `persona_remember` tools never write outside a chat's
  sandbox.** In a chat purlis started they hand the write to purlis, and where purlis cannot
  take it (under Codex, which gives the tools no connection to purlis, or when the hand-over
  fails) they refuse and name the command to run in the chat instead: `purlis session record`
  or `purlis persona remember`. Outside such a chat, or once purlis has quit, they write as
  before (#1408).
