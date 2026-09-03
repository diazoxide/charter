# A release has a FIFTH version site the charter never listed: docs/assets

_2026-09-02 23:46 · persistent_

A release has a FIFTH version site the charter never listed: docs/assets/captured.json. tests/test_asset_freshness fails when a capture lags more than one minor (lag = dmajor*1000 + dminor > 1), so captures stamped N-1 are legal and N-2 is not — meaning roughly every SECOND minor release must regenerate docs/assets/{demo,personas,statusline}.svg before CI can go green. It blocked 0.50.0 and 0.55.0 for exactly this. Regenerate, never re-stamp: the gate exists so a screenshot cannot drift from what charter prints. Recipe: build a throwaway plane with docs/assets/demo-plane.sh in a directory with NO charter.toml above it, put a shim on PATH that runs the worktree's charter (else the capture documents the installed CLI), then use the README's load-bearing flags verbatim. social-card.svg/.png are composed from statusline.svg and must be regenerated after it.
