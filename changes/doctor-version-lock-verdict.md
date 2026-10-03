### Changed

- **`charter doctor` checks the version pin.** Its `version lock` row used to name the pin and
  say it was not checked. It now gives the same verdict as `charter version`: a pin this charter
  meets, or one on the Python charter's line, is fine, and any other pin is a drift warning that
  points at `charter version` (HY-10, #573).
