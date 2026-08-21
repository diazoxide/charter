# The asset-freshness gate is now the binding constraint on charter's next

_2026-08-21 10:50 · persistent_

The asset-freshness gate is now the binding constraint on charter's next MINOR. docs/assets/captured.json is stamped 0.46.0 and tests/test_asset_freshness.py fails only when lag EXCEEDS MAX_MINOR_LAG=1 — so 0.47.0 passed at lag 1 and 0.47.1 (a patch) still does, but 0.48.0 computes lag=2 and WILL FAIL until demo.svg, personas.svg and statusline.svg are regenerated and the stamp updated. Regenerating is a capture run against a specific plane state (the SVGs carry hand-positioned per-glyph x coordinates, so they cannot be text-edited), and they are additionally stale in CONTENT since 0.47.0 changed the markers, the vault dots and the cache gauge. Do this BEFORE the next minor, not during it — a freshness gate that quietly bends a version number from minor to patch is worse than one that stops you.
