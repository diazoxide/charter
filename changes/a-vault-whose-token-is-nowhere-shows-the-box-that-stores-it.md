### Fixed

- **A vault whose token is nowhere shows the box that stores it.** The tab of a 1Password vault
  read through a service-account token drew that box only after the vault had been read, which
  takes the token. So a purlis opened from the Dock, which never sees what a shell exports,
  showed the refusal and no way out. The tab now draws the refusal and the box under it, and
  lists the vault's secrets by itself once the token is stored. A token is stored per vault:
  after a store the tab names the other vaults read through the same variable that still have
  none, each a link to its own tab. A token stored again replaces the old one, which is deleted
  from the keyring. When a kept token does not read the vault, the tab says the provider's own
  reason and blames the token only for a refused sign-in. The refusal now leads with the
  keyring, then the export, and no longer leads with unbinding the vault. `purlis doctor` has a
  `vault tokens` row: for each such vault, whether its token is marked as kept in the system
  keyring, in this environment only, or nowhere (#1526).
