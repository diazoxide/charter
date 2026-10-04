### Changed

- **The explorer marks a file you moved without staging it as renamed**, not as one file deleted
  and another added. Its change markers are now read by charter's one diff engine, which also
  computes a branch against its base, any two refs, what is not committed and a cross-repo
  change, each checked against `git diff`, for the Review tab to draw (RC-2, #704).
