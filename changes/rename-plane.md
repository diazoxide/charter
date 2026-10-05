### Added

- **`charter doctor --fix rename-plane` moves a project to purlis's names in one commit.** It
  renames `charter.toml` to `purlis.toml` and `.charter-scan-allow.toml` to
  `.purlis-scan-allow.toml`, rewrites the managed blocks' markers, a committed `workspace.json`'s
  digest key, the harness variable in `.claude/settings.json`, and personas' `charter:` skill
  references, and changes nothing else. Hook commands keep `charter`, which runs on every build. The project then requires the
  `purlis-names` feature, so a build without it opens the project read-only. It runs only when
  named, never from inside a chat, and refuses with uncommitted changes, outside git, or with a file under both names; a
  step that fails puts every file back (RN-7, #1265).
- **The consent rules are written under both names.** `init`, `reinit` and `rename-plane` write
  `purlis handoff`, `purlis report --yes` and `purlis … todo promote` ask rules beside the
  `charter` ones. Where the settings a chat runs under hold the `purlis` spelling at least as
  strictly as the `charter` one, the guard lets it through instead of refusing it; anywhere
  else it still refuses it.
