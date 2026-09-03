# Comparing two git trees on a timing race: alternate the runs, never batc

_2026-08-31 11:16 · persistent_

Comparing two git trees on a timing race: alternate the runs, never batch them. #748's roster-duplication rate moved 3x with machine load (18% idle, 64% loaded), so two batches taken back to back are not a comparison — run-for-run alternation is.
