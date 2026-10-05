### Changed

- **The explorer marks a file you moved without staging it as renamed**, not as one file deleted
  and another added, and **a file edited and then put back as it was at the base is no longer
  marked**. Its change markers are now read by purlis's one diff engine, which reads the
  contents of changed and untracked files (up to 128 MiB in one read; past that a file is
  marked without being read, so one put back to its base content is still marked changed) and also computes a branch against its base, any two refs, what is
  not committed and a cross-repo change, each checked against `git diff`, for the Review tab to
  draw (RC-2, #704).
