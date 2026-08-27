---
version: unreleased
headline: A resize changes which panes a frame has, and a size measured for a window you have already left is not applied
---

Resize a running frame and it now ends up with the panes a launch at that size would have
drawn — in both directions.

Until now, which panels a frame *had* was decided once, at launch. Only the size of each
one was re-applied afterwards. So a frame started in a 200-column terminal and dragged
down to 80 kept a sidebar the same frame would have refused to draw had it started there
— 22 columns plus a border taken off a session that is now the narrow thing on screen —
and kept a repo table pane too narrow to draw a table in, which said so honestly and
could not do anything about it. Widen back and nothing came back that had not been there.
Getting the panes you had room for meant relaunching, or pressing `F2` and choosing a
density.

```
                                   drag 200 → 80              drag 80 → 200

before   ⬢ identity                ⬢ identity (cramped)       ⬢ identity
         │ session      │ chips    │ session     │ chips      │ session
         │ repo table   │          │ ⋯ too narrow│            │ session
         │ 0 todos · …  │          │ 0 todos · … │            │ 0 todos · …

now      ⬢ identity                ⬢ identity                 ⬢ identity
         │ session      │ chips    │ session                  │ session      │ chips
         │ repo table   │          │ session                  │ repo table   │
         │ 0 todos · …  │          │ 0 todos · …              │ 0 todos · …  │
```

**What you chose is never undone by a resize.** A panel you hid with its own key, or a
density you picked from the palette, stays hidden however you drag the terminal. Only the
size filter is re-run — the one that decides an 80-column terminal has no room for a
95-column table — and it is re-run over *your* arrangement, not over what
`charter.toml` says.

**Panes only move once the terminal has stopped.** A drag fires a resize event per size
change, and killing and re-splitting a pane at each step would start and kill a panel
process every time — each one a fresh interpreter and a first paint, and each one spending
one of the three respawn lives that pane gets. So a resize that would change the frame's
*shape* waits 400ms for the window to hold still and checks again; the one your drag
actually ended on is the one that acts. A resize that only changes sizes waits for nothing
and behaves exactly as it did before.

The 400ms is a floor with a measurement under it rather than a taste: re-laying a frame
out takes a measured 72ms of tmux commands on this machine before the new panels have even
started, so a shorter wait would let one crossing land inside the last one.

There is one thing a resize will not do: drop your *last* panel. Below half the size
floors charter would draw no panels at all, and a frame with no panels also has no resize
hook — the very thing that would notice you widening the terminal again. So at that size
the frame keeps what it has. `F2` still turns everything off, because a keypress can be
followed by another one.

## A size computed for a window you have already left is refused

Every resize event starts its own background charter, and nothing serialises them, so
during a drag several are in flight at once — each holding a measurement from a different
instant. One that measured a *taller* window computes a taller table pane; landing after
the one that measured the size you actually stopped at, it hands tmux a height for a
window that no longer exists. tmux does not refuse an over-large height. It grants it, and
takes the difference out of the neighbouring pane — which is your agent session (measured:
asking for 40 rows in a 20-row window left the session pane **one row tall**).

Each of those runs now re-reads the window immediately before it applies anything, and
does nothing at all if the size has moved since it measured. The newest measurement is the
only one that still matches, which is the only one worth applying — and it costs a re-read
rather than a lock file, so nothing is left behind when a run is killed mid-drag.

A window tmux will not report a size for is refused the same way. That used to fall back
to 80x24 and assert it, which is the same wrong move as applying a stale measurement with
less of an excuse; the next resize tries again.

## The table pane's width comes from tmux now, not from charter's arithmetic

How wide the repo table's pane is depends on what was split before it: a `slots` list
naming `right` first insets it by the sidebar's 23 columns, so a 110-column window has an
87-column table pane. Charter worked that out from the order the panes were recorded in —
a file in the frame's own state directory. That is right until the file and the window
disagree, and nothing inside charter can tell when they do.

tmux knows. It is asked, and its answer wins; the arithmetic stays as the answer for a
pane that cannot be asked (at launch, the pane does not exist yet). The order matters and
was measured: tmux stretches every pane proportionally when a window resizes, so the
sidebar has to be put back to its own width *before* the pane beside it is measured — on a
120x40 frame grown to 200x40, the sidebar came back 62 columns wide and the table pane
read 137, where the truth one correction later is 177.

## A panel that fails no longer lays itself out from your shell

A panel that cannot start paints the reason into its own pane and stays alive so you can
read it. It measured its own pane for that, and fell back to `$COLUMNS` when there was no
pane to measure — but `$COLUMNS` describes the terminal charter was *launched* from, which
a panel inherits whole and which has nothing to do with the rectangle it is drawing in
(measured: a 22-column pane whose launcher had exported `COLUMNS=200` sees `200`). So on a
wide terminal, the one message that exists to explain a failure could be painted wider
than the pane holding it, and wrap itself out of view.

A pane charter cannot measure now gets a plain 80 columns, exactly as it has always got a
plain 24 rows. Nothing about a panel's size comes out of your shell any more.

## What to do

Nothing. If you never resize your terminal you will not notice any of this; if you do, the
frame follows.
