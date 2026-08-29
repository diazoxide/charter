# charter's CI already runs tools/sweep.py as a gate on every PR — 'Size t

_2026-08-29 20:02 · persistent_

charter's CI already runs tools/sweep.py as a gate on every PR — 'Size the sweep' fans out to 'Sweep shard N of 3' and 'Add up what the shards found' prints the decomposed verdict (unpinned / masked-cluster / platform-deferred / unresolved / pinned), with per-line GitHub annotations you can read via: gh api repos/<owner>/<repo>/check-runs/<job-id>/annotations. Push the branch and read that instead of running the sweep locally: a local run takes ~10 min just to trace 376 test modules, is worthless on a loaded box, and gets killed by any sibling agent's pkill. The gate reports and does not block (no --enforce).
