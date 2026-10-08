### Added

- **A project can list the profiles every persona's tasks run on at once.** `"*" = ["work"]`
  under `[dispatch.profiles]` holds every persona that has no list of its own, and none up the
  chain it extends, a persona added later included. A persona's own list still answers first
  (#1522).
