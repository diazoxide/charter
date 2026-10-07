### Changed

- **A chat's persona is fixed for its life.** `purlis persona use` inside a chat purlis started
  is now refused: it exits non-zero, changes nothing, and names the two ways forward, which are
  the operator allowing the vault on the chat's tab, or dispatching to that persona. `purlis
  persona create --use` is refused there too. Outside a chat both work as before. A persona is
  now defined as a role a chat runs as for its whole life, never a harness sub-agent, and
  ADR 0090 (chats working together by dispatch) is accepted as amended (#1435).
