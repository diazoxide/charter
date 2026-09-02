---
version: unreleased
headline: A filesystem that refuses now costs what charter says it costs
---

The quit/close/reopen path is full of `except OSError:` clauses whose docstrings state
exactly what a failure costs — `False` from `reopen.write`, `[]` from `plane_chats`, `0`
from the in-flight tracker, a chat whose directory was not recorded. **Nothing in the suite
had ever entered one of them.** Fourteen clauses, from #810's group A.

## Why the sweep could not tell

The deletion sweep's `narrow-except` mutation replaces the whole exception spec with
`ZeroDivisionError`. A clause that is never entered survives that, because the mutated
program and the shipped one run identically on every input the tests supply.

## The two questions #810 asked, answered

**"Does charter test filesystem failure at all?"** It does, in two places, and that settles
the seam: `unittest.mock.patch` on the exact call that would raise — never a `chmod`. A
mode bit is advice to root, some CI jobs run as one, and a test whose premise the runner can
ignore reports green for the wrong reason.

**"Are some of these unreachable rather than untested?"** The sweep attaches a `PLATFORM`
note to every `OSError` survivor, because `OSError` is sometimes the operating system's
opinion — its own example is a pty read whose clause is dead on macOS and live on Linux.
**None of these fourteen is that.** Every one is charter writing, reading, scanning or
unlinking a file under its own state directory, where `ENOSPC`, `EACCES`, `EROFS`, `EIO`
and a directory removed underneath a scan are reachable everywhere charter runs. So this is
fourteen tests rather than a measurement saying it should be twelve.

## What is asserted

The consequence, never "it did not raise":

- a manifest that cannot be written leaves the **previous** manifest whole and readable —
  half a manifest is a plane half-restored;
- a value `json` cannot serialise costs one manifest rather than the quit, which is what
  the three names in that clause are for;
- a manifest that cannot be deleted costs a duplicate tab and does not fail a reopen that
  has already relaunched every harness;
- a transcript that cannot be removed does not strand the ones after it in the sweep;
- a frame root that cannot be scanned means **no** chats, so a quit that cannot see the
  plane records nothing and stops nothing rather than half of each;
- a chat whose `cwd` or `closed` marker will not write reads back as not recorded, which is
  the cost those two docstrings already state;
- a persona pointer that will not write still lets the transcript move — the persona is
  attempted first, so a clause that let its failure through would take the transcript with
  it;
- a reopen that cannot enter a recorded directory says so and does **not** relaunch the
  chat somewhere else; and one that cannot get back afterwards still reports the chat that
  did come back.

**The two that mask each other are asked separately**, which #810 flagged and is worth
repeating: `inflight.prune_all` holds a `glob` guard and an `unlink` guard in sequence, so
with the listing swallowed the loop never runs and a mutant on the second guard looks
equivalent. One case breaks the listing; one breaks exactly one removal with the listing
intact. `prune_transcripts` has the same shape and gets the same treatment.

## Verification

Seventeen cases, no tmux, no production code changes. Every one of the fourteen clauses was
verified by hand: replace its exception spec with `ZeroDivisionError` — the sweep's own
mutation — and the module goes red. Fourteen applied, fourteen killed.

Nothing to adopt.
