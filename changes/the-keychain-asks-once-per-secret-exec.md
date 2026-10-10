### Security

- **purlis no longer reads the keyring from inside a sandboxed chat.** A vault whose values come
  from the keyring (a `keyring` vault, or a 1Password vault whose token purlis keeps there) is
  read for a sandboxed chat only by the app, through `purlis secret exec`. When the app does not
  take the run, and for `purlis secret get`, `cp` and every other read in the chat, the command
  refuses with a sentence saying why and what to run instead, rather than reading the keyring
  itself and raising a Keychain question on the chat's behalf (#1638). On Linux this is new
  for `get`, `cp` and every other read of such a vault in a sandboxed chat, which used to
  read it; there the refusal says to run it in a terminal outside the chat.

### Fixed

- **`purlis secret exec` reads a kept 1Password token once per run.** It read the token from
  the Keychain once for every value it handed on, so one run with three values asked you three
  times (#1638).
- **`purlis vault list` no longer reads a kept 1Password token.** Its status for such a vault
  says the token is kept in the keyring and that `purlis vault verify` reads it, so listing
  vaults never makes the Keychain ask (#1180).
