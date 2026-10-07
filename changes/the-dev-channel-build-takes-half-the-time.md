### Changed

- **A dev channel build takes about half the time.** It is compiled with a lighter profile,
  `dev-release` (thin LTO), and makes no `.dmg`; a first install of a dev build on macOS uses the
  `.app.zip`. A stable build is unchanged. `purlis --version` and About purlis now name the
  profile a build was made with, for example `purlis 0.4.2 (dev-release profile)`. A nightly
  profile guard builds `main` with the stable profile and opens an issue if it fails (ADR 0092,
  #1429).
