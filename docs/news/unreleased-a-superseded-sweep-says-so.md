---
version: unreleased
headline: A sweep that never sized itself says that, instead of borrowing the shard sentence
---

`sweep.yml` sets `concurrency: cancel-in-progress`, so a pull request that gets three pushes
pays for one sweep rather than three. The runs it cancels still reach the job that publishes
the answer — `collect` runs under `always()`, deliberately, because "a shard did not finish"
is the case that job exists for and a chain that only ran on success could never say it.

A cancelled run therefore reached the shard arithmetic and answered about shards nobody had
planned. Measured on the branch that fixes it: run 99, cancelled by run 100, published
**`no verdict: 1 of 1 shard did not report`** — #626's own sentence, from a run where no shard
was ever asked for. Both rows stay on the checks list, because the verdict is carried by the
check's *name* and two different names do not replace one another.

`collect` now separates the two on **`needs.plan.result`**, which is `cancelled` exactly when
the sizing job did not finish — the only state in which the shard arithmetic is answering
about shards that were never planned. A shard that exceeds its own `timeout-minutes` leaves
`plan` succeeded, so #626's sentence is untouched and still means what it meant.

**The obvious discriminator was `cancelled()`, and it is wrong.** That was the first version,
and it did not fire: run 99 had `conclusion=cancelled`, a cancelled `Size the sweep` and a
cancelled shard, and `collect` still took the other branch. In a job running under `always()`,
`cancelled()` does not answer for the run the way its one-line summary suggests. The
correction came from the measurement, not from re-reading the docs, and a test now pins it —
regressing the condition to `cancelled()` goes red.

What it publishes is `cancelled: this sweep did not size itself`, and that wording is
deliberately narrower than "superseded". `plan` carries `timeout-minutes: 30`, so a cancelled
sizing job has two possible causes and the check names only what it can observe. Either way
the cancelled `Size the sweep` job sits beside it, so nothing is hidden.

The step reaches for nothing: no checkout, no interpreter, no artifacts. The single state it
exists for is the one where every step above it was cancelled, so a step that needed the tree
could not run in it.

Why this is worth a fix rather than a shrug: `no verdict` is the one outcome this gate exists
to make loud. #644 carried a real one — eight of eight shards cancelled against a 418-mutation
diff — and it was true, actionable, and led to that PR being split. A gate that cries the same
sentence on every second push is how a true one stops being read.
