---
version: unreleased
headline: A stuck sub-agent now says so instead of disappearing
---

The in-flight tracker deleted any record older than thirty minutes, on every read. So the
single most interesting thing it could ever hold — a dispatch that has outlived every
reasonable expectation — was the one case it rendered as *nothing at all*. "Presumed dead"
and "never happened" were the same picture, and the deletion was irreversible: the first
render past the threshold was the last one that could mention it.

That threshold was doing two jobs at once, and they want opposite horizons. It is now two:

* **presumed dead at thirty minutes** — the record is kept and flagged, not deleted;
* **pruned after a day** — so a stray from a killed process still cannot accumulate into a
  permanent false warning, but nothing vanishes while a human might still be looking at it.

On screen the chip appends a `?` to the age:

```
▫ statusline ✎9 ⚡ 3m
▫ forge ◦ ✎3 ⚡2 47m?
```

The age keeps climbing — which is also what finally makes the badge's `2h` and `3d`
reachable, since nothing used to survive long enough to render them. The mark says
*presumed dead, not confirmed*, because that is the whole of what charter knows: it cannot
tell a killed process from a sub-agent still grinding away. Hence a question mark rather
than a cross, and hence no colour escalation — it stays in the age's dim, since a red mark
would claim exactly the certainty the mark exists to disclaim.

The session strip's `⚡ N` counts every record, presumed-dead ones included. The chips are
where the distinction lives; the aggregate is what survives a narrow pane, and a total that
shrank when a dispatch got *more* worrying would be the original bug in miniature.

Two rules follow the record now that it outlives the threshold:

* `finish` retires the oldest **still-running** record for a persona, not simply the oldest.
  Oldest-first alone would hand a finishing dispatch the stuck record to retire and leave
  its own behind — deleting the thing this change exists to keep. A presumed-dead record is
  still eligible when there is nothing else, because a genuinely long dispatch does finish
  eventually.
* The overlap nudge at dispatch time asks only about records charter can still call
  *running*. It claims a peer "is already running", which stops being true at the
  presumed-dead threshold, and a warning that nags for a day after a killed process is one
  people learn to dismiss.

Closes #308.
