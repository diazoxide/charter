### Fixed

- **A branch whose reads run past their memory cap is paused however busy the machine is.** A
  read that ran out of time counts against its branch only when it was given at least half its
  deadline. A read that ran past its memory cap now always counts, since what it took is the
  branch's own doing (#1605).
