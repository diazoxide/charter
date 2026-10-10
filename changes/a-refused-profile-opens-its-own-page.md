### Changed

- **A profile the new-chat picker refuses opens its own page in Settings.** Each refused profile
  has its own _Open … in Settings_ link. It opens the profile's page with its command in focus.
  A profile with no page of its own (one committed in `charter.toml`, or one renamed since)
  opens Harness & profiles. Before, one link opened Harness & profiles for all of them (#1296).
- **A link to a Settings page that is no longer there opens the group it was under.** Before,
  it opened the level's first group (#1296).
