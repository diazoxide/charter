### Fixed

- **`purlis discover` finds the repos of a GitLab user, not only of a group.** A GitLab owner
  that is no group was refused with `404 Group Not Found`. purlis now asks GitLab for that
  user's own repos, as it already did for a GitHub personal account; any other refusal still
  fails (#803).
