### Changed

- **The app, the command line and the skills say purlis.** Messages, refusals, doctor rows,
  the command line's help and usage line, the window's text and the plugin's skills name the
  product purlis, always lowercase. What the window, a project or a harness reads by name is
  unchanged: the `charter` alias, the `CHARTER_*` variables still read, the commands a
  project's ask rules spell (`charter handoff`, `charter report --yes`, a todo's promote,
  `charter session record`) and every file name stay as they were until the project is
  migrated. Text written into a project's committed files (the generated persona agents, a
  new persona, the README's roster, `workspace.md`, the topology and the inventory) names the
  program `charter` until the project is migrated with `doctor --fix rename-plane`, and
  `purlis` after. The crates and packages are `purlis-core`, `purlis-cli`, `purlis-app`,
  `purlis-session-protocol`, `purlis-same-user` and `purlis-site` (RN-11a, #1269; RN-13, #1272).
