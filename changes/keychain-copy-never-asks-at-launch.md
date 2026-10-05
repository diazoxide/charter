### Changed

- **No burst of keychain prompts after an update.** On macOS the app's launch copies keychain
  items to the purlis names with the Keychain's dialogs off. A vault whose items macOS would ask
  about (items an earlier build made) stays on its old items, where it keeps working, and the
  window offers **Finish moving N vaults**: only that press lets macOS ask, once per secret.
  `purlis migrate` in a terminal now copies the vaults whose secrets the command wrote itself,
  without asking, which also moves an install that has no app; `purlis migrate --undo` puts
  vaults moved either way back (#1306).
