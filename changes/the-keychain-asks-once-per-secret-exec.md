### Security

- **A sandboxed chat never makes the Keychain ask you for a vault.** When the app does not take
  a sandboxed chat's `purlis secret exec`, a vault whose values come from the keyring (a
  `keyring` vault, or a 1Password vault whose token purlis keeps there) is refused with a
  sentence saying why, instead of the command reading the Keychain from inside the chat.
  `purlis secret get` and `cp` in a sandboxed chat are refused such a vault the same way
  (#1638).

### Fixed

- **`purlis secret exec` reads a kept 1Password token once per run.** It read the token from
  the Keychain once for every value it handed on, so one run with three values asked you three
  times (#1638).
