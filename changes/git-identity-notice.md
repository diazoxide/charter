### Added

- **A missing git identity is a Notice you can fix where it stands.** When the doctor finds
  git's `user.name` or `user.email` unset, the project's window says so under the strip, with
  Fix. Fix opens the same name-and-email form as the Doctor row: a key already set is shown
  locked, a refusal is shown under its field, and the Notice goes once the doctor checks again
  and finds the identity set. Dismiss lasts until the identity is set, and the Notice shows
  again if it goes missing later (#1250).

### Fixed

- **A git identity name has to show something.** The git identity fix refuses a name made only
  of characters that draw nothing on their own, such as Hangul fillers, the braille blank,
  combining marks or variation selectors. It also refuses a name or email that looks like a
  secret, as every other writer does, naming its kind and never repeating the value (#1250).
