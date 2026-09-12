# On this plane with three agents sharing the machine, a local FULL suite

_2026-09-12 22:03 · persistent_

On this plane with three agents sharing the machine, a local FULL suite run is not evidence: measured 2026-09-12, four of them were killed at 90-97% of 13,090 tests (two at ~12,600, one at ~11,600) while sibling agents ran their own suites and sweeps. Coordinator ruling that day: run TARGETED modules locally — everything the change touches plus the real-tmux modules when touched — and read CI's 3.11-3.14 matrix as the full-suite evidence, job by job. A CI matrix red where the targeted runs were green is a finding to report, not a reason to start a local full run.
