### Fixed

- **Reading a branch waits its turn, and a branch that keeps hanging is paused.** purlis reads
  at most six branches at once. A read waiting for a place is now let in before any read that
  came after it, so it no longer runs out of time while newer ones go first. A branch whose
  reads ran out of time or memory twice in a row is not read again for a pause that grows each
  time it happens again, so it cannot keep the places full for other branches, comparisons and
  searches. When a read does run out of time, the message names the time it actually had. A
  file tab now also marks a chat that touched a file before the tab could name it, and finds
  its branch again after a lookup that failed (#1605).
