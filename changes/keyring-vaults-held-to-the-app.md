### Fixed

- **A sandboxed project with a keyring vault starts its chats again.** Both defaults together
  refused every chat. On macOS charter now writes every keyring item so that only charter's app
  reads it without asking you, so the sandbox holds a keyring vault for Claude Code, Codex and
  opencode, and Claude Code chats start. An item written before is moved under that rule the
  first time charter reads it, and the vault's next `charter secret get`, `cp` or `exec` says so
  once. The `charter` command is another program to the Keychain, so it now asks you before it
  reads a secret, until you choose Always Allow for it. On Linux, where the system keyring keeps
  no such rule, a Claude Code chat is still refused, and the refusal says how to go on: start
  that chat without the sandbox, or move those secrets to a plain-file or 1Password vault
  (ruling V90, #1179).
