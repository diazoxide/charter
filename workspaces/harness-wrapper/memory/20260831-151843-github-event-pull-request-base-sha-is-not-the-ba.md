# github.event.pull_request.base.sha is NOT the base the checked-out merge

_2026-08-31 15:18 · persistent_

github.event.pull_request.base.sha is NOT the base the checked-out merge ref was built on, and the gap is measurable: run 33331151759 checked out 'Merge 04bf8e6 into c29f3a8' while the payload said d40d998 — 3 main commits earlier. Never pass that field as a diff base. The right base for a refs/pull/N/merge checkout is the merge's first parent, which is exactly what 'git merge-base <merge> origin/main' returns, because nothing later than the first parent is reachable from the merge.
