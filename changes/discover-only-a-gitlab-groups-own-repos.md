### Fixed

- **`purlis discover` on a GitLab group lists only the group's own repos.** A repo another
  namespace had shared with the group was listed as if it were the group's. GitHub's
  organisation listing has no such case (#804).
