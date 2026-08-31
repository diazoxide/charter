---
version: unreleased
headline: Switching workspace resizes the repo strip, instead of leaving it the size the workspace you left wanted
---

The repo strip's height is a function of its content. It was recomputed in exactly two
places — at launch, and on every terminal resize — and a workspace switch is neither, so
the pane kept the height of the workspace you left and the difference came out of the
agent's session:

```
▪ repos 2
  ├─ api                               main
  └─ ledger                            main
                                       <- five blank rows, taken from the harness
────────────────────────────────────────
0 todos · F2 palette
```

It cut the other way too. Switching from a one-clone workspace to an eight-clone one drew
the table into the pane the small one wanted, so real repos were replaced by `…(+2 more)`
on a terminal with room for all of them.

## The fix is a call, not a mechanism

`_reassert_sizes` already recomputes every height from the frame's current content and
applies it — it is what the `window-resized` hook has called on every step of a terminal
drag since heights stopped being constants. `switch.to_workspace` re-gathered the cache and
bumped the frame so the panels repaint, and never made that call. Now it does, between the
two: after the gather, because the height is computed from the count the gather writes, and
before the bump, because the bump is what makes a panel repaint and a panel that repaints
into a rectangle charter is about to change has drawn its table at the wrong height with
nothing left to make it draw again.

The strip itself is not what gets resized, and that is not a detail: it is the stack's
dependent pane, so what moves is the **harness**, and the strip takes the rows that leaves.
Asserting all of them is what once made the layout depend on which pane tmux happened to
resize first.

## The refusals come with it

A keypress can reach a frame in the states a resize hook can, so the same answers apply: a
harness pane that is not tmux's own `%N`, a frame with no panes recorded, and a window tmux
will not report a size for are each left alone, and the window is read again before
anything is applied so a layout computed for a size the window has already left is never
asserted. A switch whose re-size could not happen is still a switch that happened — the
strip is one size off, not the workspace unmoved.

## What was already right

A **chat** switch was reported as having the same defect. It does not: `cmd_chat` re-lays
the target chat out through the path that sizes from the *target's* own workspace, so the
strip already lands on its own number. Measured on a real server, and now pinned by a test
so a later change cannot quietly introduce it.
