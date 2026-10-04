### Changed

- **A repo nobody touches no longer costs a `git status` every ten seconds.** A clone's save
  standing (its Saving row, and auto-save's look at a repo that saves itself) is now read again
  when something in its working tree changes, heard by the same watch the explorer uses for a
  branch's changes, rather than on a ten-second clock. In a large repo one `status` can take a
  second of several cores, so an idle project with one open used to keep the machine busy. A
  repo that can't be watched whole (past the app's share of Linux's file watches) is still read
  on a clock, but never more often than every 200 times what its last read took. The alerts now
  read the project root's changes from the same shared reading instead of a `git status` of
  their own, and charter's own reads no longer ask git for the untracked cache, which a read
  that never writes the index could not use; saves still add it (FD-11, #651).
