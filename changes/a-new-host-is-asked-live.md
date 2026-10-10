### Added

- **A new host is asked about while the connection waits.** When a sandboxed chat reaches a host
  nothing lists, purlis's proxy holds the connection for about a minute while a Notice asks you.
  Allow lets the same command carry on, with nothing failing and nothing restarting. New hosts
  from one chat within about two seconds are one Notice, each listed whole. If nobody answers in
  time, the connection is refused and recorded, the Notice stays, and a later Allow applies at
  once and tells the chat to run the command again (#1666).
- **Allow for me on this machine is the main button**, for this project on this machine. Other
  scopes… offers Allow only for this chat and Allow for everyone in this project. Keep blocked refuses the waiting connection and tells the chat not
  to try again unless you ask.
- **Policy can turn live asks off, remove Allow scopes, and pin hosts that can never be
  allowed** (`live-asks`, `allow-scopes`, `never-hosts`). The Notice says when policy ruled a
  choice out, and who set it.

### Fixed

- **A host allowed already no longer offers Allow buttons that do nothing.** Its Notice says the
  host is allowed and that this chat takes it once it restarts, with Restart this chat. An Allow
  pressed for a host allowed already now closes the Notice and says why.
