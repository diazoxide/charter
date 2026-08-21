# 0.47.2 (2026-08-21) cut as a PATCH carrying #318 (cross-process probe gu

_2026-08-21 13:21 · persistent_

0.47.2 (2026-08-21) cut as a PATCH carrying #318 (cross-process probe guard) and #319 (check: held to _PROBEABLE). #319 narrows what a check: may name — a contract change — but entries ship inside the wheel so the only authors are maintainers, and all three shipped check: lines name 'persona lint', which _PROBEABLE matches on subcommand path only, so 'persona lint --only delegate-when' still resolves. Narrowing an authoring surface whose only authors are the people cutting the release is a patch, not a minor.
