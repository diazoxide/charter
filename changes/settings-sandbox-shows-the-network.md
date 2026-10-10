### Added

- **Settings › Sandbox › Network shows what chats here can reach.** The page that was Granted
  is now Network, with three lists. **Open hosts** spells out every host of each Internet access
  preset in force and the project's own hosts. **Allowed** is the old Granted list: each host,
  folder, vault or persona's hosts you or a teammate allowed, with its scope (this chat, this
  project on this machine, or everyone in the project), who allowed it and when, and **Remove**
  in place of Revoke. **Blocked lately** lists what chats here were refused in the last 30 days,
  with the host, the chat and the time, and **Allow** for a host, kept for this project on this
  machine or for everyone in it. Each chat also has a **Network** view on its tab's menu: what it
  can reach now, by Open, Persona and Allowed hosts, and what it was refused. A chat started
  without the sandbox says "Not sandboxed: can reach anything". Nothing about what a chat may
  reach changes (#1662).

### Changed

- **Sandbox blocks are kept in a network record on this machine, for 30 days.** Each block,
  Allow and removal is written to `<data>/network/<project key>.jsonl` in purlis's data home,
  with the chat, its persona, the host and port of a refused connection, the scope and who
  decided. It is never in the project, never committed and never sent. It replaces the project's
  `.purlis/app/sandbox-blocks.json`, which is no longer written and can be deleted. `purlis
  doctor`'s `sandbox blocks` row reads it and now also names the hosts refused most (#1662).
