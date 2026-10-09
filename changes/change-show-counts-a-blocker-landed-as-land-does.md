### Fixed

- **`purlis change show` counts a blocker landed only as `change land` does.** A blocker is ticked
  when its request merged and purlis landed it: in the landing log, with the clone's default
  branch still holding that commit, or as a landing purlis started. One merged outside purlis is
  crossed with the land gate's reason and still blocks its dependent, in `show` and in the
  changes view alike (#877).
