### Added

- **Four more doctor fixes, each with a Fix button in the Doctor dialog and an id for
  `charter doctor --fix <id>`.**
  - `plugin-install`: installs charter's plugin for chats started outside the app. The
    `plugin install`, `plugin` and `plugin files` rows offer it. Bare `--fix` still installs the
    plugin, and now runs it once even when several rows offer it.
  - `local-ignore`: appends `/charter.local.toml` to `.gitignore` when git would commit that
    file. When git already tracks the file, the fix refuses and tells you to untrack it yourself.
    charter never runs `git rm` for you.
  - `memory-optimize`: links each memory its `MEMORY.md` is missing, through
    `persona|workspace optimize --all --apply`. It moves extra exact-duplicate copies into
    `memory/archive/` and deletes nothing.
  - `discover`: builds an empty inventory from the forges `charter.toml` declares.

  No fix removes or replaces your content (FX-2, #1234).
