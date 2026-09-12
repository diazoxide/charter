# tools/sweep.py only mutates guards under charter/ — a diff confined to t

_2026-09-10 16:53 · persistent_

tools/sweep.py only mutates guards under charter/ — a diff confined to tests/ gets 'NOTHING TO SWEEP' (0 mutations), which proves nothing. For a guard added in tests/ (e.g. tests/_planeguard.py), run the deletion check by hand: delete one guard at a time in a scratch copy and run the covering class. On #944 that found two survivors the sweep could not see.
