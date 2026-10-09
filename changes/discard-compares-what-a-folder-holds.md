### Fixed

- **Discard compares what a task's branch folder holds, not only its paths.** A file you were
  shown that was written again, a file added inside a folder git ignores whole, and a commit
  made in a repository nested in the folder each count as a change, and nothing is removed
  until you are asked again. A nested repository is named in the question, since its history
  goes with the folder. The last look is taken in the same step as the removal, and no chat
  starts in the folder while it is being removed (#1472).
