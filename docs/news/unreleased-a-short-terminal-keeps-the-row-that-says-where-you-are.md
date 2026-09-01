---
version: unreleased
headline: A short terminal keeps the row that says where you are, and your hidden todos are one keypress away
---

Two reports about the bottom of the frame, from the same audit.

## The ten-row terminal was spending three rows to say the number 8

Shrink a framed terminal to 120x10 and this is the whole screen:

```
h$
                        <- five rows of harness
────────────────────────────────────────
▪ repos 8
────────────────────────────────────────
7 todos · F2 palette
```

Six rows of agent session, three rows to say a repo count, one row of attention strip. The
workspace name and the persona you are being — the two things the identity row exists to
tell you — are gone, dropped a few rows earlier to make room. The repo table's own pane had
by then shrunk to a single row, which is its heading and no repo at all: `▪ repos 8` between
two pane rules.

**The ladder now drops the table before the identity row, on rows as well as on columns.**
The table already refused to be drawn in a pane too narrow for it (below 95 columns the slot
is not split at all, rather than split for a bordered rectangle with nothing in it). It has
the same rule on its other axis now: a pane too short to draw one repo row under its heading
is not split either. And the identity row leaves the group that goes at `min-rows` — it is
one row carrying two facts nothing else on a short terminal says, and on a terminal with no
sidebar it is the plane's only persona roster, so it goes last, with everything else, below
half the floors.

**This costs your harness nothing.** Measured on real tmux — 3.7c and the 3.2 floor, a real
client on a real pty, byte-identical on both — at 120 columns on a plane with eight clones:

| rows | before | now |
|---|---|---|
| 20 | identity 1 · harness 12 · sidebar 12 · table 3 · attention 1 | *unchanged* |
| 19 | harness 12 · table 4 · attention 1 | identity 1 · harness 12 · table 2 · attention 1 |
| 18 | harness 12 · table 3 · attention 1 | identity 1 · **harness 14** · attention 1 |
| 17 | harness 12 · table 2 · attention 1 | identity 1 · **harness 13** · attention 1 |
| 16 | harness 12 · table 1 · attention 1 | identity 1 · harness 12 · attention 1 |
| 15 | harness 11 · table 1 · attention 1 | identity 1 · harness 11 · attention 1 |
| 10 | harness 6 · table 1 · attention 1 | identity 1 · harness 6 · attention 1 |

A table pane the rows could not afford was floored at one row and still paid for its pane
rule, so from 16 rows down the exchange is exactly even: two rows of terminal that said `▪
repos 8` now say `⬢ alpha · ◆ steward`. At 17 and 18 the harness gains the rows the table
was spending on one or two repo rows. Nothing about a frame at 20 rows or above moves.

**What you lose, said plainly.** Between 17 and 19 rows the table draws fewer repos than it
did — two fewer at 19, all of them at 17 and 18. That is the trade the ladder was always
supposed to make and did not: the frame ranks the table below the identity row, and until now
the table's pane was the one thing on the rows axis with no test of its own to fail.

**The rule, so the next rung has one to follow.** A rung drops when the pane it would get
cannot carry the thing the rung is for; among rungs that still can, the ones whose facts
another surface reaches go first. `bottom` never drops — it is the one alert and the command
that fixes it. `top` is last above it. The tab bars go at `min-rows` because `F2` reaches
every chat and every workspace at every width. The table goes above them because it needs
two rows before it can name a single repo.

## `…(+5 more)` todos you could not get at without leaving the frame

The sidebar draws your open todos under your personas, and a short pane draws two of them
and a count:

```
│▪ todos 7
│- Audit auth token ro…
│- Cut 0.55.0
│  …(+5 more)
```

Those five were reachable only by typing `charter workspace todo` into the harness — which
is the surface the frame exists to replace. The persona column above them got its route in
the last release (click a name to adopt it, click its badges to be told what they mean). The
todo list had nothing: no click, no key, no palette row.

**`F2` → `todo: read the next open todo`.** It reads the list out on the palette's own
header, one press per todo:

```
charter /todo · todo 3/7: Cut 0.55.0
>   todo: read the next open todo
```

It repeats, like the two `repo: select the …` rows, so five hidden todos are five Enters and
one palette rather than five. It says where you are in the list, because a wrap that did not
would read as a repeat; it wraps at the end rather than pressing nothing; and on a plane
whose cache holds part of a very long list it says that too — `todo 3/20 of 400: …`, counted
against the same total your `▪ todos N` heading carries, so the header and the pane cannot
disagree.

**A key rather than a wheel, deliberately.** `[frame] mouse` ships off, so a pointer-only
answer would be inert on exactly the frames this was reported from — and charter's own rule
runs that way anyway: a pointer affordance always has a key or a palette row beside it, so
the keyboard route is the half that has to exist. A wheel over the section can be added on
top of this. It could not have stood in for it.

**The `…(+5 more)` row still does nothing when you click it**, and that is not an omission.
It stands for todos it does not name, so there is no todo for a click on it to be about —
the same case the persona column's own overflow row has carried since it became clickable.
What changed is that the content behind the count has a route, not that the count became a
door.

It costs nothing when you are not pressing it: the row reads the plane snapshot the palette
had already loaded to draw itself, writes no file, bumps no version and starts no process.
