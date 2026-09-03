# tools/sweep.py --path tests does NOT work, and its verdict is misleading

_2026-09-01 14:34 · persistent_

tools/sweep.py --path tests does NOT work, and its verdict is misleading rather than empty. Measured on 2e6eecb + a tests-only branch: --plan reports 38 mutations, then the run reports 'Ran 0 tests — no tests ran in 334s' and 'the tree is RED before any mutation'. The selection map is source-file -> covering-test-module, so a path under tests/ selects no covering modules and the unmutated baseline runs an empty suite. --path reads as if it took any directory. For a branch that changes only tests/, the honest sweep verdict is the default-path one: 'gate: nothing to sweep' with 0 added lines under charter/ — a #782 case, so hand-mutate the guards instead (revert the guard, watch the case go red, restore).
