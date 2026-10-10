### Fixed

- **A vault tab reopened at launch reads nothing until you ask.** A 1Password vault's tab that
  was open when purlis quit no longer reads its token as purlis starts, which could make macOS
  ask about the Keychain or another app's data before you had done anything. The tab says it was
  open and reads the vault when you press *Read*. A keyring vault's tab still shows its table at
  once, since that reads no secret (#1660).
- **A token deleted in Keychain Access is no longer used until purlis quits.** Before purlis
  reuses a token it read earlier in the same run, it checks that the Keychain item is still
  there. The check does not read the token (#1660).
- **A 1Password Connect sign-in leaves the 1Password app's data alone.** An `op` handed a
  Connect token is told not to read the 1Password app's settings, as one handed a
  service-account token already is (#1660).
