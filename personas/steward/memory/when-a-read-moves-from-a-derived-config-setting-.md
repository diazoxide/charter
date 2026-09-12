# When a read moves from a derived config setting to a lazy function (harn

_2026-09-11 23:44 · persistent_

When a read moves from a derived config setting to a lazy function (harness-profiles ruling 43: config.PROFILES removed, profiles.current() reads charter.local.toml when a surface asks), the _GUARDED_SETTINGS read guard in tests/_planeguard goes with the setting, and nothing stops a non-isolated test from reading the operator's real file. The review of a36194d measured it with a sentinel charter.local.toml at config.ROOT: profiles.current(), harness list and doctor all read it, and four doctor tests opened it. The fix is in _planeguard's open_ (refuse a READ of the real plane's charter.local.toml, basename compared first so other opens stay cheap), plus moving the tests that ran doctor's full row set onto PersonaIso. isolate_state_dir does not help: the file sits at the plane root, not in .charter/.
