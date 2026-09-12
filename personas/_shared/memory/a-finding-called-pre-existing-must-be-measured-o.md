# A finding called PRE-EXISTING must be measured on MAIN, never inferred f

_2026-09-11 10:39 · persistent_

A finding called PRE-EXISTING, or called a REGRESSION, must be measured on MAIN in the SAME path and state. Never infer it from an earlier commit on the PR's own branch, or from a repro of a similar-looking path. On PR 948 (2026-09-11) the verdict flipped twice:
- A round-4 reviewer called a reinit crash with "refs/ at chmod 000" pre-existing, because the branch's previous head (be14e9d) crashed the same way. That compared the branch with itself.
- forge then measured main 3286a4f with a CLONE's .git/refs at 000 and got rc 0. On that basis the controller re-ruled it the PR's own regression.
- Finally the implementer measured BOTH paths, on the branch and on main. A clone's .git/refs at 000 never crashes anywhere. The WORKSPACE's own refs/ at 000, which is what round 4 had actually locked, gives reinit rc 1 with a traceback on main too (3.14 and 3.11). So it was pre-existing after all.
Rule for review briefs and adjudication: a pre-existing or regression verdict names the exact path and state, the main SHA, the command, and its result. A comparison between branch commits, or a repro of a different path, is unverified and cannot justify parking or un-parking a finding. When two measurements disagree, suspect that they measured different things before deciding either one is wrong.
