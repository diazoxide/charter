### Added

- **The new-chat picker shows each persona's one-line description** (`agent-description`, else
  `description`) on its row (#1460).
- **The persona mark picker takes any colour as `#rrggbb`**, typed under the palette's swatches
  (#1454).

### Changed

- **`persona lint` warns about a `color:` purlis cannot draw**, and names the fix that rewrites
  Claude Code's `cyan` and `magenta` (#1460).
- **`purlis doctor --fix persona-agents` also removes what this machine kept for the retired
  sub-agents**: the in-flight records and the map of sub-agents to personas in the state
  folder (#1460).
- **The README roster block says what the committed dispatch log holds**, from before a persona
  ran as its own chat, and sends a reader to `persona stats` for the dispatches since. It no
  longer draws a share of "generic agent" dispatches (#1460).
- The persona view's label reads **Dispatch to it for**, and the persona skill says a chat's
  persona is fixed when it starts and teaches a task's `note`, `ask` and `report` (#1460, #1463).
- `posttooluse-message` is no longer wired into the plugin; an installed plugin that still names
  it is answered with nothing, as before (#1460).
