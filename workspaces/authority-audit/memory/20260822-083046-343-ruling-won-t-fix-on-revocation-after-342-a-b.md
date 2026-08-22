# #343 ruling (won't-fix on revocation): after #342 a broken persona refer

_2026-08-22 08:30 · persistent_

#343 ruling (won't-fix on revocation): after #342 a broken persona reference contributes NO tools (load() returns None for a non-name), so gating toolgate on structural_errors buys zero containment and only revokes the persona's OWN tools:. Verified in 3 shapes — uses:-as-path, uses:-typo, extends:-cycle. It would also be SILENT: hooks.pretooluse emits nothing when decide() declines, so a revoked grant looks identical to a tool never declared. Closed instead by a doctor 'persona grant' row naming the persona + the binaries it still auto-approves.
