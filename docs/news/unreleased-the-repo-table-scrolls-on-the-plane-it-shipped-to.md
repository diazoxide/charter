---
version: unreleased
headline: The repo table's viewport moves over rows, so a plane with one clone and fourteen worktrees can finally scroll it
---

Charter shipped a scrollable repo table, and on the control plane it was written on the
wheel did nothing at all. Not sometimes — never, at every pane height there is.

```
 ▪ repos 1
   ├─ charter
   │  ├─ frame-inside-tmux
   │  ├─ guard-all-or-nothing
   │  ├─ opencode-mcp-rule
   │  ├─ op-asks-once
   │  ╰─ reap-live-pid          <- and nine more worktrees, off the pane, unmentioned
```

Three wheel notches into that pane moved nothing, and the nine rows past its height were
dropped with no line saying they existed.

**The bound was computed in the wrong unit.** The pane renders ROWS: its budget is in rows,
the `…(+N more)` line takes a row, and a click arrives as a row number. The scroll limit
was computed from the REPO count — the one quantity in that expression that is not the
pane's. The two agree only where no repo has pieces, so on a many-clones plane the feature
worked and on a one-clone plane it answered *nothing to scroll* however many worktrees hung
off the clone. A control plane is the second shape, which is why this shipped switched off
for its own operator.

It was written down, too. The limit's own docstring declared the repos-not-pieces window as
a stated limit rather than an oversight, and the argument it gave was sound on its face:
*piece rows appear only in the one-repo shape, where one repo always fits*. What nobody
checked is that the plane in front of us was that shape. **A stated limit that happens to
cover the only configuration you have is not a limit; it is a no-op with documentation.**

**The window now moves over the table's own rows** — repo rows in the ranking's order,
then the piece rows nested under them, which is the order the table already spent its
budget in. So the first notch walks the clone's row off the top and the worktrees slide up
behind it, and the last notch puts the last worktree on screen.

**Two things a starved one-clone table now admits that it used to hide.** The `…(+N more)`
line is reserved on the same condition every other shape reserves it on, so a table showing
five of fifteen rows says so and says whether the ten it is hiding are clean — counting the
hidden worktrees, not just the hidden clones. And the `⑂14` badge on the clone's row is
counted from the pieces actually drawn rather than from the ones in the cache, so it stops
disappearing exactly when it is most needed.

**What did not change.** A many-clones table is byte-for-byte what it was, at every offset
and every pane height: the ranking still decides which repos survive a starved pane, and
scrolling still moves a window over that ranking rather than replacing it. A one-clone
plane at its natural height — the pane is sized to its content — draws exactly what it drew
before and still scrolls nothing, and that nothing is still asserted as a number at the
boundary and one either side. What changed there is the reason: the pieces fit, rather than
never having been counted.

The half of the report that did not reproduce: a click on a worktree row already selected
its parent clone, on the shipped code, on any cache written since worktree rows started
carrying their repo. Every drawn row but the overflow line resolves to something, and did
before.

Verified on a real tmux with a real client on a real pty and charter's own two panels: five
cases red before the change and green after, including a click on a piece row of a table
scrolled far enough that the clone's own row is off the pane — where a dead row would have
had nothing to answer.

[#663](https://github.com/diazoxide/charter/issues/663), reported against
[#658](https://github.com/diazoxide/charter/pull/658), which shipped the scroll and stated
the limit.
