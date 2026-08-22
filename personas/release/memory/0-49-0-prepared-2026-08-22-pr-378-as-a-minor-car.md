# 0.49.0 prepared (2026-08-22, PR #378) as a MINOR carrying the authority-

_2026-08-22 20:13 · persistent_

0.49.0 prepared (2026-08-22, PR #378) as a MINOR carrying the authority-audit tail: 3 code PRs (#364, #377), 2 docs PRs (#359, #362), 8 issues. Minor not patch on TWO scripted paths, not one: 'guard ask mcp__…​ *' moves rc=0-and-write -> rc=2-and-refuse, AND 'guard list' changes its row format (adds a bucket column) plus a second file section. The old rc=0 being a LIE does not make the change invisible to a caller — same reading that made 0.48.0 a minor. Version points still 14 (1 pyproject + 1 __init__ + 1 plugin.json + 11 --plugin-version).
