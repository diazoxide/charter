### Security

- **A chat can no longer plant charter's settings or state under their coming purlis names.** The
  sandbox now denies a chat writing `purlis.toml`, `purlis.local.toml` and `.purlis/app` as it
  already denied `charter.toml`, `charter.local.toml` and `.charter/app`, and a chat may not
  create either state folder at the project root when it isn't already there. An extension may not
  declare either name as a write path, a chat's commit may not change `.purlis-scan-allow.toml`,
  and the vault guards cover `.purlis/vaults`. charter reads a project's files under either name,
  the purlis one first, and keeps writing the file the project already has (#1257).
