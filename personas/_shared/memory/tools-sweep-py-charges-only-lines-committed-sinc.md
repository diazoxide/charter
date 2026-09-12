# tools/sweep.py charges only lines COMMITTED since the merge-base. With a

_2026-09-10 14:17 · persistent_

tools/sweep.py charges only lines COMMITTED since the merge-base. With a fix still uncommitted it printed 'carrying 4 uncommitted file(s) into the sandboxes', then 'diff against <base>: 0 file(s), 0 added line(s)' and 'NOTHING TO SWEEP' (exit 0), which is no evidence at all. Commit locally before sweeping, then push. Measured 2026-09-10 on fix-937.
