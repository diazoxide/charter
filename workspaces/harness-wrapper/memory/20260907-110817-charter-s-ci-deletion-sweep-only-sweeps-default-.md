# charter's CI deletion sweep only sweeps DEFAULT_PATHS = ('charter',), so

_2026-09-07 11:08 · persistent_

charter's CI deletion sweep only sweeps DEFAULT_PATHS = ('charter',), so a PR that only touches tools/sweep.py gets 'no survivors' with 0 mutations — the harness does not sweep itself in CI. Sweep it by hand: python3 tools/sweep.py --gate --path tools --base origin/main.
