# A fix for an ambiguity defect must be re-measured over the whole tree, n

_2026-08-31 01:32 · persistent_

A fix for an ambiguity defect must be re-measured over the whole tree, not just its test fixture: #721's fix made Mutation.tag append the mutant's replacement, and truncating it at 40 chars re-merged two drop-conjunct siblings in tools/sweep.py:706 — the fixture was too short to show it. Bound such a discriminator with a digest of the full value, never a bare cut.
