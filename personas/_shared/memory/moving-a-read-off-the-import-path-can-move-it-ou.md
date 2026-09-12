# Moving a read off the import path can move it out from under a test guar

_2026-09-11 23:39 · persistent_

Moving a read off the import path can move it out from under a test guard. Found reviewing PR 978 (harness profiles, ruling 43, 2026-09-11): tests/_planeguard refused the operator's real charter.local.toml only through the config.PROFILES setting. Once profiles.current() read the file lazily, profiles.current(), harness list and doctor all read the real plane's file in non-isolated tests, proved with a sentinel file and an audit hook. Four doctor tests were already opening it before the change. On a machine whose charter checkout IS the plane, that is the operator's own profiles. Rule: a guard belongs on the file open (planeguard's open_ refusing the _REAL entry), not on the setting that happened to carry the value, and moving where a value is read means re-checking what guarded it.
