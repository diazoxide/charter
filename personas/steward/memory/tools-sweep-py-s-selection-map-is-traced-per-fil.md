# tools/sweep.py's selection map is traced per FILE, so a charter module i

_2026-09-11 23:26 · persistent_

tools/sweep.py's selection map is traced per FILE, so a charter module is charged to every test module that executes any of it — and importing a module executes its body. Measured on harness-profiles Task 1 (2026-09-11): charter/profiles.py, imported by config.derive, cost 347 test modules per mutation (117 of the branch's 156 mutations), and every CI sweep shard hit its 60-minute limit with no verdict. The fix (ruling 43) had to cover every module-level import, not just the config.derive call: commands.py spelled its constant instead of importing profiles, commands_harness.py and doctor.py import it inside the function that uses it, and tests/_planeguard.py (run at tests-package import, so charged to every test) spells the filename literally. A test then pins the spelled copies equal to profiles.LOCAL_FILE.
