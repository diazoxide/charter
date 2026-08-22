# Smoke-testing a release, the isolation pattern that works: pin a throwaw

_2026-08-22 20:14 · persistent_

Smoke-testing a release, the isolation pattern that works: pin a throwaway plane with CHARTER_ROOT=$(mktemp -d) holding only a 'charter.toml', then run the REAL CLI as a subprocess. $CHARTER_ROOT wins outright in root.find_root, so config.SHARED_VAULTS/.claude/settings.json all redirect into the fixture — assert this by PRINTING the resolved paths under the fixture, which is stronger evidence than hashing the operator's files afterwards. Anti-vacuity checks that earned their keep on 0.49.0: for a refusal, shasum every harness file before/after and assert byte-equality AND that stderr is the refusal sentence and NOT argparse ('usage:'/'invalid choice'); for a widened listing, replay the OLD version's logic against the SAME fixture for a row-count A/B (5 -> 7); for a crash fix, replay the old crashing line and CATCH the real exception to prove the precondition.
