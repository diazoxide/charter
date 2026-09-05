---
version: unreleased
headline: dragging a tab strip taller now sticks, the workspace tabs lead with the workspaces you have been in, and the strip spends five cells between tabs where it spent eight
---

Four changes to the tab strips, reported together from a running plane and made together
because all four feed the same cut.

```
before
   authority-audit (2)   autonomy   charter-update-skill   default   fleet  *harness-wrapper (3)   news-dispatch-guard  +8

after
   harness-wrapper 3   authority-audit 2   autonomy   charter-update-skill   default   fleet   news-dispatch-guard   plane-shape  +7
```

## A drag sticks

*"when switching between workspaces — resized tabs resetting to one line. it should
preserve sizes."*

Dragging a pane divider with the mouse is the gesture an operator reaches for, and charter
threw it away. Measured on the reporting plane: **no frame had a recorded strip height at
all**, so nobody was pressing `F3` — they were dragging, and the next layout pass
recomputed the height away. A workspace switch is enough to trigger one.

#880 chose the keybinding over the drag *because* the layout owns bar heights, and judged
teaching the layout to respect a drag "a much larger change than the one-row default". That
was the wrong trade: it left the obvious gesture failing **silently** and a key nobody knows
about as the only way to do the thing.

A drag now writes the same stored height `F3` writes, so both gestures have one meaning and
one place to look. It is still a **ceiling**, not a height: drag a strip taller on a plane
whose names already fit on one row and the rows come back, because there is nothing to put
on the second one.

**The hazard was the whole difficulty, and it is worth saying what it was.** charter resizes
these panes itself — on every terminal resize and every re-layout. A rule that adopted
whatever height it found would have pinned the strip at whatever the layout last computed
and never let go, which is a worse defect than the one being fixed, because you cannot drag
your way out of it. So the comparison is against **what charter last asked for**, written
down at the end of every resize alongside the window's size and the frame's slot set. Three
states adopt nothing and are refused before tmux is asked anything: a frame that has not
been sized yet, a window whose height changed (tmux rescales every pane proportionally on
its own, so every height differs and none was dragged), and a frame whose panes were added
to or removed. That last exclusion is also why a terminal drag costs no extra tmux call at
all.

Below tmux 3.3 this does not happen on its own, for the reason the strip's height already
did not: there is no `window-resized` hook to notice. `charter frame-resize` typed in the
frame's own window is the recovery, as it already was.

## The workspace tabs lead with your working set

*"active last used tabs should be in first order, then olds."*

The workspace strip is ordered by when you last worked in each workspace, newest first —
worked out from mtimes charter already moves, with no new tally and no new counter.

**This contradicts two decisions that were measured, and the way it does is the interesting
part.** The strip's cut refuses a window centred on the marked tab: *"a window CENTRED on
the marked tab moves every column each time the operator switches — on this project's own
fifteen workspaces at 160 columns, six of the nine drawn tabs answer a second press at the
identical column with a SECOND, different workspace."* The chat roster refuses recency for
the same reason: *"`api.1` stays leftmost, where an operator learned to look for it, instead
of jumping to the end because it happens to be the newest."*

Both are right about a row that **re-sorts while you look at it**. Neither argues against an
order that is fixed while you look at it. So the order is computed once per frame launch and
held for the frame's life: switching updates the record for next time and moves no column
now. A workspace made while the frame is open goes on the end, where it cannot displace a
tab you are already aiming at.

**Chats keep ordinal order.** `api.1` staying leftmost is a stronger promise for a handful of
numbered siblings than recency would be, and nothing about that changed.

## The tab you are on gets an edge

It was reverse video plus a `*`. It is reverse video plus an **underline** now — an
underline reads as the edge of a tab where a band reads as a highlighted word, and it costs
zero columns. Side glyphs (`▏name▕`) are the more literal border and cost two columns per
active tab, which is the opposite of what the same report asked for one field over.

The `*` is gone with it, and **what that costs is stated rather than waved past**: with
`NO_COLOR` set, on a console with no highlight to give, or with a pane redirected to a file,
every escape is stripped and the strip no longer says which tab is yours. `F2` answers that
at every width. #880 held the `*` for exactly this reason and #903 traded it.

**What the `*` did not buy back is its column.** The report expected one: drop the mark, free
a cell. Measured against the cut, that cannot be had — the pages are cut from the marked
fields, so an active tab one cell narrower than the same tab inactive makes the page
boundaries a function of *where the mark is*. Switching would then re-cut the strip and shift
every tab right of the mark one column left, which is precisely the double-press #767 exists
to stop. So the lead cell stays one blank on every tab, and one thing did come free from it:
**the chat you are typing in now shows its spinner**, where before the `*` had that cell and
a working chat you were standing in looked idle.

## The margins come in, and it is #880's fault

`some-workspace (99+)` reserved six columns on every workspace tab plus a two-cell gap, so
two tabs with no count sat **eight cells apart**. #880's own report predicted the objection —
*"the reserve is visibly wide… that is the constraint working as specified, but it is the
first thing an operator will remark on"* — and it was.

The field is `some-workspace 5` now, and `9+` past nine: three cells, so two tabs with no
count sit **five** apart. Ten open chats in one workspace is "a lot", not a number anyone
reads precisely, and at three cells a pair of brackets is two thirds of the field.

**The reserve itself is unchanged and that is the point.** Every tab still buys the field
whether or not it has a number in it, because a suffix that appeared when a chat opened and
vanished when one closed would move every tab to its right and put another workspace's name
under the cell you were about to press. What #880's report objected to was the price, not the
constraint. Those fifteen workspaces need **307** columns for one row now, where they needed
352.

## What that costs to read

`docs/assets/frame.svg` is a capture of this surface taken before these changes; the
freshness gate allows one minor of lag, so it is regenerated on the next pass rather than in
this one.
