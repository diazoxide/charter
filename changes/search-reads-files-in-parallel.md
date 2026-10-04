### Changed

- **⌘⇧F reads a branch's files on several threads.** A search for something rare reads every
  file of the branch, and one thread left the disk mostly idle. Results still arrive in the same
  order, page after page, and Stop and the page's time limit work as before. Searching a
  100,000-file repo for a word it does not hold took 28–30 s and now takes 7–13 s (#1153).
