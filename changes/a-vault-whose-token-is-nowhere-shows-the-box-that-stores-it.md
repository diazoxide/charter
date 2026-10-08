### Fixed

- **A vault whose token is nowhere shows the box that stores it.** The tab of a 1Password vault
  read through a service-account token drew that box only after the vault had been read, which
  takes the token. So a purlis opened from the Dock, which never sees what a shell exports,
  showed the refusal and no way out. The tab now draws the refusal and the box under it, and
  lists the vault's secrets by itself once the token is stored. One paste serves every vault
  read through the same variable. The refusal names the keyring first, then the export, and no
  longer offers unbinding the vault as a fix. `purlis doctor` has a `vault tokens` row: for each
  such vault, whether its token is in the system keyring, in this environment only, or nowhere
  (#1526).
