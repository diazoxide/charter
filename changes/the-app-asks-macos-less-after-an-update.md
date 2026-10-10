### Fixed

- **Reopening the app no longer floods macOS with Keychain questions.** The Vaults panel, which
  every project window draws as it opens, no longer runs the 1Password CLI, so it reads no
  token kept in the keyring and no 1Password app data; a vault's tab still checks it. The app also reads each kept token once per run rather
  than once per request, so several chats that reopen after an update ask once per token.
  `purlis persona list` and `persona show` no longer read a kept token to show a persona's
  vault either (#1654).
- **purlis no longer makes macOS ask whether it may "access data from other apps" for a
  1Password vault read through a service-account token.** The 1Password CLI read the
  1Password app's settings from that app's container on every run; purlis now tells it not
  to when the run signs in with a service-account token (#1654).

### Security

- **The docs say purlis needs no Full Disk Access.** macOS counts what a chat does as the
  app's doing, so a grant to the app reaches every chat it starts. *Updating* says to turn it
  off (#1654).
