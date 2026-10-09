### Fixed

- **A host refused to a command a sandboxed chat runs with a vault now raises the chat's block
  Notice.** `purlis secret exec` in a sandboxed chat is run by the app, behind a proxy carrying
  the chat's hosts. When that proxy refused a host, the command said only what its client says
  (kubectl's "Forbidden"), no Notice came, and the chat was stuck. Now the chat's tab shows the
  sandbox block Notice naming the host and port the proxy refused, with Allow for this chat,
  Always allow and Keep blocked, and purlis says on the command's stderr that its sandbox
  refused that host and where to allow it. The host comes from the proxy's own record, never
  from the command's output. Once allowed, the chat is started again with it and the next run
  reaches the host.
