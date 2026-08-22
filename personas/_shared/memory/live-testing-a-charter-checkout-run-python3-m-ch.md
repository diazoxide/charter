# Live-testing a charter checkout: run 'python3 -m charter ...' (CONTRIBUT

_2026-08-22 17:59 · persistent_

Live-testing a charter checkout: run 'python3 -m charter ...' (CONTRIBUTING.md) — 'python3 -m charter.cli' silently produces no output. cwd wins over $CHARTER_ROOT for plane resolution, so run from inside the temp plane with PYTHONPATH at the checkout; the LOCAL vaults.json follows $CHARTER_HOME, not the plane root.
