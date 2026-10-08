### Fixed

- **npm's and pnpm's own setting names no longer read as an npm token.** The commit scan and
  every guard built on it took `npm_` and any sixteen more letters, digits, `_` or `-` for a
  token, so a file that set pnpm's store or cache folder through npm's environment spelling was
  refused as holding a forge token. An npm token's body is letters and digits only, and the
  rule now asks for that, so the setting names pass and a real token is still refused (#1364).
