### Added

- **Keychain items move to the purlis names, checked before they are used.** `rename-local` now
  copies each keyring vault's secrets and each stored 1Password identity token from its
  `charter/…` keychain item to a `purlis/…` one, reads every copy back, and only switches the
  vault or the identity to the new items once all of them read back the same. An item that is
  already under the purlis name is never taken on trust: when its value differs from the
  original, that vault stays on its old items and the run says so. The old items are kept, so
  `purlis migrate --undo` just reads them again, after first copying back any secret written
  since the switch. On macOS the app does the copy at launch, where reading its own items asks
  nothing, and `purlis migrate` in a terminal leaves it to the app. 1Password items keep their
  `charter-<vault>` titles (RN-6, #1264).
