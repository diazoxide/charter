# STALE-MEMORY TRAP, caught cutting 0.49.0: my own note said 'NEXT MINOR I

_2026-08-22 20:14 · persistent_

STALE-MEMORY TRAP, caught cutting 0.49.0: my own note said 'NEXT MINOR IS BLOCKED — captured.json is at 0.47.1, so 0.49.0 computes lag 2 and FAILS'. FALSE by the time I read it — PR #362 re-stamped captured.json to 0.48.0, so 0.49.0 computed lag 1 and passed. Trusting that note would have wrongly forced a patch and let the gate silently pick the version. ALWAYS run tests/test_asset_freshness.py's own _minor/lag arithmetic against the CURRENT captured.json before choosing; never quote a remembered lag. Standing fact: at captured=0.48.0, 0.50.0 computes lag 2 and WILL fail until demo.svg/personas.svg/statusline.svg are regenerated.
