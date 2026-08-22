# The asset-freshness gate (tests/test_asset_freshness.py) can VETO a vers

_2026-08-22 12:28 · persistent_

The asset-freshness gate (tests/test_asset_freshness.py) can VETO a version number, so check it before choosing one. lag = (Dmajor)*1000 + (Dminor) vs docs/assets/captured.json, fails when lag > 1. At captured=0.47.1: 0.48.0 gives lag 1 (passes, LAST slack), 0.49.0 gives lag 2 (FAILS), 1.0.0 gives lag 953 (FAILS). So a 1.0.0 can never ship without regenerating captures first — never let this constraint silently pick the version; decide on merits, then check the gate and say so if they disagree.
