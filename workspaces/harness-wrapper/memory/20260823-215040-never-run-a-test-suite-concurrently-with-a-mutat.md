# Never run a test suite concurrently with a mutation battery: the battery

_2026-08-23 21:50 · persistent_

Never run a test suite concurrently with a mutation battery: the battery owns the working tree while it cycles, so a suite launched alongside it imports mutated source and reports failures that look like real defects (cost ~20min chasing two phantom guard failures in #376). Same cause makes 'git diff' show mutations the file no longer has. Verify with md5 before/after the battery.
