### Changed

- **A repo nobody touches no longer costs a `git status` every ten seconds.** A clone's save
  standing (its Saving row, and auto-save's look at a repo that saves itself) is now read again
  when something in its working tree changes, heard by the same watch the explorer uses for a
  branch's changes, rather than on a ten-second clock. In a large repo one `status` can take a
  second of several cores, so an idle project with one open used to keep the machine busy. On
  Linux, where a tree is watched folder by folder, a clone is still read on a clock for now,
  but never more often than every 200 times what its last read took. The alerts now read the
  project root's changes from the same shared reading instead of a `git status` of their own,
  and purlis's own reads no longer ask git for the untracked cache, which a read that never
  writes the index could not use; saves still add it (FD-11, #651).

### Fixed

- **purlis's reads no longer take `index.lock` from under your `git add`.** Every
  `git status` purlis runs now leaves the index as it is, so a commit or an agent's `add` in
  the same repo is never refused because purlis was reading it (FD-11, #651).
