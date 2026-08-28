---
version: unreleased
headline: A green deletion sweep now means no survivors, and a sweep too big to finish says so
---

The deletion sweep has run on every pull request since August. Two things about it were
wrong, and the quieter one was worse.

## A green check was not "no survivors"

The gate is reporting-only — `--enforce` is deliberately absent, and that is still the
design. But a job that blocks nothing still has to *say* something, and this one said
`success` whatever it found. One branch went green with **eight survivors** under it.

To anyone who did not open the run summary, that read as "this branch is clean." It is
the opposite of what the job exists to say.

There are three answers, not two, and the check now tells them apart:

```
deletion sweep / no survivors
deletion sweep / 8 survivors
deletion sweep / no verdict: 2 of 3 shards did not report
```

All three are **green**. Distinguishing the states is not the same as failing on them, and
nothing here blocks a merge; `--enforce` is still the one flag that changes that.

The answer is carried by the check's *name* rather than its conclusion because GitHub's
conclusions cannot carry it. A job driven by a shell step concludes `success` or `failure`
and nothing else. The one conclusion that means *I looked, here is the count, this is not
a pass or a fail* — `neutral` — exists only on the Checks API, which needs a writable
token in the job that runs mutated copies of this repository's own code. That is the last
place in the repository that should hold one, so the sweep spends a job name instead.

**And every survivor is now an annotation on the line it is about.** A warning in the
margin of the guard itself, on the Files-changed tab, where a reviewer is already looking
— with the question it failed and what the covering tests actually assert. Platform-
deferred survivors arrive as notices rather than warnings, because the gate never fails on
one of those and should not shout about one either.

## A sweep too big for one machine now runs on several

Two of the last three substantial branches could not finish the gate at all. #608's guard
tables produced 62 mutations and were cancelled at the hour twice; #626's `gitconfig.py`
produced 78 and was cancelled three times, each run reaching about two thirds of its plan.
The job had been sized against the 30 to 52 mutations Phase 2's branches produced, and the
`retune-string` operator roughly doubles the count on any diff that touches string
constants — which a guard table and a config reader are made of almost nothing else.

A run that is cancelled produces **no numbers at all**, so the gate was reporting nothing
on exactly the branches least likely to be fully pinned.

**Nothing is dropped to make a branch fit.** A cap on the mutation count would read, to
every reader downstream, as "covered everything" — which is the silent truncation the
harness exists to refuse. What is capped is the fan-out. A first job counts the mutations
in one `ast` pass over the diff, sizes the sweep from the real number, and the sweep runs
on as many machines as that number needs. Past even the fan-out ceiling the plan says so
out loud instead of quietly measuring less.

**Dealt one mutation at a time, and not split by file.** Sharding by file was the obvious
answer and it does nothing here: both diffs that ran out of time were dominated by a
*single new file*, so a job-per-file hands one job all 78 of `gitconfig.py`'s mutations and
the others nothing. Round-robin also spreads the expensive ones, which arrive in clumps — a
survivor costs a full suite run where a red costs seconds, and survivors cluster in the
function that has no test.

The selection map — one trace of the whole suite, **250 s on a GitHub runner** — is now
warmed once and restored by every shard in seconds, so a second machine costs its own
baseline and not its own trace. That the map was cached was the premise for sharding, and
it was not true before: the workflow had no cache step at all, so every run rebuilt it.

The budget is written for the case where the cache misses anyway, because a fork's pull
request cannot write one and a budget that only holds on a hit fails on exactly the runs
nobody is watching. Measured end to end on the runner: a shard pays about 8½ minutes
before its first mutation with no cache and about 4½ with one, and roughly a minute a
mutation after that.

## And a shard that vanishes cannot look like a clean one

Everything above turns on one distinction the tool did not previously make: **a sweep that
ran and found nothing, and a sweep that did not run, are different answers.**

So a branch with nothing under the swept paths now writes an empty result set rather than
no result set — because a file that is absent is indistinguishable from a job that was
cancelled. A results file that will not parse counts as a shard that did not report; there
is no third reading of a truncated upload. And when the job that sizes the sweep fails, its
empty output is read as *the sweep never sized itself* rather than as *no shards were
needed*, which would have turned the loudest failure the workflow has into the quietest
kind of pass.

## The sweep swept the gate that reports it

119 mutations, against a change whose whole subject is the sweep. It found two lines of
its own that no test went red without.

**The unsharded path had stopped being tested.** Forcing `if shard is not None` to
always-true left every test green — because once the shard tests existed, nothing in the
suite ran a sweep *without* a shard any more. That is the ordinary way anyone uses
`tools/sweep.py` from a terminal, and it had quietly become the path no test walked.

**And the sizing arithmetic could divide by zero.** The `max(1, …)` in `per_shard` is
what stops a fixed cost that grew past the budget from making the mutations-per-shard
zero. Deleting it changed no answer with today's constants and would have made the plan
job raise with tomorrow's — no plan, no shards, no numbers, which is the failure this
whole change exists to prevent, arriving out of the arithmetic written to prevent it.

Both are pinned now. A third survivor is not a finding about this code at all: it is a
string inside a *type annotation*, which `from __future__ import annotations` means the
interpreter never evaluates, so no test can ever go red without it. That is a blind spot
in the string operator's scoping rather than a guard, and it is filed as one (#632)
instead of being worked around here.

Nothing to adopt — this is CI, not the CLI. The gate still reports and still blocks
nothing.
