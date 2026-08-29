---
version: unreleased
headline: A sweep a newer push cancelled says it was superseded, not that it found nothing
---

`sweep.yml` sets `concurrency: cancel-in-progress`, so a pull request that gets three pushes
pays for one sweep rather than three. The runs it cancels still reach the job that publishes
the answer — `collect` runs under `always()`, deliberately, because "a shard did not finish"
is the case that job exists for and a chain that only ran on success could never say it.

A cancelled run therefore read its own empty `needs.plan.outputs.shards` and published
**`no verdict: the sweep never sized itself`**. True of that run, and indistinguishable on
the checks list from the sentence this gate was built to say. Both rows stay, because the
verdict is carried by the check's *name* and two different names do not replace one another —
so a branch could show `no verdict` beside a perfectly good `81 survivors`, with nothing to
say which was which.

Measured on #652 at `74ccfdab40c2`: runs 86 and 87 created in the same second, 87 cancelled
86 by concurrency group, and 86's `collect` completed and published that verdict beside 87's
real one.

`collect` now distinguishes the two on `cancelled()`, which is a fact about the **run** and
not about a job. A shard that exceeds its own `timeout-minutes` is a cancelled job inside a
run that was never cancelled, so #626's sentence — `no verdict: 8 of 8 shards did not
report` — is untouched and still means what it meant. Only a superseding push reaches the
new step, and what it publishes is `superseded: a newer push cancelled this sweep`.

The step reaches for nothing: no checkout, no interpreter, no artifacts. The single state it
exists for is the one where every step above it was cancelled, so a step that needed the tree
could not run in it.

Why this is worth a fix rather than a shrug: `no verdict` is the one outcome this gate exists
to make loud. #644 carried a real one — eight of eight shards cancelled against a 418-mutation
diff — and it was true, actionable, and led to that PR being split. A gate that cries the same
sentence on every second push is how a true one stops being read.
