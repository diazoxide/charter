### Added

- **Four more doctor fixes, each with a Fix button in the Doctor dialog and an id for
  `charter doctor --fix <id>`.**
  - `plugin-install`: installs charter's plugin for chats started outside the app. The
    `plugin install`, `plugin` and `plugin files` rows offer it. Bare `--fix` still installs the
    plugin, and now runs it once even when several rows offer it.
  - `local-ignore`: appends `/charter.local.toml` to `.gitignore` when git would commit that
    file. When git already tracks the file, the fix refuses and tells you to untrack it yourself.
    charter never runs `git rm` for you.
  - `memory-optimize`: appends a link to `MEMORY.md` for each memory the index is missing, and
    changes nothing else. Collapsing duplicates stays with `persona|workspace optimize --apply`.
  - `discover`: builds an empty inventory from the forges `charter.toml` declares. It goes over
    the network, so it runs only when named, as `charter doctor --fix discover` or its Fix
    button. Bare `charter doctor --fix` applies the other three, and `reinit`.

  No fix removes or replaces your content (FX-2, #1234).
