# charter's deletion sweep: a shard that exits green WITHOUT writing its -

_2026-09-07 18:23 · persistent_

charter's deletion sweep: a shard that exits green WITHOUT writing its --json (the red-baseline early return) is read by merge() as a shard that did not report — silence is how a dead shard reports, so a deliberate refusal has to be written down (as_refusal/Refusal, #920). Check any new early return in tools/sweep.py --gate for this.
