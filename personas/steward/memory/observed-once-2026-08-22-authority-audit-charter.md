# Observed once (2026-08-22, authority-audit/charter): a workspace clone's

_2026-08-22 11:25 · persistent_

Observed once (2026-08-22, authority-audit/charter): a workspace clone's HEAD moved from a feature branch to main mid-session with no command of mine causing it — reflog showed a plain 'checkout: moving from <branch> to main' between a commit and the next suite run. NOT reproducible: 'charter workspace _reconcile' was cleared over 3 trials, as were charter news, persona remember and the news tests. Only happens on a clean tree (a dirty clone is correctly left alone). Cause unidentified; charter runs detached background processes from SessionStart
