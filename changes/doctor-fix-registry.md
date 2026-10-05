### Added

- **The doctor fixes what it can, from the terminal and from the window.** A doctor finding that
  charter can fix carries a fix id, which `charter doctor --json` prints as `fix` on that row.
  `charter doctor --fix <id>` applies that one fix. Bare `charter doctor --fix` applies every fix
  the rows offer, on top of its plugin and ask-rule repairs. Each fix says what it changed or why
  it was refused. The Doctor dialog has a **Fix** button on those rows, which applies the fix and
  checks again. The first fix is `reinit`, offered when the project is missing a baseline folder.
  It adds what is missing and never removes or replaces your content. Removing a git index lock
  is never a fix (FX-1, #1224).
