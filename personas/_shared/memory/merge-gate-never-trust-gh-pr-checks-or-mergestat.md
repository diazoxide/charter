# MERGE GATE: never trust 'gh pr checks' or mergeStateStatus=CLEAN to mean

_2026-08-26 20:59 · persistent_

MERGE GATE: never trust 'gh pr checks' or mergeStateStatus=CLEAN to mean CI passed on this repo. main has NO branch protection and NO required status checks (gh api .../branches/main/protection -> 'Branch not protected'), and GitHub is intermittently swallowing Actions push triggers here — a pushed sha can get total_count:0 check-runs while the PR still reports CLEAN and offers the merge button. Hit independently on two branches on 2026-08-26 (fix/558 and task7-focus-events), plus main's own run 32985668436 cancelled-after-16min and an earlier startup_failure. Always verify with: gh api repos/diazoxide/charter/commits/<HEAD_SHA>/check-runs — per head sha, not per PR. If total_count is 0, trigger it: gh workflow run test.yml --ref <branch>. Filed as #561.
