# A read guard only sees reads that happen. On PR #978 (2026-09-11) tests/

_2026-09-11 23:58 · persistent_

A read guard only sees reads that happen. On PR #978 (2026-09-11) tests/_planeguard open_ was made to refuse reads of the real charter.local.toml, and a sentinel probe still found doctor harness profiles row unrefused: for a file git would commit, the row answered from git alone and returned before reading the file. Fix was ordering (read through profiles.current() first, as harness list does), pinned by EverySurfaceIsRefusedTheOperatorsFile in tests/test_the_local_file_stays_out_of_git.py. When probing a new read guard, drive every surface down each early-return path, not just the happy path.
