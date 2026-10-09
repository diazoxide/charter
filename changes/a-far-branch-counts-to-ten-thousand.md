### Fixed

- **A branch thousands of commits from its base shows a number.** The branch cockpit stops
  counting commits ahead and behind at 10,000 and says "10,000+", where a very long count used
  to run out of time and show an error instead (#1152).
