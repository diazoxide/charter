### Changed

- **The question for several tasks reads like a chat's own sandbox Notice.** It says when a
  task's command is waiting on your answer, says what your administrator's policy ruled out,
  and offers its scopes in the same order: a host for you on this machine first, then only these
  tasks or everyone in the project under **Other scopes…**; a folder for these tasks first
  (#1709).

- **A database host given alone in `PGHOST` reaches its database through the tunnel.** A
  brokered `purlis secret exec` now reads its port from `PGPORT` (or libpq's 5432) and points both
  at the tunnel, as it does for a connection string (#1708).

- **A certificate checked by name still matches through the tunnel.** For a libpq connection
  string or `postgres://` URL with `sslmode=verify-full` (and `PGSSLMODE=verify-full` beside
  `PGHOST`), purlis keeps the host name and adds the tunnel as `hostaddr`, so the name the
  certificate is checked against is the database's own. Other drivers are pointed as before
  (#1708).

### Fixed

- **A connection is no longer held a minute when nobody can be asked.** In the first instant of
  a chat's start, or while the project's hooks are not listening yet, a connection to a host
  nothing lists is refused at once, as before purlis asked live (#1709).

- **The network record says how a host was allowed.** A host allowed for everyone in the
  project and taken live by a running chat is recorded as `project`, not `open`; a brokered
  command's connections are recorded by the layer that lists the host for the chat (persona,
  you, this chat), as the chat's own are (#1709, #1708).

### Security

- **Allow on Blocked lately allows only a connection the record refused.** A host the network
  record holds no refused connection to is not allowed from there (#1681).
