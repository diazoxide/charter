# ASSET-FRESHNESS GATE IS PATCH-EXEMPT BY CONSTRUCTION. tests/test_asset_f

_2026-08-23 18:49 · persistent_

ASSET-FRESHNESS GATE IS PATCH-EXEMPT BY CONSTRUCTION. tests/test_asset_freshness.py computes lag via _minor(), which parses only (major, minor) and DISCARDS the patch; lag = dmajor*1000 + dminor, fails when > 1. So captured.json at 0.50.0 against a 0.50.1 charter computes lag 0. A PATCH release NEVER needs SVG regeneration no matter how stale the captures look — only a MINOR can trip it. Confirmed cutting 0.50.1: captures stamped 0.50.0, gate green, no PATH-shim regeneration needed. Still run the gate rather than quoting this: the rule is stable, the numbers are not.
