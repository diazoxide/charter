### Changed

- **Show what changed reads its branch in one bounded read.** Finding the branch's folder no
  longer runs git in the app itself: the reader that already compares the file finds the folder,
  checks the path inside it and reads the comparison, in one child that a deadline and a memory
  cap bound. A refusal reads as it did (#1189).
