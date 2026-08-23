# In this plane a release cut cannot land itself: 'gh pr merge' is refused

_2026-08-23 17:31 · persistent_

In this plane a release cut cannot land itself: 'gh pr merge' is refused by the Claude Code auto-mode classifier (a harness permission denial, NOT charter's own release floor — the floor only fires when _unattended). So an agent-run release goes as far as 'PR open, all checks green, MERGEABLE/CLEAN' and stops there; the operator merges, and only then is the tag pushed from a synced main. Do not merge locally and push to main to route around it — landing code is exactly what the denial is aimed at. Verified safe-stop state to report: main unmoved, no v<X.Y.Z> tag local or remote, and pypi.org/pypi/charter-cp/<X.Y.Z>/json returning 404 with the previous version returning 200 as a control that the endpoint works.
