### Added

- **`purlis doctor` flags a landing the default branch no longer holds.** When the landing log
  names a commit purlis landed for a change, and the clone's default branch (pushed or local) was
  rewritten so it no longer holds that commit, the `changes` row fails, naming the change and the
  member. A commit this clone has never fetched is not read as lost (#878).
