# A hand deletion-sweep harness must verify its restore with 'git diff --q

_2026-08-30 15:03 · persistent_

A hand deletion-sweep harness must verify its restore with 'git diff --quiet -- <file>', not only with a sha256 of its own in-memory copy: a run killed by a tool timeout (SIGTERM at 2 minutes) left a mutant on disk, and the next run read that mutant as its baseline and 'restored' to it. Two guards silently vanished from the branch before I noticed.
