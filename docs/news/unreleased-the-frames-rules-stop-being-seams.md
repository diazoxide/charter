---
version: unreleased
headline: The frame's rules stop being seams between the panels
---

A plane that painted all four panels — `chrome = "dark"` with `bg = "brightblack"` on every
component — got four grey rectangles with a **one-cell dark strip running between every pair
of them**, and around the sidebar. Reported off a screenshot: the frame read as coloured
boxes separated by seams rather than as one application.

The cause is a boundary, not a colour. `window-style` fills a **pane's** rectangle, and the
rule tmux draws between two panes is in neither rectangle — it comes from
`pane-border-style`, which charter has pinned to `fg=default,dim` since the frame started
drawing its own chrome. An `fg` and an attribute, and no background at all, so the cell
stayed whatever your terminal already was. Read off an attached client's wire, through a
nested tmux, three panes each `bg = "brightblack"`:

```
before  '\x1b[100m … \x1b[2m\x1b[49m│\x1b[0m\x1b[100m'   <- ESC[49m: the seam
after   '\x1b[100m … \x1b[2m│\x1b[0m\x1b[100m'            <- the surface runs through
```

Charter now puts a background behind the frame's rules as well, and **you configure
nothing** — it is derived from the colours you already wrote.

**Which colour, when a rule has a different pane on each side.** There is no "panel side" to
match: tmux draws one border, not two half-borders. A gutter in *no* colour between two
identically-coloured panels is a seam, and that is the whole report. A plane that never set a
`bg` sees the rules take the `chrome` colour, which is what every pane in it was already
wearing — and a plane at `chrome = "off"` with no `bg` anywhere gets exactly the frame it got
before, byte for byte.

**Below tmux 3.7, one colour is all charter can say, and it says the one the panels agree
on** — the frame-wide `chrome` surface where they name different ones. That is
`instance.border_bg`, and it is now the *floor's* answer rather than the frame's: two later
changes in this same release replaced it above the floor, where tmux resolves each border
cell against one pane and can therefore be told more than one thing. Each panel's edges take
that panel's own surface (*Each panel's edges are drawn in that panel's own colour*), and the
three rules that run round your harness take the surface every panel agrees on, or nothing at
all when they do not (*One rule, one colour — including the three that run round your own
session*). Read those two for what a 3.7 frame actually draws. What holds at every version
is the property, not the value: no rule cell is left in a colour no pane beside it wears.

**One rule colour and not two.** tmux draws the active pane's border from a second option
whose default is a format expression (`#{?pane_in_mode,fg=yellow,…}`), and letting the two
differ is the defect that put charter in charge of these options in the first place: a rule
that changes colour halfway along, where it passes the active pane's corner. A *background*
that changed there would be that defect an order of magnitude more visible, so both options
get the same value or neither does. Which pane is live is still shown on the pane itself —
its background is one shade off the others — never on the border.

**Window-scoped below the floor, and that was measured rather than assumed.**
`pane-border-style` is a pane option on tmux 3.7c, where tmux draws each border cell from the
pane above or left of it and ignores the other side's outright. At charter's own floor, tmux
3.2, it is not a pane option at all: `set -p` returns 0 but writes the **window's** value, and
`set -p -u` would remove charter's own pin for the whole window. So per-side border colours
are unavailable below 3.7, silently window-wide there, and one-sided where they exist — which
is the line the two entries named above are drawn along. Both border options accept the
combined value and read it back verbatim on 3.7c and on 3.2 alike, and a tmux that refuses
one of these settings is reported and not fatal, as it already was.

`NO_COLOR` takes the background off the rules and leaves the rules drawn: the dim
foreground is an attribute over your own palette rather than a colour charter chose, and a
background is exactly what that variable is about — including one charter asked tmux to
paint on its behalf.

`F2`'s `chrome:` rows and `charter frame-chrome <level>` repaint the rules along with the
panes, so a live change cannot leave grey borders around panes that just went back to your
terminal's colour, or the seam running between panes that just went grey. As before, a
`chrome` level never erases a colour a component wrote.

Nothing to adopt: upgrading is the whole of it.
