### Changed

- **A sandbox grant for a persona the project does not have is refused.** Saving
  `[sandbox.personas.<name>]` in Settings for a persona the project does not define now says
  that it names no persona of this project and grants nothing, instead of being taken without a
  word. Make the persona first; it is read at the next read (#1407).
