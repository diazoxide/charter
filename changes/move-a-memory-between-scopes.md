### Added

- **Move a memory to the scope it belongs to.** A memory's tab has *Move to* and a *Move*
  button, and the command line has `charter workspace move-memory <slug> --to-…` and `charter persona
  move-memory <name> <slug> --to-…` (`--to-workspace`, `--to-persona` or `--to-shared`). The file
  moves whole, with its title and date; nothing is copied, and a target that already holds a
  memory of that name is refused (KN-3, #715).
