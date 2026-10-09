### Changed

- **After a vault stops reading a variable, purlis asks you to remove its export.** Converting a
  1Password vault from an environment variable to a token kept in the keyring (from its tab, or
  with `purlis vault add --token-stdin`) now names the variable when no vault of any project
  this machine opened reads it any more, and asks for its `export` line to be taken out of your
  shell's startup files (#1542).

### Security

- **`purlis vault add --account` takes a sign-in address only.** It is held to the same rule as
  the guided set-up: a letter or a digit, then letters, digits, `.`, `_` and `-`. A pasted
  `https://…/` link is stored as the address it names (#1542).

- **A 1Password token stored before the guided set-up is held to its item.** Records made since
  then already named the 1Password item a vault's secrets are kept in, so a commit that changed
  the item stopped the token being used. Older records named none. When the item is this
  machine's own (or the default), the next read now writes it into the record; when a commit
  chose it, the token is not used until you give it again from the vault's tab (#1542).

- **`purlis vault add --token-stdin` is refused in a chat of any project this machine opened.**
  It read only the open chats of the project the command named, so a chat of another project
  that named this one was not recognised as a chat. The projects are read from the machine store
  under your account's own home as well as where the environment points (#1542).
