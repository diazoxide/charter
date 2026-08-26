# Deletion-sweep methodology gap, measured on release.yml's version check

_2026-08-26 20:37 · persistent_

Deletion-sweep methodology gap, measured on release.yml's version check (#558/PR 560): a sweep that asserts only the EXIT CODE can stay green over a real guard deletion, because two guards in sequence mask each other. Deleting the '-z $claimed' refusal still exits 1 — the mismatch check below it catches the empty string and says 'this run names  (the version input <none>) but pyproject.toml says 0.53.0' instead. Same rc, different reason, worse answer. So a refusal test must assert WHICH refusal fired, by its message. This is the sweep's own failure mode one level up: matching a symptom instead of a property — the shape behind every bypass charter has shipped. Written into the Phase 2 plan's sweep section as 'Assert the reason, not just the refusal'.
