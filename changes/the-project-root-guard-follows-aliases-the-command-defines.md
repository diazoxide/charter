### Fixed

- **The project-root git guard follows an alias the command defines for itself.** Besides
  `-c alias.*` and the config files, an alias set through `GIT_CONFIG_COUNT` with
  `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n`, `--config-env` or `GIT_CONFIG_PARAMETERS`, on the
  command or earlier on the same line, is now followed to the branch move or reset it runs. When
  the command points git at a config file of its own (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`,
  `HOME`, `XDG_CONFIG_HOME`, an `include.path`), or sets a value only the shell fills in, a
  possible alias after it is refused with a sentence that says why. git's own commands still
  run (#1358).
- **A long line of `export`s no longer slows the git guards down.** Each segment used to get
  its own copy of every earlier export, so the cost grew with the square of the line; a long
  enough line could outlast the hook's deadline. It now grows with the line (#1358).
