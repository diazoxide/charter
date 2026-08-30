---
version: unreleased
headline: A number written down twice is now held to one copy of it
---

Two defects, one shape: a fact recorded in two places, in prose, in neither of which
anything could check it. Both were found by reading rather than by a failure, which is the
tell — nothing in the repository was able to fail.

**`pad` had two ranges (#669).** `docs/frame.md` said `` `0` to `5` `` where it explains
the key and `` `0`–`8` `` four hundred lines down, in the list of values that take an
arrangement out of play. The second is the harmful one, because it is the sentence about
*refusal*: `pad = 6` on the documentation's word does not clamp and does not warn — an
arrangement charter cannot draw is refused **whole**, so the operator loses their entire
`[[frame.component]]` block and the frame falls back to `slots`.

The text was corrected. The defect was the test. It asked

```python
self.assertIn(f"`0` to `{instance.FRAME_PANE_PAD_MAX}`", md)
```

and the *right* copy satisfied it, so the wrong one was never in its reach. An assertion
that one occurrence exists cannot see a second one that disagrees. It now reads every range
`docs/frame.md` states and holds each of them to the constant, and a second check asks
`pane_pad` itself about every number the file prints beside `pad` — so a copy phrased
"capped at `8`" rather than as a range is red too.

**The selection map's trace cost had two figures (#670).** `SHARD_FIXED`'s budget note in
`tools/sweep.py` said the trace costs 250 s; `sweep.yml`'s cache step said 350 s, about the
same trace. Neither was asserted. It survived because nothing downstream turns on it — the
twelve-minute `SHARD_FIXED` covers either reading — so it was only a number a reader would
quote, with no way to know which one to quote.

Re-measured across nine cache-miss runs on `ubuntu-latest`, where the tool prints its own
`selection map: … in Ns`: 242, 252, 257, 276, 277, 279, 280, 282, 285 seconds. 250 was
honest when it was written against 7,693 tests and the suite has grown past it; 350 matches
no run at all.

The itemisation is now data rather than a bulleted comment — `SHARD_FIXED_COSTS`, quoting
the trace at the ceiling of that spread, because a budget is sized against the slow run —
and `sweep.yml`'s comment names the constant instead of restating a figure. What is
asserted is not the digits: the budget must cover the costs it is written from, the map
must still be the largest of them, and **any duration the workflow's comments quote must be
one the tool states**. A third copy is then either right or red.

Nothing an operator runs changes. Both fixes are the same correction to the same habit: an
assertion about *presence* was standing in for one about a *value*, and a check that counts
occurrences is exactly that.
