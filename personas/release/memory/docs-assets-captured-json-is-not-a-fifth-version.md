# docs/assets/captured.json is NOT a fifth version point — it records when

_2026-08-21 13:20 · persistent_

docs/assets/captured.json is NOT a fifth version point — it records when captures were taken and must stay at the version they were shot at. Bumping it would forge the freshness evidence. It also gives the patch-vs-minor call a real cost: MAX_MINOR_LAG=1, so against captures stamped X.Y.Z a minor lands at exactly the limit and the NEXT minor goes red, where a patch keeps the lag at 0.
