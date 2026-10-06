### Fixed

- **`purlis secret exec` works from a sandboxed chat.** Inside a chat the app started sandboxed,
  `purlis secret exec <vault> … -- <command>` no longer reads the vault itself, which the
  sandbox correctly forbids. It asks the app over the chat's hook channel. The app checks that
  the vault registry tags the vault with the chat's persona, resolves the secret, and runs the
  command in a sandbox built from what the chat's own sandbox was compiled to when it started.
  Output streams back with each value's literal text masked, followed by the exit status. A
  vault the persona may not use is refused with a sentence. Outside an app chat, the command
  runs as before (#1407).

  What this keeps from the chat: the vault's storage and its provider's session, every vault
  not tagged for the chat's persona, and a `--file` credential, which only the command may read.
  It does not keep the values from the chat: the command is the chat's own, so the chat can
  obtain any key of a vault tagged for its persona.

### Security

- **A live line on the hook channel is read only from inside the chat it names.** A chat's token
  can be read by any process of the same user on macOS, so the token alone no longer identifies
  a chat. The app records each chat's program as it starts and forgets it when it ends. It reads
  a line only from that program or from a process in its session or below it. A line from
  outside the chat (tmux, `nohup` after its shell ended, `docker exec`) is refused with a
  sentence saying so. Reports a hook spools while the app is away are not covered yet: they are
  still checked by the token's key alone (#1407).
