### Added

- **A branch cut for a chat that never started says so.** A writing chat's branch is cut just
  before its chat starts. If purlis is killed or crashes in between, the folder and the branch
  stay behind. The explorer now marks such a branch `unclaimed`, with how long ago it was cut.
  purlis never removes it on its own: start a chat in it, or remove its folder from its menu.
  Branches and worktrees made with plain git are never marked (#835).
