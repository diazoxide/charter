### Changed

- **`purlis clone` into a workspace that is not there makes it.** The workspace is created and
  scaffolded first, as `purlis workspace create` does, and the clone says so, instead of
  refusing and asking you to create it. A clone the app runs for a sandboxed chat still clones
  only into a workspace that exists (#1382).
