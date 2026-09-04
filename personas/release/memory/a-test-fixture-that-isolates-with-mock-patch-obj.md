# A test fixture that isolates with mock.patch.object(config, 'ROOT', tmp)

_2026-09-03 16:56 · persistent_

A test fixture that isolates with mock.patch.object(config, 'ROOT', tmp) is NOT isolation — it patches one name while ~19 other config.DERIVED settings still point at the real plane. It breaks outright once the command under test calls config.use(): the patcher restores ROOT and leaves the rest in a deleted tmp dir. Use tests._isolation.point_config_at(case, root) (added #858), which snapshots exactly the set config.use writes.
