---
version: unreleased
headline: A deletion sweep too big to finish now reports what it measured instead of reporting nothing at all
---

The fourth time this repository has closed a way for **silence to read as success**, and the
first one where the gate actually ran.

## Measured, on run `33500900581`

```
Size the sweep        11:09:49 → 11:15:10   success
Sweep shard 1..8      11:15:12 → 12:15:2x   cancelled   ← 60 min, to the second, all eight
Add up what the shards found                success
no verdict: 8 of 8 shards did not report    success      ← reports SUCCESS
```

231 mutations, eight shards, every one killed at `timeout-minutes: 60`. Between them those
eight had measured something close to 224 mutations by the time they died, and **every one
of those answers was thrown away** — the result set lived in memory until the last line of
the sweep, so a killed process took the whole thing with it.

The plan job had predicted it in the same run and proceeded:

> `231 mutations → 8 shard(s) of at most 29 each`
> ⚠ `231 mutations is past the 224 that 8 shards can measure inside 40 minutes each … a
> shard may be cancelled at the job timeout, and a cancelled shard reports no verdict
> rather than a short one.`

That warning was accurate. It was also the design confessing: `MAX_SHARDS = 8`,
`SHARD_BUDGET = 40 min`, so `8 × per_shard()` is 224, and **any branch past that got this
result regardless of what was in it**. It is not a signal about the branch.

## What was fixed, and what was deliberately not

**A shard now stops short of being killed and reports what it measured.** Ten minutes before
the runner's own cap it deals no further mutation; the ones it never reached come back
`out of time`, named, in the same result set as the answers. The check on the pull request
reads

```
deletion sweep / no verdict: 23 of 29 measured, 6 out of time
```

instead of `no verdict: 8 of 8 shards did not report` — which is #630's own argument turned
on itself: *a partial answer honestly labelled beats silence.*

**And the file is written as the answers arrive, not at the end.** The measurement this
turns on is that GitHub was never the obstacle: on run `33500900581` the `if: always()`
upload step ran on every one of the eight cancelled shards and concluded `success`. It
uploaded nothing because there was no file to upload. So the shard now writes one before
its first mutation — the whole plan, every row marked `out of time` — and rewrites it as
each answer lands. Whatever a killed shard has measured is on disk at the instant it dies,
and the file it leaves is the **complete plan with holes in it** rather than a short list
that would read as a complete sweep of a short plan.

`out of time` is its own bucket and not folded into `unresolved`, which is the same rule
#698 established for `withheld`: the three say different things about what the tool knows,
and a reader's next move differs for each. `unresolved` is *I looked and could not tell* —
re-run it. `out of time` is *I never looked* — re-running the same plan on the same fan-out
stops at the same place, so what answers it is a smaller branch or more machines.

**`MAX_SHARDS` was not raised.** Its own docstring calls it a ceiling on machines and never
on questions, so it is the number written to be raised — but raising it moves the ceiling
without removing it, and the next large diff hits the same wall having burned more runners
on the way. The ceiling is now survivable, which is the property that was missing.

**And `timeout-minutes` was not lowered to match `SHARD_BUDGET`,** though the issue is right
that the two disagreeing by twenty minutes was a defect on its own: a shard that overran its
sizing got twenty further minutes of runway and *then* died with nothing, which is the worst
of both. The two were never one deadline. Forty minutes sizes the **plan** — how many
mutations a machine may be dealt, computed from an average mutation — and sixty is the
runner's cap. A shard whose slice happens to hold five survivors pays a full-suite run for
each and legitimately needs longer than forty; stopping it there would turn a complete answer
arriving at forty-eight minutes into a partial one, which is a regression on exactly the
branches that have something to report, dressed as a fix.

So what closed the gap is a **third** number between them, `SHARD_REPORT_AT`, ten minutes
short of the cap. A shard uses every minute the runner will give it and stops just before
being killed. `SHARD_REPORT_AT > SHARD_BUDGET` is asserted, because that inequality is the
one that says a correctly sized shard is never cut short by this; the runner's cap is now
written in `sweep.py` and the YAML is held to it, so one deadline no longer lives in two
files drifting apart (#670).

## The lineage, and what is still open

Each of these removed one way for a silent gate to read as a passing one:

* **#617** — a green check was not "no survivors". #630 put the verdict in the check **name**,
  because GitHub's conclusion vocabulary cannot carry it.
* **#646/#561** — a gate that never ran looked identical to one that found nothing. Fixed by
  making `Add up what the shards found` a **required** check, so a missing row blocks.
* **#782** — a sweep that planned nothing still said `no survivors`. Fixed by
  `nothing to sweep`.
* **This** — a sweep that ran, could not finish, and said `success`.

**The name is now honest in every case, and nothing yet stops a merge on it.** `--gate` is
still non-enforcing by deliberate design, and `no verdict` still sits on the checks list
beside real passes wearing a green tick. That is a repository-settings decision and an
operator's call, not a commit — it is recorded on #803 with a recommendation rather than
taken here.

Nothing to adopt: this is charter's own pull-request gate and does not reach an installed
charter.
