### Changed

- **After a vault stops reading a variable, purlis asks you to remove its export.** Converting a
  1Password vault from an environment variable to a token kept in the keyring (from its tab, or
  with `purlis vault add --token-stdin`) now names the variable when no vault reads it any more,
  and asks for its `export` line to be taken out of your shell's startup files (#1542).

### Security

- **`purlis vault add --account` takes a sign-in address only.** It is held to the same rule as
  the guided set-up: a letter or a digit, then letters, digits, `.`, `_` and `-`. A pasted
  `https://…/` link is stored as the address it names (#1542).
