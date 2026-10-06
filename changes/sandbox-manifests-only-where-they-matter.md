### Changed

- **A sandboxed chat can write a `purlis.toml` where it can't change a sandbox.** Before,
  `purlis.toml`, `charter.toml` and their `.local.toml` forms couldn't be written anywhere,
  so a chat couldn't check out a repo that carries one, write a test fixture or a scratch
  project, or run purlis's own test suite. Now they are denied only at the project root and in
  each folder from the chat's folder up to the root, the files purlis reads when it starts a
  chat there. Below the chat's folder and in temp folders they are ordinary files. A manifest
  purlis can't read still starts no chat (#1336).

### Security

- **A sandboxed chat still can't change its own sandbox through a manifest.** The project root's
  manifests, and those in each folder from the chat's folder up to the root, stay denied in
  every harness. A manifest kept as a link, such as a `purlis.local.toml` linked from a
  dotfiles repository, is held by the link itself as well as its target, so a chat can't
  remove it and put its own file in its place (#1336).
