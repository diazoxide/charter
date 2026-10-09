### Fixed

- **A branch thousands of commits from its base shows a number.** The branch cockpit stops
  counting commits ahead and behind past 10,000 and says "10,000+", where a very long count
  used to run out of time and show an error instead. A branch exactly 10,000 commits away shows
  10,000 (#1152).
