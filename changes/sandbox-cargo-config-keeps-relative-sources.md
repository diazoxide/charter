### Fixed

- **A vendored or local-registry source in your cargo config works in a sandboxed chat.** A
  relative `directory` or `local-registry` under `[source]` in your own cargo config is now
  resolved where cargo resolves it, the folder above your cargo home, before it is copied into
  the project's cargo home, so it no longer points into purlis's cache folder. A path that climbs
  out of that folder with `..`, leads out of it through a link, or names no folder there is left
  out, with its whole `[source]` entry, and the copied file says so. The sandbox grants nothing
  new for it (#1364).
