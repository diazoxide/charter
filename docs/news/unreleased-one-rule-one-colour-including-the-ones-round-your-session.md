---
version: unreleased
headline: One rule, one colour — including the three that run round your own session
---

Two releases ago the frame's rules got a background so a surfaced frame stopped showing a
one-cell seam between its panels. The release after that took the background back off the
three rules that touch the pane your harness runs in, because a grey outline round your own
session was reported off a screenshot. **That trade was the wrong way round, and this puts
it back — without painting anything inside your session.**

## What the second answer actually rendered

Above tmux 3.7 a border cell is drawn from exactly one pane's options: tmux walks the
window's panes in order and takes the first whose border box contains the cell. The harness
is the first pane charter's window has, so it owns *every* cell along its own top, right and
bottom — including the part of the horizontal rule that runs underneath the identity bar,
all the way to the harness's corner. The sidebar owns the rest of that same row.

Leaving the harness's two options unset therefore did not give the harness dark edges. It
gave one horizontal rule two colours. Read off an attached client's wire through a nested
tmux, charter's real four-panel shape at 100x24, every panel `bg = "brightblack"`:

```
before  row 1: cols 0-77 ESC[49m  cols 78-99 ESC[100m   <- one rule, two colours
after   row 1: cols 0-99 ESC[100m                       <- one rule, one colour
```

which is the same defect that put charter in charge of these options in the first place: a
rule that changes colour where it passes a corner.

## The line that matters is the pane, not the cell beside it

Charter has drawn those three rules' foreground, their dim attribute, their line weight,
their indicators and their border-status since it started drawing its own chrome — they are
window options, set on your harness's window, and nobody has ever called that a boundary
crossing. Withholding only their *background* was half a cell, and the half it left behind
is what you were looking at.

What charter still never touches is the **inside** of that pane. `window-style` and
`window-active-style` are what paint a pane's rectangle, and neither is among the two
options the rules are made of — so the harness keeps your terminal's own background at every
`chrome` level, on both servers, and `show -p -t <your pane> -v window-style` still answers
nothing at all. That is the line, and it is asserted rather than asserted-about: the tests
read both halves back off tmux, on a live 3.7c, in the same breath.

## Which colour, when your panels are not all one colour

The rules round your session take **the surface every panel agrees on, and nothing at all
when they do not**. Every other rule in the frame wears the colour of a pane it touches —
a panel's own edges take that panel's colour — and the harness has a pane charter never
paints on one side of it, so its edges have to match the panel on the other. Where your
components name different backgrounds there is no single colour all three of its neighbours
share, and any compromise would be a cell matching neither pane beside it, which is the seam
this all started with. So there the rules stay bare.

Nothing to configure: it is derived from the colours you already wrote. `chrome = "off"`
removes it, `NO_COLOR` refuses it, and on tmux 3.2 to 3.6 — where these options have no pane
scope and `set -p` silently writes the window — nothing pane-scoped is written at all and
the frame-wide answer is used exactly as it was.

Nothing to adopt: upgrading is the whole of it.
