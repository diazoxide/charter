### Security

- **purlis no longer reads the keyring from inside a sandboxed chat.** A vault whose values come
  from the keyring (a `keyring` vault, or a 1Password vault whose token purlis keeps there) is
  read for a sandboxed chat only by the app, through `purlis secret exec`. When the app does not
  take the run, and for `purlis secret get`, `cp` and every other read in the chat, the command
  refuses with a sentence saying why and what to run instead, rather than reading the keyring
  itself and raising a Keychain question on the chat's behalf (#1638).

### Fixed

- **`purlis secret exec` reads a kept 1Password token once per run.** It read the token from
  the Keychain once for every value it handed on, so one run with three values asked you three
  times (#1638).
